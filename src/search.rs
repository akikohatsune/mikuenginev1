use crate::board::Board;
use crate::history::{Heuristics, MAX_PLY};
use crate::movepick::MovePicker;
use crate::transposition::{NodeType, TranspositionTable};
use crate::types::{Move, PieceType, Square};
use crate::time::TimeManager;
use std::time::Instant;

const INF: i32 = 50000;
const MATE_SCORE: i32 = 48000;
const TB_SCORE: i32 = 46000; // TB win score margin

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::smp::SharedState;

#[derive(Clone)]
pub struct MikuAdapter;

impl pyrrhic_rs::EngineAdapter for MikuAdapter {
    fn pawn_attacks(color: pyrrhic_rs::Color, square: u64) -> u64 {
        let bitboard = 1u64 << square;
        if color == pyrrhic_rs::Color::White {
            let mut attacks = 0;
            if (bitboard & !crate::bitboard::FILE_A) != 0 {
                attacks |= bitboard << 7;
            }
            if (bitboard & !crate::bitboard::FILE_H) != 0 {
                attacks |= bitboard << 9;
            }
            attacks
        } else {
            let mut attacks = 0;
            if (bitboard & !crate::bitboard::FILE_H) != 0 {
                attacks |= bitboard >> 7;
            }
            if (bitboard & !crate::bitboard::FILE_A) != 0 {
                attacks |= bitboard >> 9;
            }
            attacks
        }
    }

    fn knight_attacks(square: u64) -> u64 {
        crate::attacks::knight_attacks(crate::types::Square::new(square as u8)).0
    }

    fn bishop_attacks(square: u64, occupied: u64) -> u64 {
        crate::attacks::bishop_attacks(crate::types::Square::new(square as u8), crate::bitboard::Bitboard(occupied)).0
    }

    fn rook_attacks(square: u64, occupied: u64) -> u64 {
        crate::attacks::rook_attacks(crate::types::Square::new(square as u8), crate::bitboard::Bitboard(occupied)).0
    }

    fn queen_attacks(square: u64, occupied: u64) -> u64 {
        crate::attacks::queen_attacks(crate::types::Square::new(square as u8), crate::bitboard::Bitboard(occupied)).0
    }

    fn king_attacks(square: u64) -> u64 {
        crate::attacks::king_attacks(crate::types::Square::new(square as u8)).0
    }
}

pub struct Search {
    pub tt: Arc<TranspositionTable>,
    pub smp: Arc<SharedState>,
    pub heuristics: Heuristics,
    pub nodes: u64,
    pub start_time: Instant,
    pub timer: TimeManager,
    pub stop: Arc<AtomicBool>,
    pub thread_id: usize,
    pub best_move: Move,
    pub best_move_nodes: u64,
    pub prev_best_score: i32,
    pub pv_table: [[Move; MAX_PLY]; MAX_PLY],
    pub pv_length: [usize; MAX_PLY],
    pub lmr_table: [[u8; 64]; 64],
    // Stockfish-style integer reductions table: reductions[i] = int(2747/128 * ln(i))
    pub reductions: [i32; 64],
    // NMP minimum ply for verification search
    pub nmp_min_ply: usize,
}

/// Per-node state threaded through the search (equivalent to Stockfish's Stack)
#[derive(Clone, Copy)]
pub struct SearchStack {
    pub static_eval: i32,
    pub tt_pv: bool,
    pub in_check: bool,
    pub cutoff_cnt: i32,
    pub reduction: i32,
    pub stat_score: i32,
    pub move_count: usize,
    pub current_move: Option<Move>,
    pub excluded_move: Option<Move>,
}

impl SearchStack {
    pub const NONE_EVAL: i32 = i32::MIN;
    pub fn new() -> Self {
        SearchStack {
            static_eval: Self::NONE_EVAL,
            tt_pv: false,
            in_check: false,
            cutoff_cnt: 0,
            reduction: 0,
            stat_score: 0,
            move_count: 0,
            current_move: None,
            excluded_move: None,
        }
    }
}

impl Search {
    pub fn new(smp: Arc<SharedState>, thread_id: usize) -> Self {
        let mut lmr_table = [[0u8; 64]; 64];
        for d in 1..64 {
            for m in 1..64 {
                let reduction = 0.75 + (d as f64).ln() * (m as f64).ln() / 2.25;
                lmr_table[d][m] = reduction as u8;
            }
        }
        // Stockfish integer reductions: reductions[i] = int(2747/128.0 * ln(i))
        let mut reductions = [0i32; 64];
        for i in 1..64 {
            reductions[i] = ((2747.0 / 128.0) * (i as f64).ln()) as i32;
        }

        Search {
            tt: smp.tt.clone(),
            smp: smp.clone(),
            heuristics: Heuristics::new(),
            nodes: 0,
            start_time: Instant::now(),
            timer: TimeManager::new(),
            stop: smp.stop_flag.clone(),
            thread_id,
            best_move: Move::new(0, 0, 0),
            best_move_nodes: 0,
            prev_best_score: 0,
            pv_table: [[Move::new(0, 0, 0); MAX_PLY]; MAX_PLY],
            pv_length: [0; MAX_PLY],
            lmr_table,
            reductions,
            nmp_min_ply: 0,
        }
    }

    pub fn check_time(&mut self) {
        if self.nodes.is_multiple_of(2048) {
            if self.thread_id == 0 && self.timer.should_stop() {
                self.stop.store(true, Ordering::Relaxed);
            }
        }
    }

