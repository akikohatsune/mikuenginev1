use crate::board::Board;
use crate::eval::PIECE_VALUES;
use crate::history::Heuristics;
use crate::movegen::{generate_pseudo_legal_moves, MoveList};
use crate::types::{Move, PieceType, Square};

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum MovePickerStage {
    TTMove,
    CapturesInit,
    GoodCaptures,
    Killers,
    CounterMove,
    QuietsInit,
    Quiets,
    BadCaptures,
    BadQuiets,
    EvasionTT,
    EvasionInit,
    Evasions,
    AllDone,
}

pub struct MovePicker {
    tt_move: Option<Move>,
    counter_move: Option<Move>,
    ply: usize,
    stage: MovePickerStage,
    quiescence: bool,

    list: MoveList,
    scores: [i32; 256],
    cur: usize,

    bad_captures: [Move; 256],
    bad_captures_count: usize,
    bad_quiets: [Move; 256],
    bad_quiets_count: usize,
}

impl MovePicker {
    pub fn new(
        tt_move: Option<Move>,
        ply: usize,
        counter_move: Option<Move>,
        quiescence: bool,
        in_check: bool,
    ) -> Self {
        let stage = if in_check {
            MovePickerStage::EvasionTT
        } else {
            MovePickerStage::TTMove
        };
        MovePicker {
            tt_move,
            counter_move,
            ply,
            stage,
            quiescence,
            list: MoveList::new(),
            scores: [0; 256],
            cur: 0,
            bad_captures: [Move(0); 256],
            bad_captures_count: 0,
            bad_quiets: [Move(0); 256],
            bad_quiets_count: 0,
        }
    }

    pub fn next(&mut self, heuristics: &Heuristics, board: &Board) -> Option<Move> {
        loop {
            match self.stage {
                MovePickerStage::TTMove => {
                    self.stage = MovePickerStage::CapturesInit;
                    if let Some(m) = self.tt_move {
                        if board.is_pseudo_legal(m) {
                            return Some(m);
                        }
                    }
                }
                MovePickerStage::CapturesInit => {
                    self.list.count = 0;
                    generate_pseudo_legal_moves(board, &mut self.list);
                    self.score_captures(heuristics, board);
                    self.stage = MovePickerStage::GoodCaptures;
                    self.cur = 0;
                }
                MovePickerStage::GoodCaptures => {
                    if let Some(m) = self.get_next_scored_move() {
                        if !m.is_capture() && !m.is_promotion() {
                            continue;
                        }
                        if self.is_tt_move(m) {
                            continue;
                        }

                        if board.see_ge(m, 0) {
                            return Some(m);
                        } else {
                            self.bad_captures[self.bad_captures_count] = m;
                            self.bad_captures_count += 1;
                            continue;
                        }
                    }
                    if self.quiescence {
                        self.stage = MovePickerStage::AllDone;
                    } else {
                        self.stage = MovePickerStage::Killers;
                    }
                    self.cur = 0;
                }
                MovePickerStage::Killers => {
                    if self.cur < 2 {
                        let killer = heuristics.killers[self.ply][self.cur];
                        self.cur += 1;
                        if killer.0 != 0
                            && !self.is_tt_move(killer)
                            && board.is_pseudo_legal(killer)
                            && !killer.is_capture()
                        {
                            return Some(killer);
                        }
                        continue;
                    }
                    self.stage = MovePickerStage::CounterMove;
                }
                MovePickerStage::CounterMove => {
                    self.stage = MovePickerStage::QuietsInit;
                    if let Some(cm) = self.counter_move {
                        if !self.is_tt_move(cm)
                            && !self.is_killer_move(heuristics, cm)
                            && board.is_pseudo_legal(cm)
                            && !cm.is_capture()
                        {
                            return Some(cm);
                        }
                    }
                }
                MovePickerStage::QuietsInit => {
                    self.score_quiets(heuristics, board);
                    self.stage = MovePickerStage::Quiets;
                    self.cur = 0;
                }
                MovePickerStage::Quiets => {
                    if let Some(m) = self.get_next_scored_move() {
                        if m.is_capture() || m.is_promotion() {
                            continue;
                        }
                        if self.is_tt_move(m)
                            || self.is_killer_move(heuristics, m)
                            || self.is_counter_move(m)
                        {
                            continue;
                        }

                        let score = self.scores[self.cur - 1];
                        if score > -14000 {
                            return Some(m);
                        } else {
                            self.bad_quiets[self.bad_quiets_count] = m;
                            self.bad_quiets_count += 1;
                            continue;
                        }
                    }
                    self.stage = MovePickerStage::BadCaptures;
                    self.cur = 0;
                }
                MovePickerStage::BadCaptures => {
                    if self.cur < self.bad_captures_count {
                        let m = self.bad_captures[self.cur];
                        self.cur += 1;
                        return Some(m);
                    }
                    self.stage = MovePickerStage::BadQuiets;
                    self.cur = 0;
                }
                MovePickerStage::BadQuiets => {
                    if self.cur < self.bad_quiets_count {
                        let m = self.bad_quiets[self.cur];
                        self.cur += 1;
                        return Some(m);
                    }
                    self.stage = MovePickerStage::AllDone;
                }
                MovePickerStage::EvasionTT => {
                    self.stage = MovePickerStage::EvasionInit;
                    if let Some(m) = self.tt_move {
                        if board.is_pseudo_legal(m) {
                            return Some(m);
                        }
                    }
                }
                MovePickerStage::EvasionInit => {
                    self.list.count = 0;
                    generate_pseudo_legal_moves(board, &mut self.list);
                    self.score_evasions(heuristics, board);
                    self.stage = MovePickerStage::Evasions;
                    self.cur = 0;
                }
                MovePickerStage::Evasions => {
                    if let Some(m) = self.get_next_scored_move() {
                        if self.is_tt_move(m) {
                            continue;
                        }
                        return Some(m);
                    }
                    self.stage = MovePickerStage::AllDone;
                }
                MovePickerStage::AllDone => return None,
            }
        }
    }

