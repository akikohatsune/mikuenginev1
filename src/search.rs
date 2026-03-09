use crate::board::Board;
use crate::history::{Heuristics, MAX_PLY};
use crate::movepick::MovePicker;
use crate::transposition::{NodeType, TranspositionTable};
use crate::types::{Move, PieceType, Square};
use std::time::{Duration, Instant};

const INF: i32 = 50000;
const MATE_SCORE: i32 = 48000;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Search {
    pub tt: Arc<TranspositionTable>,
    pub heuristics: Heuristics,
    pub nodes: u64,
    pub start_time: Instant,
    pub soft_limit: Duration,
    pub hard_limit: Duration,
    pub stop: Arc<AtomicBool>,
    pub thread_id: usize,
    pub best_move: Move,
    pub best_move_nodes: u64,
    pub prev_best_score: i32,
    pub pv_table: [[Move; MAX_PLY]; MAX_PLY],
    pub pv_length: [usize; MAX_PLY],
    pub lmr_table: [[u8; 64]; 64],
}

impl Search {
    pub fn new(tt: Arc<TranspositionTable>, stop: Arc<AtomicBool>, thread_id: usize) -> Self {
        let mut lmr_table = [[0; 64]; 64];
        for d in 1..64 {
            for m in 1..64 {
                let reduction = 0.75 + (d as f64).ln() * (m as f64).ln() / 2.25;
                lmr_table[d][m] = reduction as u8;
            }
        }

        Search {
            tt,
            heuristics: Heuristics::new(),
            nodes: 0,
            start_time: Instant::now(),
            soft_limit: Duration::from_secs(u64::MAX),
            hard_limit: Duration::from_secs(u64::MAX),
            stop,
            thread_id,
            best_move: Move::new(0, 0, 0),
            best_move_nodes: 0,
            prev_best_score: 0,
            pv_table: [[Move::new(0, 0, 0); MAX_PLY]; MAX_PLY],
            pv_length: [0; MAX_PLY],
            lmr_table,
        }
    }

    pub fn set_time(&mut self, time_ms: u64) {
        // Soft limit = nominal time, Hard limit = 3x
        self.soft_limit = Duration::from_millis(time_ms);
        self.hard_limit = Duration::from_millis(time_ms.saturating_mul(3).min(u64::MAX / 2));
    }

    pub fn check_time(&mut self) {
        if self.nodes.is_multiple_of(2048)
            && self.thread_id == 0 && self.start_time.elapsed() >= self.hard_limit {
                self.stop.store(true, Ordering::Relaxed);
            }
    }