    // ------------------------------------------------------------------
    //   QUIESCENCE SEARCH
    // ------------------------------------------------------------------
    pub fn quiescence(
        &mut self,
        board: &mut Board,
        ss: &mut [SearchStack],
        ply: usize,
        mut alpha: i32,
        beta: i32,
    ) -> i32 {
        if ply >= MAX_PLY - 1 {
            return crate::eval::endgame_evaluate(board, board.nnue.evaluate(board.side_to_move, &board.accumulator));
        }
        self.nodes += 1;
        self.check_time();
        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        let stand_pat = crate::eval::endgame_evaluate(board, board.nnue.evaluate(board.side_to_move, &board.accumulator));
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        let mut picker = MovePicker::new(None, 0, None, true, false, self.thread_id, None);

        while let Some(m) = picker.next(&self.heuristics, board) {
            let is_capture = (m.0 & 0x4000) != 0;
            let is_promo = (m.0 >> 12 & 0x3) != 0;

            if !is_capture && !is_promo {
                continue;
            }

            // Delta Pruning
            if is_capture && !is_promo {
                let to_sq = m.to_sq();
                let margin = 200;

                if let Some(victim) = board.piece_on_sq[to_sq as usize] {
                    let victim_val = crate::eval::PIECE_VALUES[victim.piece_type() as usize];
                    if stand_pat + victim_val + margin < alpha {
                        continue;
                    }
                } else if (m.0 & 0x8000) != 0
                    && stand_pat + crate::eval::PAWN_VALUE + margin < alpha {
                        continue;
                    }
            }

            // SEE Pruning
            if is_capture && !board.see_ge(m, 0) {
                continue;
            }

            if !board.is_castling_legal(m) {
                continue;
            }

            let undo = board.make_move(m);

            let side = board.side_to_move.flip();
            let king_sq_opt = board.piece_bb(PieceType::King) & board.color_occupancy(side);
            let in_check = if king_sq_opt.is_not_empty() {
                let king_sq = Square::new(king_sq_opt.lsb());
                board.is_square_attacked(king_sq, board.side_to_move)
            } else {
                false
            };

            if !in_check {
                let score = -self.quiescence(board, ss, ply + 1, -beta, -alpha);
                board.unmake_move(m, &undo);
                if score >= beta {
                    return beta;
                }
                if score > alpha {
                    alpha = score;
                }
            } else {
                board.unmake_move(m, &undo);
            }
        }

        alpha
    }