    fn score_captures(&mut self, heuristics: &Heuristics, board: &Board) {
        for i in 0..self.list.count {
            let m = self.list.moves[i];
            if m.is_capture() || m.is_promotion() {
                let to_sq = m.to_sq();
                let attacker_pt = match board.piece_on_sq[m.from_sq() as usize] {
                    Some(p) => p.piece_type(),
                    None => {
                        self.scores[i] = -20_000_000;
                        continue;
                    }
                };
                let victim_pt = if m.is_en_passant() {
                    PieceType::Pawn
                } else {
                    board.piece_on_sq[to_sq as usize]
                        .map(|p| p.piece_type())
                        .unwrap_or(PieceType::Pawn)
                };

                let cap_hist =
                    heuristics.get_capture_history(attacker_pt, Square::new(to_sq), victim_pt);
                self.scores[i] = 10_000_000 + PIECE_VALUES[victim_pt as usize] * 10
                    - PIECE_VALUES[attacker_pt as usize]
                    + cap_hist / 32;
            } else {
                self.scores[i] = -20_000_000; // Low priority
            }
        }
    }

    fn score_quiets(&mut self, heuristics: &Heuristics, board: &Board) {
        let side = board.side_to_move;

        let opp_king_bb = board.piece_bb(PieceType::King) & board.color_occupancy(side.flip());
        let opp_king_sq = if opp_king_bb.is_not_empty() {
            Some(Square::new(opp_king_bb.lsb()))
        } else {
            None
        };

        for i in 0..self.list.count {
            let m = self.list.moves[i];
            if !m.is_capture() && !m.is_promotion() {
                let attacker_pt = match board.piece_on_sq[m.from_sq() as usize] {
                    Some(p) => p.piece_type(),
                    None => {
                        self.scores[i] = -20_000_000;
                        continue;
                    }
                };
                let mut h = heuristics.get_history(side, attacker_pt, Square::new(m.to_sq()));

                if let Some(king_sq) = opp_king_sq {
                    let to = Square::new(m.to_sq());
                    let occ_after = (board.occupancies().0 & !(1 << m.from_sq())) | (1 << to.0);
                    let gives_check = match attacker_pt {
                        PieceType::Knight => {
                            (crate::attacks::knight_attacks(to).0 & (1 << king_sq.0)) != 0
                        }
                        PieceType::Bishop => {
                            (crate::attacks::bishop_attacks(
                                to,
                                crate::bitboard::Bitboard::new(occ_after),
                            )
                            .0 & (1 << king_sq.0))
                                != 0
                        }
                        PieceType::Rook => {
                            (crate::attacks::rook_attacks(
                                to,
                                crate::bitboard::Bitboard::new(occ_after),
                            )
                            .0 & (1 << king_sq.0))
                                != 0
                        }
                        PieceType::Queen => {
                            (crate::attacks::queen_attacks(
                                to,
                                crate::bitboard::Bitboard::new(occ_after),
                            )
                            .0 & (1 << king_sq.0))
                                != 0
                        }
                        PieceType::Pawn => {
                            (crate::attacks::pawn_attacks(side, to).0 & (1 << king_sq.0)) != 0
                        }
                        _ => false,
                    };
                    if gives_check {
                        h += 16384;
                    }
                }

                // Optimization: could add continuation history here if available
                self.scores[i] = h;
            } else {
                self.scores[i] = -20_000_000;
            }
        }
    }