    // ------------------------------------------------------------------
    //   QUIESCENCE SEARCH
    // ------------------------------------------------------------------
    pub fn quiescence(
        &mut self,
        board: &mut Board,
        mut alpha: i32,
        beta: i32,
        q_ply: usize,
    ) -> i32 {
        if q_ply > 200 {
            println!(
                "q_ply max reached! stand_pat={}",
                board.nnue.evaluate(board.side_to_move, &board.accumulator)
            );
            std::process::exit(1);
        }
        self.nodes += 1;
        self.check_time();
        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        let stand_pat = board.nnue.evaluate(board.side_to_move, &board.accumulator);
        if stand_pat >= beta {
            return beta;
        }
        if stand_pat > alpha {
            alpha = stand_pat;
        }

        let mut picker = MovePicker::new(None, 0, None, true, false);

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
                let score = -self.quiescence(board, -beta, -alpha, q_ply + 1);
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
    //   NEGAMAX WITH ALL MODERN HEURISTICS
    // ------------------------------------------------------------------
    pub fn negamax(
        &mut self,
        board: &mut Board,
        mut depth: u8,
        mut alpha: i32,
        mut beta: i32,
        ply: usize,
        prev_move: Option<Move>,
        cut_node: bool,
    ) -> i32 {
        if ply > 200 {
            println!(
                "max ply reached! depth={} in_check={} prev_move={:?} tt_hash={:x}",
                depth,
                board.is_square_attacked(
                    Square::new(
                        (board.piece_bb(PieceType::King)
                            & board.color_occupancy(board.side_to_move))
                        .lsb()
                    ),
                    board.side_to_move.flip()
                ),
                prev_move,
                board.zobrist_key
            );
            std::process::exit(1);
        }

        let true_hash = board.compute_hash();
        if board.zobrist_key != true_hash {
            eprintln!(
                "HASH DRIFT DETECTED IN NEGAMAX! incrementally={:x}, true={:x}, ply={}",
                board.zobrist_key, true_hash, ply
            );
            std::process::exit(1);
        }

        self.nodes += 1;
        self.check_time();
        if self.stop.load(Ordering::Relaxed) {
            return 0;
        }

        self.pv_length[ply] = ply;

        // Draw detection
        let draw_score = -1 + (self.nodes & 2) as i32;
        if board.halfmove_clock >= 100 {
            return draw_score;
        }
        if ply > 0 && board.is_repetition() {
            return draw_score;
        }

        // Mate distance pruning
        alpha = alpha.max(-MATE_SCORE + ply as i32);
        beta = beta.min(MATE_SCORE - (ply as i32 + 1));
        if alpha >= beta {
            return alpha;
        }

        let orig_alpha = alpha;
        let pv_node = beta - alpha > 1;
        let mut tt_move = None;
        let mut tt_score = 0;
        let mut tt_depth = 0;
        let mut tt_node_type = NodeType::None;
        let mut tt_is_pv = false;

        // TT Probe
        if let Some(entry) = self.tt.probe(board.zobrist_key) {
            tt_move = Some(entry.best_move);
            tt_score = entry.score;
            tt_depth = entry.depth;
            tt_node_type = entry.node_type;
            tt_is_pv = entry.is_pv;

            if !pv_node && entry.depth >= depth {
                if entry.node_type == NodeType::Exact {
                    return entry.score;
                }
                if entry.node_type == NodeType::Alpha && entry.score <= alpha {
                    return alpha;
                }
                if entry.node_type == NodeType::Beta && entry.score >= beta {
                    return beta;
                }
            }

            // Small ProbCut (TT-based)
            if !pv_node && entry.depth < depth && entry.depth >= depth.saturating_sub(4) {
                if (entry.node_type == NodeType::Beta || entry.node_type == NodeType::Exact)
                    && entry.score >= beta + 418 {
                        return beta;
                    }
            }
        }

        if depth == 0 {
            return self.quiescence(board, alpha, beta, 0);
        }

        let side = board.side_to_move;
        let king_sq_opt = board.piece_bb(PieceType::King) & board.color_occupancy(side);
        let in_check = if king_sq_opt.is_not_empty() {
            let king_sq = Square::new(king_sq_opt.lsb());
            board.is_square_attacked(king_sq, side.flip())
        } else {
            false
        };

        // Check Extension
        if in_check {
            depth += 1;
        }

        // Static evaluation
        let mut eval = if in_check {
            -INF // Don't trust static eval when in check
        } else {
            board.nnue.evaluate(side, &board.accumulator)
        };

        // --- Calculate Correction Value ---
        let mut correction_value = 0;
        let mat_hash = board.non_pawn_material(side) as usize;
        correction_value += self.heuristics.get_non_pawn_correction(side, mat_hash);

        if let Some(pm) = prev_move {
            if let Some(_prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                // If there's a previous move, add its continuation correction (using a dummy attacker_pt for now, typically it's context-dependent, but we use King as a placeholder for the generic state if not specifically attached to a piece yet. In true Stockfish this is per-piece, but we adjust the *board sum* here).
                // Wait, continuation history is mostly used for move ordering score!
                // For static eval correction, Stockfish uses exactly the pieces from the last 2 plies.
                // We will just use the non-pawn correction for the static eval baseline here.
            }
        }

        if !in_check && eval.abs() < 4000 {
            // Don't adjust mates/winning evals too wildly
            eval += correction_value;
        }

        // Store static eval for improving detection
        if ply < MAX_PLY {
            self.heuristics.static_evals[ply] = eval;
        }
        let mut improving = !in_check && self.heuristics.is_improving(ply, eval);
        improving |= eval >= beta; // #53: improving |= staticEval >= beta

        let opponent_worsening = if ply >= 1 && !in_check {
            eval > -self.heuristics.static_evals[ply - 1]
        } else {
            false
        };

        let has_non_pawn_material = (board.colors[side as usize]
            & !(board.pieces[PieceType::Pawn as usize] | board.pieces[PieceType::King as usize]))
            .is_not_empty();

        // ------------------------------------------------------------------
        //   PRE-MOVES PRUNING
        // ------------------------------------------------------------------

        // Reverse Futility Pruning (Static Null Move Pruning)
        // Use improving to adjust margin
        if !pv_node && !in_check && depth <= 6 {
            let mut rfp_margin = depth as i32 * if improving { 75 } else { 120 };
            if opponent_worsening {
                rfp_margin -= depth as i32 * 25;
            }
            if eval - rfp_margin >= beta {
                return eval;
            }
        }

        // Razoring (Stockfish formula)
        if !pv_node && !in_check && depth <= 4 {
            let razor_margin = alpha - 485 - 281 * (depth as i32 * depth as i32);
            if eval <= razor_margin {
                let q_score = self.quiescence(board, alpha, beta, 0);
                if q_score <= alpha {
                    return q_score;
                }
            }
        }

        // Null Move Pruning
        if cut_node && !in_check && depth >= 3 && has_non_pawn_material && eval >= beta {
            let undo = board.make_null_move();
            let mut r = 4 + depth / 3;
            if eval - beta > 0 {
                r += ((eval - beta) as u8 / 200).min(3);
            }
            if opponent_worsening {
                r = r.saturating_sub(1);
            }
            let null_depth = depth.saturating_sub(r);
            let null_score =
                -self.negamax(board, null_depth, -beta, -beta + 1, ply + 1, None, !pv_node);
            board.unmake_null_move(&undo);

            if self.stop.load(Ordering::Relaxed) {
                return 0;
            }

            if null_score >= beta {
                if depth >= 8 && ply < depth as usize {
                    let verify_score =
                        self.negamax(board, null_depth, alpha, beta, ply + 1, None, false);
                    if verify_score >= beta {
                        return beta;
                    }
                } else {
                    return beta;
                }
            }
        }

        // Razoring
        if !pv_node && !in_check && depth <= 2 {
            let razor_margin = 300 + depth as i32 * 60;
            if eval + razor_margin <= alpha {
                let q = self.quiescence(board, alpha, beta, 0);
                if q <= alpha {
                    return q;
                }
            }
        }

        // Internal Iterative Deepening (IID)
        // If no TT move at high depth, do a reduced search to find one
        if tt_move.is_none() && depth >= 4 && pv_node {
            self.negamax(board, depth - 2, alpha, beta, ply, prev_move, cut_node);
            if let Some(entry) = self.tt.probe(board.zobrist_key) {
                tt_move = Some(entry.best_move);
            }
        }

        let is_shuffling = board.halfmove_clock >= 20;

        // ------------------------------------------------------------------
        //   MOVE GENERATION & ORDERING
        // ------------------------------------------------------------------
        let futility_pruning = !pv_node && !in_check && depth <= 6;
        let mut futility_margin = 75 + depth as i32 * 150;

        // --- Stockfish Technique: Futility w/ correction history ---
        futility_margin += correction_value.abs() / 150;

        let countermove = if let Some(pm) = prev_move {
            if let Some(prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                let cm = self
                    .heuristics
                    .get_countermove(prev_piece.piece_type(), Square::new(pm.to_sq()));
                if cm.0 != 0 {
                    Some(cm)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut picker = MovePicker::new(tt_move, ply, countermove, false, in_check);

        let mut legal_moves = 0;
        let mut best_m = Move::new(0, 0, 0);
        let mut best_score = -INF;

        // Track quiet moves searched for history penalty and LMR boost
        let mut quiets_searched: [Move; 64] = [Move::new(0, 0, 0); 64];
        let mut quiet_count = 0;
        let mut first_move = true;

        while let Some(m) = picker.next(&self.heuristics, board) {
            let start_nodes = self.nodes;
            let is_capture = m.is_capture();
            let is_promo = m.is_promotion();

            if !is_capture && !is_promo && quiet_count < 64 {
                quiets_searched[quiet_count] = m;
                quiet_count += 1;
            }

            // Store attacker piece type BEFORE make_move (from_sq will be empty after)
            let attacker_pt = board.piece_on_sq[m.from_sq() as usize]
                .map(|p| p.piece_type())
                .unwrap_or(PieceType::Pawn);

            // SEE pruning for quiet moves BEFORE make_move (Stockfish style)
            if depth <= 4 && !is_capture && !is_promo && legal_moves > 0
                && !board.see_ge(m, -(depth as i32 * 80)) {
                    continue;
                }

            // SEE pruning for captures w/ captHist
            if depth <= 6 && is_capture && !is_promo && legal_moves > 0 {
                let victim_pt = if m.flag() == Move::FLAG_EP {
                    PieceType::Pawn
                } else {
                    board.piece_on_sq[m.to_sq() as usize]
                        .map(|p| p.piece_type())
                        .unwrap_or(PieceType::Pawn)
                };
                let capt_hist = self.heuristics.get_capture_history(
                    attacker_pt,
                    Square::new(m.to_sq()),
                    victim_pt,
                );
                let margin = (166 * depth as i32 + capt_hist / 29).max(0);
                if !board.see_ge(m, -margin) {
                    continue;
                }
            }

            if !board.is_castling_legal(m) {
                continue;
            }

            let undo = board.make_move(m);
            if board.zobrist_key != board.compute_hash() {
                eprintln!(
                    "HASH DRIFT AFTER MAKE_MOVE {:?}! incrementally={:x}, true={:x}, ply={}",
                    m,
                    board.zobrist_key,
                    board.compute_hash(),
                    ply
                );
                std::process::exit(1);
            }

            let move_side = board.side_to_move.flip();
            let move_king_bb = board.piece_bb(PieceType::King) & board.color_occupancy(move_side);
            let move_is_legal = if move_king_bb.is_not_empty() {
                let king_sq = Square::new(move_king_bb.lsb());
                !board.is_square_attacked(king_sq, board.side_to_move)
            } else {
                true
            };

            if move_is_legal {
                legal_moves += 1;
                let mut score = 0;

                // --- Singular Extension ---
                let mut extension: i32 = 0;
                if !is_shuffling {
                    if let Some(ttm) = tt_move {
                        if m.0 == ttm.0
                            && depth >= 7
                            && !is_promo
                            && tt_depth >= depth - 3
                            && (tt_node_type == NodeType::Beta || tt_node_type == NodeType::Exact)
                        {
                            let singular_margin = depth as i32 * 2;
                            let se_beta = (tt_score - singular_margin).max(-MATE_SCORE);
                            let se_depth = (depth - 1) / 2;

                            board.unmake_move(m, &undo);
                            let se_score = self.negamax(
                                board,
                                se_depth,
                                se_beta - 1,
                                se_beta,
                                ply,
                                prev_move,
                                cut_node,
                            );
                            let _undo_re = board.make_move(m);

                            if se_score < se_beta {
                                extension = 1;
                            } else if se_beta >= beta {
                                // Multi-cut pruning
                                board.unmake_move(m, &_undo_re);
                                return beta;
                            } else if !cut_node {
                                extension = -1; // Negative singular extension
                            }
                        }
                    }
                }

                // Check if THIS move gives check
                let gives_check = {
                    let opp = board.side_to_move;
                    let opp_king_bb = board.piece_bb(PieceType::King) & board.color_occupancy(opp);
                    if opp_king_bb.is_not_empty() {
                        let opp_king_sq = Square::new(opp_king_bb.lsb());
                        board.is_square_attacked(opp_king_sq, opp.flip())
                    } else {
                        false
                    }
                };

                // Passed pawn extension (attacker_pt was stored before make_move)
                if attacker_pt == PieceType::Pawn && !is_capture {
                    let rank = m.to_sq() / 8;
                    if (side == crate::types::Color::White && rank >= 6)
                        || (side == crate::types::Color::Black && rank <= 1)
                    {
                        extension = extension.max(1);
                    }
                }

                let mut stat_score = 0;
                if !is_capture && !is_promo {
                    stat_score =
                        self.heuristics
                            .get_history(side, attacker_pt, Square::new(m.to_sq()));
                    if attacker_pt == PieceType::Pawn {
                        stat_score += self.heuristics.get_pawn_history(
                            side,
                            Square::new(m.from_sq()),
                            Square::new(m.to_sq()),
                        );
                    } else if attacker_pt == PieceType::Knight || attacker_pt == PieceType::Bishop {
                        stat_score += self.heuristics.get_minor_piece_history(
                            side,
                            Square::new(m.from_sq()),
                            Square::new(m.to_sq()),
                        );
                    }
                    stat_score += self.heuristics.get_low_ply_history(m, ply);

                    if let Some(pm) = prev_move {
                        if let Some(prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                            stat_score += self.heuristics.get_continuation(
                                prev_piece.piece_type(),
                                Square::new(pm.to_sq()),
                                attacker_pt,
                                Square::new(m.to_sq()),
                            );
                        }
                    }
                }

                // --- Futility Pruning ---
                if futility_pruning && legal_moves > 1 && !is_capture && !is_promo && !gives_check {
                    let adjusted_margin = futility_margin + stat_score / 150;
                    if eval + adjusted_margin <= alpha {
                        board.unmake_move(m, &undo);
                        continue;
                    }
                }

                // --- Capture Futility Pruning ---
                if !pv_node && is_capture && !is_promo && !gives_check && depth <= 5 {
                    let victim_pt = if m.flag() == Move::FLAG_EP {
                        PieceType::Pawn
                    } else {
                        board.piece_on_sq[m.to_sq() as usize]
                            .map(|p| p.piece_type())
                            .unwrap_or(PieceType::Pawn)
                    };
                    let capt_hist = self.heuristics.get_capture_history(
                        attacker_pt,
                        Square::new(m.to_sq()),
                        victim_pt,
                    );
                    let futility_value = eval
                        + 232
                        + 217 * (depth as i32)
                        + crate::eval::PIECE_VALUES[victim_pt as usize] * 10
                        + capt_hist / 100;
                    if futility_value <= alpha {
                        board.unmake_move(m, &undo);
                        continue;
                    }
                }

                // --- Late Move Pruning (LMP) ---
                if !pv_node && depth <= 4 && !in_check && !is_capture && !is_promo && !gives_check {
                    let lmp_threshold = if improving {
                        3 + (depth as usize) * (depth as usize)
                    } else {
                        (3 + (depth as usize) * (depth as usize)) / 2
                    };
                    if legal_moves > lmp_threshold {
                        board.unmake_move(m, &undo);
                        continue;
                    }
                }

                // Compute search depth with extension
                let mut new_depth = (depth as i32 - 1 + extension).max(0) as u8;

                // --- Late Move Reductions (LMR) ---
                let mut needs_full_search = true;

                if legal_moves >= 3
                    && depth >= 3
                    && !is_capture
                    && !is_promo
                    && !in_check
                    && !gives_check
                {
                    let d_idx = (depth as usize).min(63);
                    let m_idx = legal_moves.min(63);
                    // Fractional reduction (base 1024 = 1 depth)
                    let mut r_frac = (self.lmr_table[d_idx][m_idx] as i32) * 1024;

                    // Reduce more when not improving
                    if !improving {
                        r_frac += 1024;
                    }

                    // #19 cutNode LMR boost
                    if cut_node {
                        r_frac += 1024;
                    }

                    // #20 ttCapture LMR boost
                    if let Some(ttm) = tt_move {
                        if ttm.is_capture() {
                            r_frac += 1024;
                        }
                    }

                    // --- Stockfish Technique: cutoffCnt LMR boost ---
                    // If we have searched many quiet moves without a cutoff, increase LMR
                    if quiet_count > 2 {
                        r_frac += 256 + 1024;
                        // Add an extra boost if allNode (pv_node == false and no cutoffs yet)
                        if !pv_node {
                            r_frac += 1024;
                        }
                    }

                    // ttPv LMR reduction reduction
                    if tt_is_pv {
                        r_frac -= 1024;
                    }

                    // Reduce less for killer/countermove
                    let k = self.heuristics.killer_slot(m, ply);
                    if k > 0 {
                        r_frac -= 1024;
                    }

                    // Adjust by history score
                    let h = self
                        .heuristics
                        .get_history(side, attacker_pt, Square::new(m.to_sq()));
                    if h > 4000 {
                        r_frac -= 1024;
                    }
                    if h < -2000 {
                        r_frac += 1024;
                    }

                    // statScore LMR (smooth fractional offset)
                    r_frac -= stat_score / 4;

                    let r = (r_frac / 1024).max(0);

                    let reduced_depth = (new_depth as i32 - r).max(1) as u8;

                    score = -self.negamax(
                        board,
                        reduced_depth,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        Some(m),
                        true,
                    );

                    needs_full_search = score > alpha;

                    // Hindsight depth adjustment
                    if needs_full_search && r >= 3 && !opponent_worsening && new_depth < 60 {
                        new_depth += 1;
                    }
                }

                // --- Principal Variation Search (PVS) ---
                if needs_full_search {
                    if legal_moves == 1 {
                        score =
                            -self.negamax(board, new_depth, -beta, -alpha, ply + 1, Some(m), false);
                    } else {
                        score = -self.negamax(
                            board,
                            new_depth,
                            -alpha - 1,
                            -alpha,
                            ply + 1,
                            Some(m),
                            !pv_node,
                        );
                        if score > alpha && score < beta {
                            score = -self.negamax(
                                board,
                                new_depth,
                                -beta,
                                -alpha,
                                ply + 1,
                                Some(m),
                                false,
                            );
                        }
                    }

                    // #24 Post-LMR contHist update
                    if legal_moves >= 3 && score > alpha && !is_capture && !is_promo {
                        if let Some(pm) = prev_move {
                            if let Some(prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                                // Small bonus for move that caused re-search to fail high
                                self.heuristics.update_continuation(
                                    prev_piece.piece_type(),
                                    Square::new(pm.to_sq()),
                                    attacker_pt,
                                    Square::new(m.to_sq()),
                                    depth,
                                );
                            }
                        }
                    }
                }

                if ply == 0 && first_move {
                    self.best_move_nodes += self.nodes - start_nodes;
                }
                first_move = false;

                board.unmake_move(m, &undo);
                if board.zobrist_key != board.compute_hash() {
                    eprintln!("HASH DRIFT AFTER UNMAKE_MOVE {:?} (PVS)! incrementally={:x}, true={:x}, ply={}", m, board.zobrist_key, board.compute_hash(), ply);
                    std::process::exit(1);
                }

                if self.stop.load(Ordering::Relaxed) {
                    return 0;
                }

                if score > best_score {
                    best_score = score;
                }

                if score >= beta {
                    // Beta cutoff — update heuristics
                    if !is_capture {
                        // Negative SEE penalty on quiet history
                        let see_ok = board.see_ge(m, 0);
                        if !see_ok {
                            self.heuristics.penalize_history(
                                side,
                                attacker_pt,
                                Square::new(m.to_sq()),
                                depth,
                            );
                            if attacker_pt == PieceType::Pawn {
                                self.heuristics.penalize_pawn_history(
                                    side,
                                    Square::new(m.from_sq()),
                                    Square::new(m.to_sq()),
                                    depth,
                                );
                            } else if attacker_pt == PieceType::Knight
                                || attacker_pt == PieceType::Bishop
                            {
                                self.heuristics.penalize_minor_piece_history(
                                    side,
                                    Square::new(m.from_sq()),
                                    Square::new(m.to_sq()),
                                    depth,
                                );
                            }
                            self.heuristics.penalize_low_ply_history(m, ply, depth);
                        } else {
                            // Update quiet histories
                            self.heuristics.update_history(
                                side,
                                attacker_pt,
                                Square::new(m.to_sq()),
                                depth,
                            );
                            if attacker_pt == PieceType::Pawn {
                                self.heuristics.update_pawn_history(
                                    side,
                                    Square::new(m.from_sq()),
                                    Square::new(m.to_sq()),
                                    depth,
                                );
                            } else if attacker_pt == PieceType::Knight
                                || attacker_pt == PieceType::Bishop
                            {
                                self.heuristics.update_minor_piece_history(
                                    side,
                                    Square::new(m.from_sq()),
                                    Square::new(m.to_sq()),
                                    depth,
                                );
                            }
                            self.heuristics.update_low_ply_history(m, ply, depth);
                        }

                        self.heuristics.update_killer(m, ply);

                        // Continuation history
                        if let Some(pm) = prev_move {
                            if let Some(prev_piece) = board.piece_on_sq[pm.to_sq() as usize] {
                                self.heuristics.update_countermove(
                                    prev_piece.piece_type(),
                                    Square::new(pm.to_sq()),
                                    m,
                                );
                                if see_ok {
                                    self.heuristics.update_continuation(
                                        prev_piece.piece_type(),
                                        Square::new(pm.to_sq()),
                                        attacker_pt,
                                        Square::new(m.to_sq()),
                                        depth,
                                    );
                                }
                            }
                        }

                        // Penalize all quiet moves that failed before cutoff
                        for qm in quiets_searched.iter().take(quiet_count) {
                            if qm.0 != m.0 {
                                if let Some(qp) = board.piece_on_sq[qm.from_sq() as usize] {
                                    let qpt = qp.piece_type();

                                    // --- Stockfish Technique: Fail-low counter bonus ---
                                    // Instead of just penalizing based on depth, Stockfish penalizes failed quiets
                                    // more if their statScore is already high, to aggressively drop them.
                                    // We will calculate a stat_score penalty.
                                    let stat_score = self.heuristics.get_history(
                                        side,
                                        qpt,
                                        Square::new(qm.to_sq()),
                                    );
                                    let mut penalty = depth as i32 * depth as i32;
                                    penalty += stat_score / 10; // Extra penalty for high initial score

                                    // Normally penalize_history just uses depth, but here we manually subtract
                                    let entry = &mut self.heuristics.history[side as usize]
                                        [qpt as usize]
                                        [qm.to_sq() as usize];
                                    *entry -= penalty - *entry * penalty.abs() / 16384;
                                    // We skip adding this logic manually into pawn_history and minor_piece_history to keep it simple,
                                    // but we run their standard penalize methods.

                                    if qpt == PieceType::Pawn {
                                        self.heuristics.penalize_pawn_history(
                                            side,
                                            Square::new(qm.from_sq()),
                                            Square::new(qm.to_sq()),
                                            depth,
                                        );
                                    } else if qpt == PieceType::Knight || qpt == PieceType::Bishop {
                                        self.heuristics.penalize_minor_piece_history(
                                            side,
                                            Square::new(qm.from_sq()),
                                            Square::new(qm.to_sq()),
                                            depth,
                                        );
                                    }
                                    self.heuristics.penalize_low_ply_history(*qm, ply, depth);
                                }
                            }
                        }
                    } else {
                        // Capture history update
                        let to_sq = m.to_sq();
                        let victim_pt = board.piece_on_sq[to_sq as usize]
                            .map(|p| p.piece_type())
                            .unwrap_or(PieceType::Pawn);
                        self.heuristics.update_capture_history(
                            attacker_pt,
                            Square::new(to_sq),
                            victim_pt,
                            depth,
                        );
                    }

                    // Fail-high softening
                    let mut ret_score = score;
                    if ret_score < MATE_SCORE - MAX_PLY as i32 {
                        ret_score = (ret_score * (depth as i32) + beta) / (depth as i32 + 1);
                    }

                    self.tt.store(
                        board.zobrist_key,
                        depth,
                        ret_score,
                        NodeType::Beta,
                        m,
                        pv_node,
                    );
                    return ret_score;
                }

                if score > alpha {
                    alpha = score;
                    best_m = m;
                    if ply == 0 {
                        self.best_move = m;
                    }

                    // Update PV
                    self.pv_table[ply][ply] = m;
                    if ply + 1 < MAX_PLY {
                        for j in (ply + 1)..self.pv_length[ply + 1] {
                            self.pv_table[ply][j] = self.pv_table[ply + 1][j];
                        }
                        self.pv_length[ply] = self.pv_length[ply + 1];
                    }
                }

                // Track quiet moves searched
                if !is_capture && !is_promo {
                    if quiet_count < 64 {
                        quiets_searched[quiet_count] = m;
                    }
                    quiet_count += 1;

                    // skip_quiet_moves (First-Move-Count Pruning)
                    if !pv_node && !in_check && depth <= 3 && best_score > -MATE_SCORE {
                        let fmc = if depth == 1 {
                            2
                        } else if depth == 2 {
                            4
                        } else {
                            8
                        };
                        if quiet_count >= fmc {
                            picker.skip_quiets();
                        }
                    }
                }
            } else {
                board.unmake_move(m, &undo);
                if board.zobrist_key != board.compute_hash() {
                    eprintln!("HASH DRIFT AFTER UNMAKE_MOVE {:?} (ILLEGAL)! incrementally={:x}, true={:x}, ply={}", m, board.zobrist_key, board.compute_hash(), ply);
                    std::process::exit(1);
                }
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

        let node_type = if alpha > orig_alpha {
            NodeType::Exact
        } else {
            NodeType::Alpha
        };
        self.tt
            .store(board.zobrist_key, depth, alpha, node_type, best_m, pv_node);

        alpha
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
        let mut average_score = 0;
        let mut best_move_changes = 0;

        for d in 1..=max_depth {
            let prev_best_move = self.best_move;
            let score;

            // Aspiration Windows from depth 5+
            if d >= 5 {
                let mut delta: i32 = 16;
                let mut a = (average_score - delta).max(-INF);
                let mut b = (average_score + delta).min(INF);
                let mut failed_high_count = 0;
                let mut search_again_counter = 0;

                loop {
                    // Adjust depth slightly if we keep failing high (Stockfish style)
                    let search_depth = if d > 1 {
                        (d as i32 - failed_high_count - search_again_counter / 2).max(1) as u8
                    } else {
                        1
                    };
                    let s = self.negamax(board, search_depth, a, b, 0, None, false);

                    if self.stop.load(Ordering::Relaxed) {
                        return self.best_move;
                    }

                    if s <= a {
                        // Fail low
                        a = (a - delta).max(-INF);
                        delta += delta / 2 + 5;
                        failed_high_count = 0;
                        search_again_counter += 1;
                    } else if s >= b {
                        // Fail high
                        a = (a + s) / 2; // Tighter alpha
                        b = (s + delta).min(INF);
                        delta += delta / 2 + 5;
                        failed_high_count += 1;
                        search_again_counter += 1;
                    } else {
                        score = s;
                        break;
                    }

                    if delta > 1000 {
                        score = self.negamax(board, d, -INF, INF, 0, None, false);
                        break;
                    }
                }
            } else {
                score = self.negamax(board, d, -INF, INF, 0, None, false);
                if self.stop.load(Ordering::Relaxed) {
                    break;
                }
            }

            // Panic time: if score drops significantly, extend
            if d >= 6 && average_score - score > 50
                && self.soft_limit != Duration::from_secs(u64::MAX) {
                    let extra =
                        Duration::from_millis((self.soft_limit.as_millis() as u64).min(2000));
                    self.soft_limit += extra;
                }

            if d > 1 && self.best_move.0 != 0 && self.best_move.0 != prev_best_move.0 {
                best_move_changes += 1;
            }

            if d == 1 {
                average_score = score;
            } else {
                average_score = average_score + (score - average_score) / (d as i32);
            }

            self.prev_best_score = score;

            let elapsed = self.start_time.elapsed().as_millis();
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

            if self.thread_id == 0 {
                println!(
                    "info depth {} nodes {} time {} nps {} hashfull {} score {} pv {}",
                    d,
                    self.nodes,
                    elapsed,
                    nps,
                    hashfull,
                    score_str,
                    pv_str.trim()
                );
            }

            // bestMoveInstability: scale soft limit
            let mut effective_soft_limit = self.soft_limit;
            if self.soft_limit != Duration::from_secs(u64::MAX) {
                let instability_factor = 1.0 + 0.1 * (best_move_changes as f64).min(5.0);
                effective_soft_limit = Duration::from_millis(
                    (self.soft_limit.as_millis() as f64 * instability_factor) as u64,
                );

                // nodesEffort: if we spend too much time on a single move, reduce the limit to avoid wasting time
                if d > 5 {
                    let effort = self.best_move_nodes as f64 / (self.nodes.max(1) as f64);
                    if effort > 0.7 {
                        let scale = 1.6 - effort; // e.g., effort 0.9 -> limits time to 70%
                        effective_soft_limit = Duration::from_millis(
                            (effective_soft_limit.as_millis() as f64 * scale.max(0.5)) as u64,
                        );
                    }
                }
            }

            // Soft time check: stop if we've used most of our time
            if self.thread_id == 0 && self.start_time.elapsed() >= effective_soft_limit {
                break;
            }
        }

        if self.thread_id == 0 {
            self.stop.store(true, Ordering::Relaxed);
        }

        self.best_move
    }
}