    // ------------------------------------------------------------------
    //   FAIL-SOFT ALPHA-BETA WITH PVS  (Stockfish-aligned)
    // ------------------------------------------------------------------
    #[inline(never)]
    pub fn alpha_beta(
        &mut self,
        board: &mut Board,
        ss: &mut [SearchStack],   // ss[ply] is current node
        ply: usize,
        mut depth: u8,
        mut alpha: i32,
        mut beta: i32,
        cut_node: bool,
    ) -> i32 {
        if ply >= MAX_PLY - 1 {
            return crate::eval::endgame_evaluate(board, board.nnue.evaluate(board.side_to_move, &board.accumulator));
        }

        let true_hash = board.compute_hash();
        if board.zobrist_key != true_hash {
            eprintln!(
                "HASH DRIFT IN NEGAMAX! inc={:x}, true={:x}, ply={}",
                board.zobrist_key, true_hash, ply
            );
            std::process::exit(1);
        }

        self.nodes += 1;
        self.check_time();
        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        // Initialise PV length
        self.pv_length[ply] = ply;

        let pv_node = beta - alpha > 1;
        let root_node = ply == 0;

        // Step 2. Draw detection
        let draw_score = -1 + (self.nodes & 2) as i32;
        if board.halfmove_clock >= 100 {
            return draw_score;
        }
        if ply > 0 && board.is_repetition() {
            return draw_score;
        }

        // Step 3. Mate distance pruning
        if !root_node {
            alpha = alpha.max(-MATE_SCORE + ply as i32);
            beta  = beta.min(MATE_SCORE - ply as i32 - 1);
            if alpha >= beta {
                return alpha;
            }
        }

        // Step 3.5. Tablebase Node Probing (WDL)
        if ply > 0 && board.halfmove_clock == 0 && !root_node {
            if let Some(tb) = &self.smp.tb {
                let pieces_count = board.occupancies().count() as u32;
                if pieces_count <= tb.max_pieces() {
                    let w_bb = board.color_occupancy(crate::types::Color::White).0;
                    let b_bb = board.color_occupancy(crate::types::Color::Black).0;
                    let k_bb = board.piece_bb(PieceType::King).0;
                    let q_bb = board.piece_bb(PieceType::Queen).0;
                    let r_bb = board.piece_bb(PieceType::Rook).0;
                    let b_bb_pc = board.piece_bb(PieceType::Bishop).0;
                    let n_bb = board.piece_bb(PieceType::Knight).0;
                    let p_bb = board.piece_bb(PieceType::Pawn).0;
                    let ep = board.en_passant.map_or(0, |sq| sq.0 as u32);
                    let turn = board.side_to_move == crate::types::Color::White;

                    if let Ok(wdl) = tb.probe_wdl(w_bb, b_bb, k_bb, q_bb, r_bb, b_bb_pc, n_bb, p_bb, ep, turn) {
                        let mut tb_value = match wdl {
                            pyrrhic_rs::WdlProbeResult::Win => TB_SCORE - ply as i32,
                            pyrrhic_rs::WdlProbeResult::Loss => -TB_SCORE + ply as i32,
                            pyrrhic_rs::WdlProbeResult::Draw | pyrrhic_rs::WdlProbeResult::BlessedLoss | pyrrhic_rs::WdlProbeResult::CursedWin => draw_score,
                        };

                        let tb_bound = if tb_value > draw_score {
                            NodeType::Exact // Win
                        } else if tb_value < draw_score {
                            NodeType::Exact // Loss
                        } else {
                            NodeType::Exact // Draw
                        };

                        if tb_bound == NodeType::Exact || (tb_bound == NodeType::Beta && tb_value >= beta) || (tb_bound == NodeType::Alpha && tb_value <= alpha) {
                            self.tt.store(board.zobrist_key, 0, tb_value, tb_bound, Move::none(), pv_node);
                            return tb_value;
                        }
                    }
                }
            }
        }

        // Initialise ss[ply] for this node
        ss[ply].cutoff_cnt = 0;
        if ply + 2 < MAX_PLY { ss[ply + 2].cutoff_cnt = 0; }

        let orig_alpha = alpha;
        let excluded_move = ss[ply].excluded_move;

        // ------------------------------------------------------------------
        // Step 4. TT Probe
        // ------------------------------------------------------------------
        let mut tt_move: Option<Move> = None;
        let mut tt_score = 0i32;
        let mut tt_depth = 0u8;
        let mut tt_node_type = NodeType::None;
        let mut tt_hit = false;

        if excluded_move.is_none() {
            if let Some(entry) = self.tt.probe(board.zobrist_key) {
                tt_move = Some(entry.best_move);
                tt_score = entry.score;
                tt_depth = entry.depth;
                tt_node_type = entry.node_type;
                tt_hit = true;
            }
        }

        // ttPv: this node is on a known PV path
        let tt_is_pv = tt_hit && matches!(tt_node_type, NodeType::Exact);
        ss[ply].tt_pv = pv_node || tt_is_pv;

        // TT cutoff
        if !pv_node && excluded_move.is_none() && tt_hit && tt_depth >= depth {
            match tt_node_type {
                NodeType::Exact => return tt_score,
                NodeType::Alpha if tt_score <= alpha => return alpha,
                NodeType::Beta  if tt_score >= beta  => return beta,
                _ => {}
            }
        }

        // Depth-0: drop into qsearch
        if depth == 0 {
            return self.quiescence(board, ss, ply, alpha, beta);
        }

        // ------------------------------------------------------------------
        // Step 6. Static evaluation
        // ------------------------------------------------------------------
        let side = board.side_to_move;
        let king_bb = board.piece_bb(PieceType::King) & board.color_occupancy(side);
        let in_check = if king_bb.is_not_empty() {
            let ksq = Square::new(king_bb.lsb());
            board.is_square_attacked(ksq, side.flip())
        } else { false };

        ss[ply].in_check = in_check;

        // Check Extension
        if in_check { depth += 1; }

        let correction_value = {
            let mat_hash = board.non_pawn_material(side) as usize;
            self.heuristics.get_non_pawn_correction(side, mat_hash)
        };

        let eval = if in_check {
            ss[ply].static_eval = SearchStack::NONE_EVAL;
            -INF
        } else if tt_hit {
            let raw = crate::eval::endgame_evaluate(board, board.nnue.evaluate(side, &board.accumulator));
            let corrected = (raw + correction_value / 131072).clamp(-MATE_SCORE + 1, MATE_SCORE - 1);
            ss[ply].static_eval = corrected;
            // Use tt_score as better eval if bound agrees
            if (tt_node_type == NodeType::Beta  && tt_score > corrected)
            || (tt_node_type == NodeType::Alpha && tt_score < corrected) {
                tt_score
            } else { corrected }
        } else {
            let raw = crate::eval::endgame_evaluate(board, board.nnue.evaluate(side, &board.accumulator));
            let corrected = (raw + correction_value / 131072).clamp(-MATE_SCORE + 1, MATE_SCORE - 1);
            ss[ply].static_eval = corrected;
            corrected
        };

        // Improving: static eval better than 2 plies ago
        let improving = !in_check && ply >= 2
            && ss[ply].static_eval != SearchStack::NONE_EVAL
            && ss[ply - 2].static_eval != SearchStack::NONE_EVAL
            && ss[ply].static_eval > ss[ply - 2].static_eval;

        // Opponent worsening: our eval better than opponent's last eval
        let opp_worsening = !in_check && ply >= 1
            && ss[ply].static_eval != SearchStack::NONE_EVAL
            && ss[ply - 1].static_eval != SearchStack::NONE_EVAL
            && ss[ply].static_eval > -ss[ply - 1].static_eval;

        // Step 6b. Hindsight reduction adjustment
        let prior_reduction = if ply > 0 { ss[ply - 1].reduction } else { 0 };
        if prior_reduction >= 3 && !opp_worsening { depth = depth.saturating_add(1); }
        if prior_reduction >= 2 && depth >= 2 && !in_check && ply >= 1
            && ss[ply].static_eval != SearchStack::NONE_EVAL
            && ss[ply - 1].static_eval != SearchStack::NONE_EVAL
            && ss[ply].static_eval + ss[ply - 1].static_eval > 173
        {
            depth = depth.saturating_sub(1);
        }

        let has_non_pawn = (board.colors[side as usize]
            & !(board.pieces[PieceType::Pawn as usize] | board.pieces[PieceType::King as usize]))
            .is_not_empty();

        // ------------------------------------------------------------------
        //   PRE-MOVES PRUNING (skip if in check)
        // ------------------------------------------------------------------
        if !in_check && excluded_move.is_none() {

            // Step 7. Razoring — Stockfish formula
            if !pv_node && eval < alpha - 485 - 281 * (depth as i32) * (depth as i32) {
                return self.quiescence(board, ss, ply, alpha, beta);
            }

            // Step 8. Futility pruning (Reverse Null Move / static NMP)
            {
                let futility_mult = 76 - 23 * if tt_hit { 0 } else { 1 };
                let fu_margin = futility_mult * depth as i32
                    - (2474 * improving as i32 + 331 * opp_worsening as i32)
                        * futility_mult / 1024
                    + correction_value.abs() / 174665;
                if !ss[ply].tt_pv && (depth as i32) < 14
                    && eval - fu_margin >= beta
                    && eval >= beta
                    && eval < MATE_SCORE - 100
                    && beta > -MATE_SCORE + 100
                {
                    return (2 * beta + eval) / 3;
                }
            }

            // Step 9. Null Move Pruning
            if cut_node && eval >= beta - 18 * depth as i32 + 350
                && has_non_pawn && ply >= self.nmp_min_ply
                && beta > -MATE_SCORE + 100
            {
                let r = 7 + depth as usize / 3;
                let null_depth = depth.saturating_sub(r as u8);

                ss[ply].current_move = None;
                let undo = board.make_null_move();
                let null_score = -self.alpha_beta(
                    board, ss, ply + 1, null_depth, -beta, -beta + 1, false,
                );
                board.unmake_null_move(&undo);

                if self.stop.load(Ordering::Relaxed) { return 0; }

                if null_score >= beta && null_score < MATE_SCORE - 100 {
                    if self.nmp_min_ply > 0 || (depth as usize) < 16 {
                        return null_score;
                    }
                    // Verification search at high depth
                    self.nmp_min_ply = ply + 3 * (depth as usize - r as usize) / 4;
                    let v = self.alpha_beta(board, ss, ply, depth.saturating_sub(r as u8),
                        beta - 1, beta, false);
                    self.nmp_min_ply = 0;
                    if v >= beta { return null_score; }
                }
            }

            // Step 10. IIR — simply reduce depth, no recursive call
            let all_node = !pv_node && !cut_node;
            if !all_node && depth >= 6 && tt_move.is_none() && prior_reduction <= 3 {
                depth = depth.saturating_sub(1);
            }

            // Step 11. Full ProbCut
            let prob_cut_beta = beta + 235 - 63 * improving as i32;
            if depth >= 3
                && beta.abs() < MATE_SCORE - 100
                && !(tt_hit && tt_depth >= depth.saturating_sub(4) && tt_score < prob_cut_beta)
            {
                // Only try captures with SEE >= probCutBeta - staticEval
                let mut pc_picker = MovePicker::new(tt_move, ply, None, true, in_check, self.thread_id, None);
                while let Some(pc_move) = pc_picker.next(&self.heuristics, board) {
                    if !pc_move.is_capture() { continue; }
                    if Some(pc_move) == excluded_move { continue; }
                    if !board.see_ge(pc_move, prob_cut_beta - eval) { continue; }
                    if !board.is_castling_legal(pc_move) { continue; }

                    let pc_undo = board.make_move(pc_move);
                    let move_side = board.side_to_move.flip();
                    let mk = board.piece_bb(PieceType::King) & board.color_occupancy(move_side);
                    let legal = if mk.is_not_empty() {
                        !board.is_square_attacked(Square::new(mk.lsb()), board.side_to_move)
                    } else { true };

                    if legal {
                        ss[ply].current_move = Some(pc_move);
                        let mut pc_val = -self.quiescence(board, ss, ply + 1, -prob_cut_beta, -prob_cut_beta + 1);

                        if pc_val >= prob_cut_beta {
                            let pc_depth = depth.saturating_sub(5);
                            if pc_depth > 0 {
                                pc_val = -self.alpha_beta(board, ss, ply + 1, pc_depth,
                                    -prob_cut_beta, -prob_cut_beta + 1, !cut_node);
                            }
                        }
                        board.unmake_move(pc_move, &pc_undo);

                        if pc_val >= prob_cut_beta {
                            self.tt.store(board.zobrist_key, depth.saturating_sub(4), pc_val,
                                NodeType::Beta, pc_move, pv_node);
                            if pc_val < MATE_SCORE - 100 {
                                return pc_val - (prob_cut_beta - beta);
                            }
                        }
                    } else {
                        board.unmake_move(pc_move, &pc_undo);
                    }
                }
            }
        }

        // Step 12. Small TT-based ProbCut (after moves_loop label in Stockfish)
        {
            let prob_cut_beta2 = beta + 418;
            if tt_hit && tt_depth >= depth.saturating_sub(4)
                && tt_score >= prob_cut_beta2
                && matches!(tt_node_type, NodeType::Beta)
                && beta.abs() < MATE_SCORE - 100
                && tt_score.abs() < MATE_SCORE - 100
            {
                return prob_cut_beta2;
            }
        }

        let is_shuffling = board.halfmove_clock >= 20;

        // ------------------------------------------------------------------
        //   MOVE LOOP SETUP
        // ------------------------------------------------------------------
        let prev_move = if ply > 0 { ss[ply - 1].current_move } else { None };

        let countermove = if let Some(pm) = prev_move {
            if let Some(prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                let cm = self.heuristics.get_countermove(
                    prev_piece.piece_type(), Square::new(pm.to_sq()));
                if cm.0 != 0 { Some(cm) } else { None }
            } else { None }
        } else { None };

        let global_pv = if ply == 0 {
            let pv_val = self.smp.get_best_move();
            if pv_val != 0 { Some(Move(pv_val)) } else { None }
        } else { None };

        let mut picker = MovePicker::new(tt_move, ply, countermove, false, in_check, self.thread_id, global_pv);

        let mut legal_moves = 0usize;
        let mut best_m = Move::new(0, 0, 0);
        let mut best_score = -INF;
        let mut quiets_searched: [Move; 64] = [Move::new(0, 0, 0); 64];
        let mut quiet_count = 0usize;
        let mut first_move = true;
        let mut cur_depth = depth; // depth may change in loop via alpha-improvement reduction

        while let Some(m) = picker.next(&self.heuristics, board) {
            if Some(m) == excluded_move { continue; }

            let start_nodes = self.nodes;
            let is_capture = m.is_capture();
            let is_promo = m.is_promotion();

            let attacker_pt = board.piece_on_sq[m.from_sq() as usize]
                .map(|p| p.piece_type())
                .unwrap_or(PieceType::Pawn);

            // --- Pre-make-move pruning (Stockfish Step 14) ---
            if !root_node && has_non_pawn && best_score > -MATE_SCORE + 100 {
                // LMP
                let lmp_threshold = (3 + cur_depth as usize * cur_depth as usize) / (2 - improving as usize);
                if legal_moves >= lmp_threshold { picker.skip_quiets(); }

                // Compute LMR depth for pruning checks
                let lmr_d = (cur_depth as i32 - 1).max(0)
                    - (self.reductions.get(legal_moves.min(63)).copied().unwrap_or(0) / 1024);
                let lmr_depth = lmr_d.max(0);

                if is_capture || (is_promo && !is_capture) {
                    // Capture futility pruning (Stockfish formula)
                    let victim_pt = if m.flag() == Move::FLAG_EP { PieceType::Pawn }
                        else { board.piece_on_sq[m.to_sq() as usize]
                            .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn) };
                    let capt_hist = self.heuristics.get_capture_history(
                        attacker_pt, Square::new(m.to_sq()), victim_pt);
                    if lmr_depth < 7 {
                        let fv = eval + 232 + 217 * lmr_depth
                            + crate::eval::PIECE_VALUES[victim_pt as usize]
                            + 131 * capt_hist / 1024;
                        if fv <= alpha { continue; }
                    }
                    // SEE pruning captures
                    let margin = (166 * cur_depth as i32 + capt_hist / 29).max(0);
                    if !board.see_ge(m, -margin) { continue; }
                } else {
                    // Continuation history pruning for quiets
                    let cont_hist = if let Some(pm) = prev_move {
                        if let Some(pp) = board.piece_on_sq[pm.to_sq() as usize] {
                            self.heuristics.get_continuation(
                                pp.piece_type(), Square::new(pm.to_sq()),
                                attacker_pt, Square::new(m.to_sq()))
                        } else { 0 }
                    } else { 0 };
                    if cont_hist < -4083 * cur_depth as i32 { continue; }

                    let history_score = self.heuristics.get_history(side, attacker_pt, Square::new(m.to_sq()));
                    let lmr_adj = lmr_depth + (history_score + 69 * cont_hist / 32 + cont_hist) / 3208;
                    // Quiet futility pruning
                    let fv = eval + 42 + 161 * (best_m.0 == 0) as i32
                        + 127 * lmr_adj + 85 * (eval > alpha) as i32;
                    if !in_check && lmr_adj < 13 && fv <= alpha {
                        if best_score <= fv && best_score < MATE_SCORE - 100 { best_score = fv; }
                        continue;
                    }
                    // SEE for quiets
                    if !board.see_ge(m, -25 * lmr_adj * lmr_adj) { continue; }
                }
            }

            if !board.is_castling_legal(m) { continue; }

            let undo = board.make_move(m);
            if board.zobrist_key != board.compute_hash() {
                eprintln!("HASH DRIFT AFTER MAKE_MOVE! ply={}", ply);
                std::process::exit(1);
            }

            let move_side = board.side_to_move.flip();
            let mk = board.piece_bb(PieceType::King) & board.color_occupancy(move_side);
            let move_is_legal = if mk.is_not_empty() {
                !board.is_square_attacked(Square::new(mk.lsb()), board.side_to_move)
            } else { true };

            if !move_is_legal {
                board.unmake_move(m, &undo);
                continue;
            }

            legal_moves += 1;
            ss[ply].current_move = Some(m);
            ss[ply].move_count = legal_moves;

            let gives_check = {
                let opp = board.side_to_move;
                let opp_kb = board.piece_bb(PieceType::King) & board.color_occupancy(opp);
                opp_kb.is_not_empty()
                    && board.is_square_attacked(Square::new(opp_kb.lsb()), opp.flip())
            };

            // ------------------------------------------------------------------
            // Step 15. Singular Extension (Stockfish)
            // ------------------------------------------------------------------
            let mut extension: i32 = 0;
            if !root_node && !is_shuffling {
                if let Some(ttm) = tt_move {
                    if m.0 == ttm.0
                        && excluded_move.is_none()
                        && cur_depth >= 6 + ss[ply].tt_pv as u8
                        && tt_score.abs() < MATE_SCORE - 100
                        && tt_depth >= cur_depth.saturating_sub(3)
                        && matches!(tt_node_type, NodeType::Beta)
                    {
                        let sing_beta = tt_score - (53 + 75 * (ss[ply].tt_pv && !pv_node) as i32)
                            * cur_depth as i32 / 60;
                        let sing_depth = (cur_depth - 1) / 2;

                        board.unmake_move(m, &undo);
                        ss[ply].excluded_move = Some(m);
                        let se_score = self.alpha_beta(
                            board, ss, ply, sing_depth, sing_beta - 1, sing_beta, cut_node);
                        ss[ply].excluded_move = None;
                        let undo2 = board.make_move(m);

                        if se_score < sing_beta {
                            // Singular: compute double/triple extension margins
                            let corr_adj = correction_value.abs() / 230673;
                            let double_margin = -4 + 199 * pv_node as i32
                                - 201 * !tt_move.map(|t| board.piece_on_sq[t.to_sq() as usize].is_some()).unwrap_or(false) as i32
                                - corr_adj;
                            let triple_margin = 73 + 302 * pv_node as i32 - 248 + 90 * ss[ply].tt_pv as i32 - corr_adj;

                            extension = 1
                                + (se_score < sing_beta - double_margin) as i32
                                + (se_score < sing_beta - triple_margin) as i32;
                            cur_depth += 1; // Stockfish increments depth when singular
                        } else if se_score >= beta && se_score < MATE_SCORE - 100 {
                            // Multi-cut: update ttMoveHistory and prune
                            self.heuristics.update_tt_move_history((-400 - 100 * cur_depth as i32).max(-4000));
                            board.unmake_move(m, &undo2);
                            return se_score;
                        } else if tt_score >= beta {
                            extension = -3;
                        } else if cut_node {
                            extension = -2;
                        }
                    }
                }

                // Promotion extension
                if is_promo { extension = extension.max(1); }

                // Pawn push to 7th rank extension
                if attacker_pt == PieceType::Pawn && !is_promo {
                    let to_rank = Square::new(m.to_sq()).rank();
                    if (move_side == crate::types::Color::White && to_rank == 6) || (move_side == crate::types::Color::Black && to_rank == 1) {
                        extension = extension.max(1);
                    }
                }
            }

            let new_depth = (cur_depth as i32 - 1 + extension).max(0) as u8;

            // Stat score for LMR
            let stat_score = if is_capture {
                let victim_pt = board.piece_on_sq[m.to_sq() as usize]
                    .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn);
                868 * crate::eval::PIECE_VALUES[victim_pt as usize] / 128
                    + self.heuristics.get_capture_history(attacker_pt, Square::new(m.to_sq()), victim_pt)
            } else {
                let mut ss_val = 2 * self.heuristics.get_history(side, attacker_pt, Square::new(m.to_sq()));
                if let Some(pm) = prev_move {
                    if let Some(pp) = board.piece_on_sq[pm.to_sq() as usize] {
                        ss_val += self.heuristics.get_continuation(
                            pp.piece_type(), Square::new(pm.to_sq()),
                            attacker_pt, Square::new(m.to_sq()));
                    }
                }
                ss_val
            };
            ss[ply].stat_score = stat_score;