    fn score_evasions(&mut self, heuristics: &Heuristics, board: &Board) {
        let side = board.side_to_move;
        for i in 0..self.list.count {
            let m = self.list.moves[i];
            if m.is_capture() {
                let to_sq = m.to_sq();
                let attacker_pt = match board.piece_on_sq[m.from_sq() as usize] {
                    Some(p) => p.piece_type(),
                    None => {
                        self.scores[i] = -20_000_000;
                        continue;
                    }
                };
                let victim_pt = if m.is_en_passant() {
                    PieceType::Pawn
                } else {
                    board.piece_on_sq[to_sq as usize]
                        .map(|p| p.piece_type())
                        .unwrap_or(PieceType::Pawn)
                };
                let cap_hist =
                    heuristics.get_capture_history(attacker_pt, Square::new(to_sq), victim_pt);
                self.scores[i] = 10_000_000 + PIECE_VALUES[victim_pt as usize] * 10
                    - PIECE_VALUES[attacker_pt as usize]
                    + cap_hist / 32;
            } else {
                let attacker_pt = match board.piece_on_sq[m.from_sq() as usize] {
                    Some(p) => p.piece_type(),
                    None => PieceType::Pawn,
                };
                self.scores[i] = heuristics.get_history(side, attacker_pt, Square::new(m.to_sq()));
            }
        }
    }

    fn get_next_scored_move(&mut self) -> Option<Move> {
        if self.cur < self.list.count {
            let mut best_idx = self.cur;
            let mut best_score = self.scores[self.cur];
            for i in (self.cur + 1)..self.list.count {
                if self.scores[i] > best_score {
                    best_score = self.scores[i];
                    best_idx = i;
                }
            }
            
            self.scores.swap(self.cur, best_idx);
            self.list.moves.swap(self.cur, best_idx);

            let m = self.list.moves[self.cur];
            self.cur += 1;
            Some(m)
        } else {
            None
        }
    }

    fn is_tt_move(&self, m: Move) -> bool {
        self.tt_move.map(|ttm| ttm.0 == m.0).unwrap_or(false)
    }

    fn is_killer_move(&self, heuristics: &Heuristics, m: Move) -> bool {
        heuristics.is_killer(m, self.ply)
    }

    fn is_counter_move(&self, m: Move) -> bool {
        self.counter_move.map(|cm| cm.0 == m.0).unwrap_or(false)
    }

    pub fn skip_quiets(&mut self) {
        if self.stage == MovePickerStage::QuietsInit || self.stage == MovePickerStage::Quiets {
            self.stage = MovePickerStage::BadCaptures;
            self.cur = 0;
        }
    }
}