            // ------------------------------------------------------------------
            // Step 17. Late Move Reductions (Stockfish)
            // ------------------------------------------------------------------
            let mut score = 0i32;
            let mut needs_full = true;

            if cur_depth >= 2 && legal_moves > 1 {
                // Build r in 1024ths (Stockfish uses /1024 scaled ints)
                let move_idx = legal_moves.min(63);
                let depth_idx = cur_depth.min(63) as usize;
                let mut r = self.reductions[move_idx] + self.reductions[depth_idx];

                // ttPv adjustments
                if ss[ply].tt_pv {
                    r += 946;
                    r -= 2719
                        + pv_node as i32 * 983
                        + (tt_score > alpha) as i32 * 922
                        + (tt_depth >= cur_depth) as i32 * (934 + cut_node as i32 * 1011);
                }

                r += 714; // base offset
                r -= (legal_moves as i32).min(63) * 73;
                r -= correction_value.abs() / 30370;

                if cut_node { r += 3372 + 997 * tt_move.is_none() as i32; }
                let tt_capture = tt_move.map(|t| t.is_capture()).unwrap_or(false);
                if tt_capture { r += 1119; }

                // cutoffCnt boost
                if ply + 1 < MAX_PLY && ss[ply + 1].cutoff_cnt > 1 {
                    let all_node = !pv_node && !cut_node;
                    r += 256 + 1024 * (ss[ply + 1].cutoff_cnt > 2) as i32 + 1024 * all_node as i32;
                }

                // TT move gets reduced less
                if Some(m) == tt_move { r -= 2151; }

                // History bonus/penalty
                r -= stat_score * 850 / 8192;

                // Compute reduced depth d (clamped to [1, new_depth+2])
                let d = (new_depth as i32 - r / 1024).clamp(1, new_depth as i32 + 2) as u8 + pv_node as u8;
                ss[ply].reduction = new_depth as i32 - d as i32;

                score = -self.alpha_beta(board, ss, ply + 1, d, -(alpha + 1), -alpha, true);
                ss[ply].reduction = 0;

                if score > alpha {
                    // Stockfish post-LMR: doDeeperSearch / doShallowerSearch
                    let deeper = (d < new_depth) && score > best_score + 50;
                    let shallower = score < best_score + 9;
                    let adj_depth = (new_depth as i32 + deeper as i32 - shallower as i32).max(0) as u8;

                    if adj_depth > d {
                        // Post-LMR continuation bonus
                        if let Some(pm) = prev_move {
                            if let Some(pp) = board.piece_on_sq[pm.to_sq() as usize] {
                                self.heuristics.update_continuation(
                                    pp.piece_type(), Square::new(pm.to_sq()),
                                    attacker_pt, Square::new(m.to_sq()), cur_depth);
                            }
                        }
                        score = -self.alpha_beta(board, ss, ply + 1, adj_depth, -(alpha + 1), -alpha, !cut_node);
                    }
                    needs_full = score > alpha;
                } else {
                    needs_full = false;
                }
            }

            // ------------------------------------------------------------------
            // Step 18. Full-depth PVS search
            // ------------------------------------------------------------------
            if needs_full {
                if legal_moves == 1 {
                    score = -self.alpha_beta(board, ss, ply + 1, new_depth, -beta, -alpha, false);
                } else {
                    score = -self.alpha_beta(board, ss, ply + 1, new_depth, -(alpha+1), -alpha, !cut_node);
                    if score > alpha && score < beta {
                        score = -self.alpha_beta(board, ss, ply + 1, new_depth, -beta, -alpha, false);
                    }
                }
            }

            if ply == 0 && first_move {
                self.best_move_nodes += self.nodes - start_nodes;
            }
            first_move = false;

            board.unmake_move(m, &undo);
            if board.zobrist_key != board.compute_hash() {
                eprintln!("HASH DRIFT AFTER UNMAKE! ply={}", ply);
                std::process::exit(1);
            }

            if self.stop.load(Ordering::Relaxed) { return 0; }

            if score > best_score { best_score = score; }

            if score >= beta {
                // cutoffCnt
                ss[ply].cutoff_cnt += (extension < 2) as i32 + pv_node as i32;

                // Update heuristics on beta cutoff
                if !is_capture {
                    let see_ok = board.see_ge(m, 0);
                    if see_ok {
                        self.heuristics.update_history(side, attacker_pt, Square::new(m.to_sq()), cur_depth);
                        if attacker_pt == PieceType::Pawn {
                            self.heuristics.update_pawn_history(side, Square::new(m.from_sq()), Square::new(m.to_sq()), cur_depth);
                        } else if attacker_pt == PieceType::Knight || attacker_pt == PieceType::Bishop {
                            self.heuristics.update_minor_piece_history(side, Square::new(m.from_sq()), Square::new(m.to_sq()), cur_depth);
                        }
                        self.heuristics.update_low_ply_history(m, ply, cur_depth);
                    } else {
                        self.heuristics.penalize_history(side, attacker_pt, Square::new(m.to_sq()), cur_depth);
                    }
                    self.heuristics.update_killer(m, ply);

                    if let Some(pm) = prev_move {
                        if let Some(pp) = board.piece_on_sq[pm.to_sq() as usize] {
                            self.heuristics.update_countermove(pp.piece_type(), Square::new(pm.to_sq()), m);
                            if see_ok {
                                self.heuristics.update_continuation(
                                    pp.piece_type(), Square::new(pm.to_sq()),
                                    attacker_pt, Square::new(m.to_sq()), cur_depth);
                            }
                        }
                    }

                    // Penalize all quiets that failed before this cutoff
                    for i in 0..quiet_count.min(64) {
                        let qm = quiets_searched[i];
                        if qm.0 == m.0 { continue; }
                        let qpt = board.piece_on_sq[qm.from_sq() as usize]
                            .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn);
                        let penalty = cur_depth as i32 * cur_depth as i32;
                        let entry = &mut self.heuristics.history[side as usize][qpt as usize][qm.to_sq() as usize];
                        *entry -= penalty - *entry * penalty.abs() / 16384;
                        self.heuristics.penalize_low_ply_history(qm, ply, cur_depth);
                    }
                } else {
                    let victim_pt = board.piece_on_sq[m.to_sq() as usize]
                        .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn);
                    self.heuristics.update_capture_history(attacker_pt, Square::new(m.to_sq()), victim_pt, cur_depth);
                }

                // Fail-high score softening + TT store
                let ret_score = if best_score < MATE_SCORE - MAX_PLY as i32 {
                    (best_score * cur_depth as i32 + beta) / (cur_depth as i32 + 1)
                } else { best_score };

                self.tt.store(board.zobrist_key, cur_depth, ret_score, NodeType::Beta, m, pv_node);
                // ttMoveHistory update (Stockfish)
                self.heuristics.update_tt_move_history(if Some(m) == tt_move { 809 } else { -865 });
                return ret_score;
            }

            if score > alpha {
                alpha = score;
                best_m = m;
                if ply == 0 {
                    self.best_move = m;
                    self.smp.set_best_move(m);
                }
                // Update PV
                self.pv_table[ply][ply] = m;
                if ply + 1 < MAX_PLY {
                    for j in (ply + 1)..self.pv_length[ply + 1] {
                        self.pv_table[ply][j] = self.pv_table[ply + 1][j];
                    }
                    self.pv_length[ply] = self.pv_length[ply + 1];
                }

                // Depth reduction on alpha improvement (Stockfish Step 20)
                if cur_depth > 2 && cur_depth < 14 && best_score.abs() < MATE_SCORE - 100 {
                    cur_depth = cur_depth.saturating_sub(2);
                }
            }

            // Track quiet moves
            if !is_capture && !is_promo && quiet_count < 64 {
                quiets_searched[quiet_count] = m;
                quiet_count += 1;
            }

            // skip_quiets (LMP)
            if !pv_node && !in_check && cur_depth <= 3 && best_score > -MATE_SCORE + 100 {
                let fmc = if cur_depth == 1 { 2 } else if cur_depth == 2 { 4 } else { 8 };
                if quiet_count >= fmc { picker.skip_quiets(); }
            }
        }

        if legal_moves == 0 {
            let side = board.side_to_move;
            let k_bb = board.piece_bb(PieceType::King) & board.color_occupancy(side);
            let in_check = if k_bb.is_not_empty() {
                let king_sq = Square::new(k_bb.lsb());
                board.is_square_attacked(king_sq, side.flip())
            } else {
                false
            };

            if in_check {
                return -MATE_SCORE + ply as i32;
            } else {
                return 0;
            }
        }

        let node_type = if best_score >= beta {
            NodeType::Beta
        } else if best_score > orig_alpha {
            NodeType::Exact
        } else {
            NodeType::Alpha
        };
        self.tt
            .store(board.zobrist_key, depth, best_score, node_type, best_m, pv_node);

        best_score
    }

    // ------------------------------------------------------------------
    //   ITERATIVE DEEPENING WITH ASPIRATION WINDOWS & TIME MANAGEMENT
    // ------------------------------------------------------------------
    pub fn iterate(&mut self, board: &mut Board, max_depth: u8) -> Move {
        self.stop.store(false, Ordering::Relaxed);
        self.nodes = 0;
        self.start_time = Instant::now();
        self.best_move = Move::new(0, 0, 0);
        self.best_move_nodes = 0;
        self.nmp_min_ply = 0;
        let mut average_score = 0;

        let start_depth = if self.thread_id > 0 {
            1 + (self.thread_id as u8 % 3)
        } else {
            1
        };

        // Root Probing for Syzygy DTZ
        if let Some(tb) = &self.smp.tb {
            let pieces_count = board.occupancies().count() as u32;
            if pieces_count <= tb.max_pieces() && board.halfmove_clock == 0 {
                let w_bb = board.color_occupancy(crate::types::Color::White).0;
                let b_bb = board.color_occupancy(crate::types::Color::Black).0;
                let k_bb = board.piece_bb(PieceType::King).0;
                let q_bb = board.piece_bb(PieceType::Queen).0;
                let r_bb = board.piece_bb(PieceType::Rook).0;
                let b_bb_pc = board.piece_bb(PieceType::Bishop).0;
                let n_bb = board.piece_bb(PieceType::Knight).0;
                let p_bb = board.piece_bb(PieceType::Pawn).0;
                let ep = board.en_passant.map_or(0, |sq| sq.0 as u32);
                let turn = board.side_to_move == crate::types::Color::White;

                if let Ok(dtz_res) = tb.probe_root(w_bb, b_bb, k_bb, q_bb, r_bb, b_bb_pc, n_bb, p_bb, board.halfmove_clock as u32, ep, turn) {
                    if let pyrrhic_rs::DtzProbeValue::DtzResult(res) = dtz_res.root {
                        if dtz_res.num_moves > 0 {
                            // Find the best DTZ move from the moves array
                            // A perfect implementation would filter `root_moves`, but for simplicity
                            // if it's a decisive win or draw, we'll try to find the move that matches `res.to_square`
                            let mut best_dtz_move = Move::none();
                            for mv_res in dtz_res.moves.iter().take(dtz_res.num_moves) {
                                if let pyrrhic_rs::DtzProbeValue::DtzResult(rm) = mv_res {
                                    if rm.wdl == res.wdl && rm.dtz == res.dtz {
                                        // Reconstruct the internal Move format (basic)
                                        let from = rm.from_square as usize;
                                        let to = rm.to_square as usize;
                                        // This doesn't perfectly match MikuEngine's move flags (castling, etc)
                                        // so we search legal moves that match from/to.
                                        let mut moves = crate::movegen::MoveList::new();
                                        crate::movegen::generate_pseudo_legal_moves(board, &mut moves);
                                        for i in 0..moves.count {
                                            let m = moves.moves[i];
                                            if board.is_pseudo_legal(m) && m.from_sq() == from as u8 && m.to_sq() == to as u8 {
                                                if rm.promotion == pyrrhic_rs::Piece::Pawn || m.is_promotion() {
                                                     best_dtz_move = m;
                                                     break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if best_dtz_move != Move::none() {
                                let score = match res.wdl {
                                    pyrrhic_rs::WdlProbeResult::Win => TB_SCORE,
                                    pyrrhic_rs::WdlProbeResult::Loss => -TB_SCORE,
                                    pyrrhic_rs::WdlProbeResult::Draw | pyrrhic_rs::WdlProbeResult::BlessedLoss | pyrrhic_rs::WdlProbeResult::CursedWin => 0,
                                };
                                println!("info depth 1 score cp {} time 1 pv {:?}", score, best_dtz_move);
                                return best_dtz_move;
                            }
                        }
                    }
                }
            }
        }

        // Initialize SearchStack: ss[0..MAX_PLY+6]
        let mut ss = vec![SearchStack::new(); MAX_PLY + 10];

        for d in start_depth..=max_depth {
            let nodes_before_iter = self.nodes;
            let score;

            // Reset ss for each iteration
            for s in ss.iter_mut() { *s = SearchStack::new(); }

            // Aspiration Windows from depth 5+
            if d >= 5 {
                // Stockfish: delta starts at 5 + abs(avg)*avg/9000, expands by delta/3
                let avg = average_score as i32;
                let mut delta: i32 = 5 + avg.abs() * avg.abs() / 9000;
                delta = delta.max(5);
                let mut a = (average_score - delta).max(-INF);
                let mut b = (average_score + delta).min(INF);

                loop {
                    let s = self.alpha_beta(board, &mut ss, 0, d, a, b, false);

                    if self.stop.load(Ordering::Relaxed) {
                        return self.best_move;
                    }

                    if s <= a {
                        // Fail low
                        if self.thread_id == 0 {
                            self.timer.aspiration_fail(true);
                        }
                        b = (a + b) / 2;
                        a = (average_score - delta).max(-INF);
                        delta += delta / 3;
                    } else if s >= b {
                        // Fail high
                        if self.thread_id == 0 {
                            self.timer.aspiration_fail(false);
                        }
                        b = (average_score + delta).min(INF);
                        delta += delta / 3;
                    } else {
                        score = s;
                        break;
                    }

                    if delta > 2000 {
                        score = self.alpha_beta(board, &mut ss, 0, d, -INF, INF, false);
                        break;
                    }
                }
            } else {
                score = self.alpha_beta(board, &mut ss, 0, d, -INF, INF, false);
                if self.stop.load(Ordering::Relaxed) {
                    break;
                }
            }

            if d == 1 {
                average_score = score;
            } else {
                average_score = average_score + (score - average_score) / (d as i32);
            }

            self.prev_best_score = score;

            let elapsed = self.timer.elapsed().max(1);
            let nps = if elapsed > 0 {
                self.nodes as u128 * 1000 / elapsed
            } else {
                0
            };

            // Mate score display
            let score_str = if score.abs() > MATE_SCORE - 100 {
                let mate_in = (MATE_SCORE - score.abs() + 1) / 2;
                if score > 0 {
                    format!("mate {}", mate_in)
                } else {
                    format!("mate -{}", mate_in)
                }
            } else {
                format!("cp {}", score)
            };

            let hashfull = self.tt.hashfull();

            let mut pv_str = String::new();
            for i in 0..self.pv_length[0] {
                pv_str.push_str(&format!("{:?} ", self.pv_table[0][i]));
            }

            let wdl_str = if self.smp.show_wdl && score.abs() <= MATE_SCORE - 100 {
                let wdl_w = crate::eval::win_rate_model(score, board);
                let wdl_l = crate::eval::win_rate_model(-score, board);
                let wdl_d = 1000 - wdl_w - wdl_l;
                format!(" wdl {} {} {}", wdl_w, wdl_d, wdl_l)
            } else {
                String::new()
            };

            if self.thread_id == 0 {
                // Stockfish info string format:
                println!(
                    "info depth {} seldepth {} multipv 1 score {}{} nodes {} nps {} hashfull {} tbhits 0 time {} pv {}",
                    d,
                    d, // MikuEngine doesn't track seldepth yet, so we just mirror depth
                    score_str,
                    wdl_str,
                    self.nodes,
                    nps,
                    hashfull,
                    elapsed,
                    pv_str.trim()
                );

                // Update TimeManager
                self.timer.update_pv(self.best_move.0);
                self.timer.update_score(score);

                let nodes_this_iter = self.nodes - nodes_before_iter;
                
                // Move importance scaling (unclear best move)
                if d > 5 {
                    let effort = self.best_move_nodes as f64 / (self.nodes.max(1) as f64);
                    if effort < 0.3 {
                        self.timer.move_importance_high();
                    }
                }

                // Check if we can safely predict finishing the next iteration
                if d < max_depth && !self.timer.can_start_next_iteration(nodes_this_iter, nps as u64) {
                    self.stop.store(true, Ordering::Relaxed);
                }
            }

            if self.stop.load(Ordering::Relaxed) {
                break;
            }
        }

        if self.thread_id == 0 {
            self.stop.store(true, Ordering::Relaxed);
        }

        self.best_move
    }
}
