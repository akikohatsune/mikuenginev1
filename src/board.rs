use crate::bitboard::Bitboard;
use crate::types::{
    CastlingRights, Color, Move, Piece, PieceType, Square, NUM_COLORS, NUM_PIECE_TYPES,
};
use crate::zobrist;

use crate::attacks;
use crate::nnue::{feature_index_for_perspective, Accumulator, NNUE};

use std::sync::Arc;

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Clone)]
pub struct UndoState {
    pub en_passant: Option<Square>,
    pub castling: CastlingRights,
    pub halfmove_clock: u8,
    pub captured_piece: Option<PieceType>,
    pub zobrist_key: u64,
    pub accumulator: Accumulator,
}

#[derive(Clone)]
pub struct Board {
    pub pieces: [Bitboard; NUM_PIECE_TYPES],
    pub colors: [Bitboard; NUM_COLORS],

    pub piece_on_sq: [Option<Piece>; 64],

    pub side_to_move: Color,
    pub en_passant: Option<Square>,
    pub castling: CastlingRights,

    pub halfmove_clock: u8,
    pub fullmove_number: u16,

    pub zobrist_key: u64,

    pub accumulator: Accumulator,
    pub nnue: Arc<NNUE>,

    pub position_history: Vec<u64>,
}

impl Board {
    pub fn new(nnue: Arc<NNUE>) -> Self {
        Self::from_fen(START_FEN, nnue).unwrap()
    }

    pub fn empty(nnue: Arc<NNUE>) -> Self {
        Board {
            pieces: [Bitboard::EMPTY; NUM_PIECE_TYPES],
            colors: [Bitboard::EMPTY; NUM_COLORS],
            piece_on_sq: [None; 64],
            side_to_move: Color::White,
            en_passant: None,
            castling: CastlingRights::new(0),
            halfmove_clock: 0,
            fullmove_number: 1,
            zobrist_key: 0,
            accumulator: Accumulator::new(),
            nnue,
            position_history: Vec::with_capacity(512),
        }
    }

    pub fn is_repetition(&self) -> bool {
        let len = self.position_history.len();
        if len < 4 {
            return false;
        }

        // Only check back as far as the halfmove clock allows (50-move rule)
        let limit = len.saturating_sub(self.halfmove_clock as usize);

        // Check positions at even intervals (same side to move)
        let mut i = len - 2;
        while i >= limit {
            if self.position_history[i] == self.zobrist_key {
                return true;
            }
            if i < 2 {
                break;
            }
            i -= 2;
        }
        false
    }

    pub fn is_pseudo_legal(&self, m: Move) -> bool {
        let from = Square::new(m.from_sq());
        let to = Square::new(m.to_sq());

        let piece = match self.piece_on_sq[from.0 as usize] {
            Some(p) => p,
            None => return false,
        };
        if piece.color() != self.side_to_move {
            return false;
        }

        if let Some(target) = self.piece_on_sq[to.0 as usize] {
            if target.color() == self.side_to_move {
                return false;
            }
            if !m.is_capture() {
                return false;
            } // Must have capture flag
        } else {
            if m.is_capture() && m.flag() != Move::FLAG_EP {
                return false;
            } // Must NOT have capture flag (unless EP)
        }

        let pt = piece.piece_type();
        let target_occ = self.occupancies();

        if pt == PieceType::Pawn {
            let is_promo = to.rank() == 0 || to.rank() == 7;
            if m.is_promotion() != is_promo {
                return false;
            }

            if m.flag() == Move::FLAG_EP {
                if self.en_passant != Some(to) {
                    return false;
                }
                if (attacks::pawn_attacks(self.side_to_move, from).0 & (1 << to.0)) == 0 {
                    return false;
                }
                return true;
            }

            if m.is_capture() {
                if (attacks::pawn_attacks(self.side_to_move, from).0 & (1 << to.0)) == 0 {
                    return false;
                }
            } else {
                if self.side_to_move == Color::White {
                    if to.0 == from.0 + 8 {
                    } else if to.0 == from.0 + 16 && from.rank() == 1 {
                        if self.piece_on_sq[(from.0 + 8) as usize].is_some() {
                            return false;
                        }
                        if m.flag() != Move::FLAG_DBL_PUSH {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    if to.0 == from.0 - 8 {
                    } else if to.0 == from.0 - 16 && from.rank() == 6 {
                        if self.piece_on_sq[(from.0 - 8) as usize].is_some() {
                            return false;
                        }
                        if m.flag() != Move::FLAG_DBL_PUSH {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        } else if pt == PieceType::Knight {
            if (attacks::knight_attacks(from).0 & (1 << to.0)) == 0 {
                return false;
            }
            if m.flag() != Move::FLAG_QUIET && m.flag() != Move::FLAG_CAPTURE {
                return false;
            }
        } else if pt == PieceType::Bishop {
            if (attacks::bishop_attacks(from, target_occ).0 & (1 << to.0)) == 0 {
                return false;
            }
            if m.flag() != Move::FLAG_QUIET && m.flag() != Move::FLAG_CAPTURE {
                return false;
            }
        } else if pt == PieceType::Rook {
            if (attacks::rook_attacks(from, target_occ).0 & (1 << to.0)) == 0 {
                return false;
            }
            if m.flag() != Move::FLAG_QUIET && m.flag() != Move::FLAG_CAPTURE {
                return false;
            }
        } else if pt == PieceType::Queen {
            if (attacks::queen_attacks(from, target_occ).0 & (1 << to.0)) == 0 {
                return false;
            }
            if m.flag() != Move::FLAG_QUIET && m.flag() != Move::FLAG_CAPTURE {
                return false;
            }
        } else if pt == PieceType::King {
            if m.flag() == Move::FLAG_K_CASTLE || m.flag() == Move::FLAG_Q_CASTLE {
                if self.side_to_move == Color::White {
                    if m.flag() == Move::FLAG_K_CASTLE {
                        if !self.castling.has_wk() || to.0 != 6 || (target_occ.0 & 0x60) != 0 {
                            return false;
                        }
                    } else {
                        if !self.castling.has_wq() || to.0 != 2 || (target_occ.0 & 0x0E) != 0 {
                            return false;
                        }
                    }
                } else {
                    if m.flag() == Move::FLAG_K_CASTLE {
                        if !self.castling.has_bk()
                            || to.0 != 62
                            || (target_occ.0 & 0x6000000000000000) != 0
                        {
                            return false;
                        }
                    } else {
                        if !self.castling.has_bq()
                            || to.0 != 58
                            || (target_occ.0 & 0x0E00000000000000) != 0
                        {
                            return false;
                        }
                    }
                }
                return true;
            }
            if (attacks::king_attacks(from).0 & (1 << to.0)) == 0 {
                return false;
            }
            if m.flag() != Move::FLAG_QUIET && m.flag() != Move::FLAG_CAPTURE {
                return false;
            }
        }

        true
    }

    pub fn is_castling_legal(&self, m: Move) -> bool {
        if m.flag() != Move::FLAG_K_CASTLE && m.flag() != Move::FLAG_Q_CASTLE {
            return true;
        }

        let side = self.side_to_move;
        let opp = side.flip();

        let (king_from, king_to) = (m.from_sq(), m.to_sq());

        // 1. Check if currently in check
        let king_sq = Square::new(king_from);
        if self.is_square_attacked(king_sq, opp) {
            return false;
        }

        // 2. Check the passing square
        let passing_sq = if king_to > king_from {
            Square::new(king_from + 1)
        } else {
            Square::new(king_from - 1)
        };

        if self.is_square_attacked(passing_sq, opp) {
            return false;
        }

        // 3. Final square is checked securely by the main legality check that comes AFTER make_move,
        // but it doesn't hurt to check it here too for early pruning.
        let dest_sq = Square::new(king_to);
        if self.is_square_attacked(dest_sq, opp) {
            return false;
        }

        true
    }

    pub fn refresh_accumulator(&mut self) {
        let wk_sq_opt = self.color_piece_bb(Color::White, PieceType::King).lsb();
        let bk_sq_opt = self.color_piece_bb(Color::Black, PieceType::King).lsb();

        if wk_sq_opt == 64 || bk_sq_opt == 64 {
            return;
        } // Empty kings, invalid

        let wk_sq = Square::new(wk_sq_opt);
        let bk_sq = Square::new(bk_sq_opt);

        // Collect features for both perspectives (HalfKAv2_hm includes all pieces, including kings)
        let mut white_features = Vec::with_capacity(32);
        let mut black_features = Vec::with_capacity(32);

        // O(1) Bit-scan iteration over all occupied squares
        let mut occ = self.occupancies();
        while occ.is_not_empty() {
            let sq = occ.pop_lsb();
            if let Some(piece) = self.piece_on_sq[sq as usize] {
                let pt = piece.piece_type();
                let pc = piece.color();
                let piece_sq = Square::new(sq);

                // White perspective: king on wk_sq
                white_features.push(feature_index_for_perspective(
                    Color::White, wk_sq, piece_sq, pt, pc,
                ));
                // Black perspective: king on bk_sq
                black_features.push(feature_index_for_perspective(
                    Color::Black, bk_sq, piece_sq, pt, pc,
                ));
            }
        }

        self.accumulator.refresh(
            &white_features,
            &black_features,
            &self.nnue.params,
        );
    }


    pub fn compute_hash(&self) -> u64 {
        let mut h = 0;

        for pt in 0..NUM_PIECE_TYPES {
            for color in [Color::White, Color::Black] {
                let mut bb = self.color_piece_bb(
                    color,
                    match pt {
                        0 => PieceType::Pawn,
                        1 => PieceType::Knight,
                        2 => PieceType::Bishop,
                        3 => PieceType::Rook,
                        4 => PieceType::Queen,
                        5 => PieceType::King,
                        _ => unreachable!(),
                    },
                );
                while bb.is_not_empty() {
                    let sq = Square::new(bb.pop_lsb());
                    h ^= zobrist::piece_key(
                        color,
                        match pt {
                            0 => PieceType::Pawn,
                            1 => PieceType::Knight,
                            2 => PieceType::Bishop,
                            3 => PieceType::Rook,
                            4 => PieceType::Queen,
                            5 => PieceType::King,
                            _ => unreachable!(),
                        },
                        sq,
                    );
                }
            }
        }

        if self.side_to_move == Color::Black {
            h ^= zobrist::side_key();
        }

        h ^= zobrist::castling_key(self.castling);

        if let Some(ep) = self.en_passant {
            h ^= zobrist::ep_key(ep);
        }

        h
    }

    #[inline(always)]
    pub fn occupancies(&self) -> Bitboard {
        self.colors[Color::White as usize] | self.colors[Color::Black as usize]
    }

    #[inline(always)]
    pub fn color_occupancy(&self, color: Color) -> Bitboard {
        self.colors[color as usize]
    }

    #[inline(always)]
    pub fn piece_bb(&self, pt: PieceType) -> Bitboard {
        self.pieces[pt as usize]
    }

    #[inline(always)]
    pub fn color_piece_bb(&self, color: Color, pt: PieceType) -> Bitboard {
        self.pieces[pt as usize] & self.colors[color as usize]
    }

    pub fn non_pawn_material(&self, color: Color) -> i32 {
        let mut mat = 0;
        mat += self.color_piece_bb(color, PieceType::Knight).count() as i32
            * crate::eval::PIECE_VALUES[PieceType::Knight as usize];
        mat += self.color_piece_bb(color, PieceType::Bishop).count() as i32
            * crate::eval::PIECE_VALUES[PieceType::Bishop as usize];
        mat += self.color_piece_bb(color, PieceType::Rook).count() as i32
            * crate::eval::PIECE_VALUES[PieceType::Rook as usize];
        mat += self.color_piece_bb(color, PieceType::Queen).count() as i32
            * crate::eval::PIECE_VALUES[PieceType::Queen as usize];
        mat
    }

    pub fn put_piece(&mut self, piece: Piece, sq: Square) {
        let pt = piece.piece_type();
        let color = piece.color();

        self.pieces[pt as usize].set_bit(sq.0);
        self.colors[color as usize].set_bit(sq.0);
        self.piece_on_sq[sq.0 as usize] = Some(piece);
        self.zobrist_key ^= zobrist::piece_key(color, pt, sq);
    }

    pub fn remove_piece(&mut self, sq: Square) {
        if let Some(piece) = self.piece_on_sq[sq.0 as usize] {
            let pt = piece.piece_type();
            let color = piece.color();

            self.pieces[pt as usize].clear_bit(sq.0);
            self.colors[color as usize].clear_bit(sq.0);
            self.piece_on_sq[sq.0 as usize] = None;
            self.zobrist_key ^= zobrist::piece_key(color, pt, sq);
        }
    }

    pub fn make_move(&mut self, m: Move) -> UndoState {
        let from = Square::new(m.from_sq());
        let to = Square::new(m.to_sq());

        let moving_piece = match self.piece_on_sq[from.0 as usize] {
            Some(p) => p,
            None => {
                panic!(
                    "WARNING: make_move called but no piece on from_sq={} for move {:?}",
                    from.0, m
                );
            }
        };
        let color = moving_piece.color();
        let pt = moving_piece.piece_type();

        let captured_piece = self.piece_on_sq[to.0 as usize].map(|p| p.piece_type());

        let undo = UndoState {
            en_passant: self.en_passant,
            castling: self.castling,
            halfmove_clock: self.halfmove_clock,
            captured_piece,
            zobrist_key: self.zobrist_key,
            accumulator: self.accumulator.clone(),
        };

        self.position_history.push(self.zobrist_key);

        // Update halfmove clock
        if pt == PieceType::Pawn || captured_piece.is_some() {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }

        // Remove ep hash if any
        if let Some(ep_sq) = self.en_passant {
            self.zobrist_key ^= zobrist::ep_key(ep_sq);
        }
        self.en_passant = None;

        let wk_sq_opt = self.color_piece_bb(Color::White, PieceType::King).lsb();
        let bk_sq_opt = self.color_piece_bb(Color::Black, PieceType::King).lsb();

        let wk_sq = Square::new(wk_sq_opt);
        let bk_sq = Square::new(bk_sq_opt);

        // Move piece incrementally in NNUE (if it's not a king)
        if pt != PieceType::King {
            let rm_w = feature_index_for_perspective(Color::White, wk_sq, from, pt, color);
            let rm_b = feature_index_for_perspective(Color::Black, bk_sq, from, pt, color);
            self.accumulator
                .remove_feature(rm_w, rm_b, &self.nnue.params);

            let add_w = feature_index_for_perspective(Color::White, wk_sq, to, pt, color);
            let add_b = feature_index_for_perspective(Color::Black, bk_sq, to, pt, color);
            self.accumulator
                .add_feature(add_w, add_b, &self.nnue.params);
        }

        self.remove_piece(from);

        if let Some(c_pt) = captured_piece {
            if c_pt != PieceType::King {
                let rmc_w = feature_index_for_perspective(Color::White, wk_sq, to, c_pt, color.flip());
                let rmc_b = feature_index_for_perspective(Color::Black, bk_sq, to, c_pt, color.flip());
                self.accumulator
                    .remove_feature(rmc_w, rmc_b, &self.nnue.params);
            }
            self.remove_piece(to);
        }

        self.put_piece(moving_piece, to);

        // Handle special moves
        if m.flag() == Move::FLAG_EP {
            // En-passant capture
            let ep_cap_sq = if color == Color::White {
                Square::new(to.0 - 8)
            } else {
                Square::new(to.0 + 8)
            };

            // Remove captured ep pawn from NNUE
            let rme_w = feature_index_for_perspective(Color::White, wk_sq, ep_cap_sq, PieceType::Pawn, color.flip());
            let rme_b = feature_index_for_perspective(Color::Black, bk_sq, ep_cap_sq, PieceType::Pawn, color.flip());
            self.accumulator
                .remove_feature(rme_w, rme_b, &self.nnue.params);

            self.remove_piece(ep_cap_sq);
        } else if m.flag() == Move::FLAG_DBL_PUSH {
            // Double pawn push
            let ep_sq = if color == Color::White {
                Square::new(to.0 - 8)
            } else {
                Square::new(to.0 + 8)
            };
            self.en_passant = Some(ep_sq);
            self.zobrist_key ^= zobrist::ep_key(ep_sq);
        } else if m.flag() == Move::FLAG_K_CASTLE || m.flag() == Move::FLAG_Q_CASTLE {
            // Castling
            let (rook_from, rook_to) = match to.0 {
                // White kingside
                6 => (Square::new(7), Square::new(5)),
                // White queenside
                2 => (Square::new(0), Square::new(3)),
                // Black kingside
                62 => (Square::new(63), Square::new(61)),
                // Black queenside
                58 => (Square::new(56), Square::new(59)),
                _ => unreachable!(),
            };
            let rook = match self.piece_on_sq[rook_from.0 as usize] {
                Some(r) => r,
                None => {
                    panic!("WARNING: make_move castling but no rook on rook_from={} (m={:?}, history={})", rook_from.0, m, self.position_history.len());
                }
            };

            // NNUE incremental update for castled rook
            let rmr_w = feature_index_for_perspective(Color::White, wk_sq, rook_from, PieceType::Rook, color);
            let rmr_b = feature_index_for_perspective(Color::Black, bk_sq, rook_from, PieceType::Rook, color);
            self.accumulator
                .remove_feature(rmr_w, rmr_b, &self.nnue.params);

            let addr_w = feature_index_for_perspective(Color::White, wk_sq, rook_to, PieceType::Rook, color);
            let addr_b = feature_index_for_perspective(Color::Black, bk_sq, rook_to, PieceType::Rook, color);
            self.accumulator
                .add_feature(addr_w, addr_b, &self.nnue.params);

            self.remove_piece(rook_from);
            self.put_piece(rook, rook_to);
        }

        // Promotions
        if m.is_promotion() {
            let promo_pt = m.promotion_type();

            // Fix NNUE promotion override: Replace the inserted pawn with the promoted piece
            let rmp_w = feature_index_for_perspective(Color::White, wk_sq, to, PieceType::Pawn, color);
            let rmp_b = feature_index_for_perspective(Color::Black, bk_sq, to, PieceType::Pawn, color);
            self.accumulator
                .remove_feature(rmp_w, rmp_b, &self.nnue.params);

            let addp_w = feature_index_for_perspective(Color::White, wk_sq, to, promo_pt, color);
            let addp_b = feature_index_for_perspective(Color::Black, bk_sq, to, promo_pt, color);
            self.accumulator
                .add_feature(addp_w, addp_b, &self.nnue.params);

            self.remove_piece(to);
            self.put_piece(Piece::new(color, promo_pt), to);
        }

        // If the king moved, perspective shifted completely. We must trigger a full Accumulator refresh.
        // It's cheaper to refresh fully than moving 32 different features manually when King moves.
        if pt == PieceType::King {
            self.refresh_accumulator();
        }

        // Update castling rights
        self.zobrist_key ^= zobrist::castling_key(self.castling);
        self.update_castling_rights(from);
        self.update_castling_rights(to);
        self.zobrist_key ^= zobrist::castling_key(self.castling);

        // Next turn
        self.side_to_move = self.side_to_move.flip();
        self.zobrist_key ^= zobrist::side_key();
        if self.side_to_move == Color::White {
            self.fullmove_number += 1;
        }

        undo
    }

    pub fn unmake_move(&mut self, m: Move, undo: &UndoState) {
        let from = Square::new(m.from_sq());
        let to = Square::new(m.to_sq());

        self.side_to_move = self.side_to_move.flip();
        if self.side_to_move == Color::Black {
            self.fullmove_number -= 1;
        }

        let moved_piece = match self.piece_on_sq[to.0 as usize] {
            Some(p) => p,
            None => {
                panic!(
                    "WARNING: unmake_move called but no piece on to_sq={} for move {:?}",
                    to.0, m
                );
            }
        };
        let color = moved_piece.color();
        let pt = moved_piece.piece_type();

        // Standard unmake step 1: Move piece back
        self.remove_piece(to);

        let orig_pt = if m.is_promotion() {
            PieceType::Pawn
        } else {
            pt
        };
        self.put_piece(Piece::new(color, orig_pt), from);

        // Step 2: Handle captures (including en passant)
        if m.flag() == Move::FLAG_EP {
            // En passant capture
            let ep_cap_sq = if color == Color::White {
                Square::new(to.0 - 8)
            } else {
                Square::new(to.0 + 8)
            };
            self.put_piece(Piece::new(color.flip(), PieceType::Pawn), ep_cap_sq);
        } else if let Some(cap_pt) = undo.captured_piece {
            self.put_piece(Piece::new(color.flip(), cap_pt), to);
        }

        // Step 3: Handle Castling
        if m.flag() == Move::FLAG_K_CASTLE || m.flag() == Move::FLAG_Q_CASTLE {
            let (rook_from, rook_to) = match to.0 {
                // White kingside
                6 => (Square::new(7), Square::new(5)),
                // White queenside
                2 => (Square::new(0), Square::new(3)),
                // Black kingside
                62 => (Square::new(63), Square::new(61)),
                // Black queenside
                58 => (Square::new(56), Square::new(59)),
                _ => unreachable!(),
            };
            let rook = match self.piece_on_sq[rook_to.0 as usize] {
                Some(r) => r,
                None => {
                    panic!(
                        "WARNING: unmake castling but no rook on rook_to={}",
                        rook_to.0
                    );
                }
            };
            self.remove_piece(rook_to);
            self.put_piece(rook, rook_from);
        }

        self.en_passant = undo.en_passant;
        self.castling = undo.castling;
        self.halfmove_clock = undo.halfmove_clock;
        self.zobrist_key = undo.zobrist_key;

        // Restore NNUE Accumulator instantly in O(1) time
        self.accumulator = undo.accumulator.clone();
        self.position_history.pop();
    }

    pub fn make_null_move(&mut self) -> UndoState {
        let undo = UndoState {
            en_passant: self.en_passant,
            castling: self.castling,
            halfmove_clock: self.halfmove_clock,
            captured_piece: None,
            zobrist_key: self.zobrist_key,
            accumulator: Accumulator::new(), // Not used for null moves
        };

        if let Some(ep_sq) = self.en_passant {
            self.zobrist_key ^= zobrist::ep_key(ep_sq);
        }
        self.en_passant = None;

        self.side_to_move = self.side_to_move.flip();
        self.zobrist_key ^= zobrist::side_key();

        undo
    }

    pub fn unmake_null_move(&mut self, undo: &UndoState) {
        self.side_to_move = self.side_to_move.flip();
        self.en_passant = undo.en_passant;
        self.castling = undo.castling;
        self.halfmove_clock = undo.halfmove_clock;
        self.zobrist_key = undo.zobrist_key;

        // Accumulator is unchanged by null move, no need to restore
    }

    pub fn is_square_attacked(&self, sq: Square, by_color: Color) -> bool {
        use crate::attacks;

        let occ = self.occupancies();

        // Check pawn attacks
        let pawns = self.color_piece_bb(by_color, PieceType::Pawn);
        if (attacks::pawn_attacks(by_color.flip(), sq) & pawns).is_not_empty() {
            return true;
        }

        // Check knight attacks
        let knights = self.color_piece_bb(by_color, PieceType::Knight);
        if (attacks::knight_attacks(sq) & knights).is_not_empty() {
            return true;
        }

        // Check king attacks
        let kings = self.color_piece_bb(by_color, PieceType::King);
        if (attacks::king_attacks(sq) & kings).is_not_empty() {
            return true;
        }

        // Check slider attacks (Bishop, Rook, Queen)
        let bishops_queens = self.color_piece_bb(by_color, PieceType::Bishop)
            | self.color_piece_bb(by_color, PieceType::Queen);
        if (attacks::bishop_attacks(sq, occ) & bishops_queens).is_not_empty() {
            return true;
        }

        let rooks_queens = self.color_piece_bb(by_color, PieceType::Rook)
            | self.color_piece_bb(by_color, PieceType::Queen);
        if (attacks::rook_attacks(sq, occ) & rooks_queens).is_not_empty() {
            return true;
        }

        false
    }

    fn update_castling_rights(&mut self, sq: Square) {
        let mut rights = self.castling.0;
        match sq.0 {
            0 => rights &= !CastlingRights::WQ,
            4 => rights &= !(CastlingRights::WK | CastlingRights::WQ),
            7 => rights &= !CastlingRights::WK,
            56 => rights &= !CastlingRights::BQ,
            60 => rights &= !(CastlingRights::BK | CastlingRights::BQ),
            63 => rights &= !CastlingRights::BK,
            _ => (),
        }
        self.castling = CastlingRights::new(rights);
    }

    pub fn attackers_to(&self, sq: Square, occ: Bitboard) -> Bitboard {
        use crate::attacks;
        let mut attackers = Bitboard::EMPTY;

        attackers |= attacks::pawn_attacks(Color::Black, sq)
            & self.color_piece_bb(Color::White, PieceType::Pawn);
        attackers |= attacks::pawn_attacks(Color::White, sq)
            & self.color_piece_bb(Color::Black, PieceType::Pawn);
        attackers |= attacks::knight_attacks(sq) & self.piece_bb(PieceType::Knight);
        attackers |= attacks::king_attacks(sq) & self.piece_bb(PieceType::King);
        attackers |= attacks::bishop_attacks(sq, occ)
            & (self.piece_bb(PieceType::Bishop) | self.piece_bb(PieceType::Queen));
        attackers |= attacks::rook_attacks(sq, occ)
            & (self.piece_bb(PieceType::Rook) | self.piece_bb(PieceType::Queen));

        attackers
    }

    pub fn see_ge(&self, m: Move, threshold: i32) -> bool {
        use crate::types::SEE_PIECE_VALUES;

        let from = Square::new(m.from_sq());
        let to = Square::new(m.to_sq());

        let mut swap = if m.is_en_passant() {
            SEE_PIECE_VALUES[PieceType::Pawn as usize] - threshold
        } else {
            match self.piece_on_sq[to.0 as usize] {
                Some(p) => SEE_PIECE_VALUES[p.piece_type() as usize] - threshold,
                None => -threshold,
            }
        };

        if swap < 0 {
            return false;
        }

        let moving_piece = match self.piece_on_sq[from.0 as usize] {
            Some(p) => p.piece_type(),
            None => return false, // No piece on from — treat as bad SEE
        };
        swap = SEE_PIECE_VALUES[moving_piece as usize] - swap;
        if swap <= 0 {
            return true;
        }

        let mut occupied =
            self.occupancies() ^ Bitboard::new(1u64 << from.0) ^ Bitboard::new(1u64 << to.0);
        let mut attackers = self.attackers_to(to, occupied);
        let mut stm = self.side_to_move.flip();

        let mut res = 1;

        loop {
            let stm_attackers = attackers & self.color_occupancy(stm);
            if stm_attackers.is_empty() {
                break;
            }

            let mut attacker_sq = 64;
            let mut attacker_pt = PieceType::King;

            for pt in [
                PieceType::Pawn,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Rook,
                PieceType::Queen,
                PieceType::King,
            ] {
                let pt_attackers = stm_attackers & self.piece_bb(pt);
                if pt_attackers.is_not_empty() {
                    attacker_sq = pt_attackers.lsb();
                    attacker_pt = pt;
                    break;
                }
            }

            if attacker_pt == PieceType::King {
                return (attackers & self.color_occupancy(stm.flip())).is_empty() == (res != 0);
            }

            res ^= 1;
            swap = SEE_PIECE_VALUES[attacker_pt as usize] - swap;
            if swap < res {
                break;
            }

            occupied ^= Bitboard::new(1u64 << attacker_sq);
            if attacker_pt == PieceType::Pawn
                || attacker_pt == PieceType::Bishop
                || attacker_pt == PieceType::Queen
            {
                attackers |= crate::attacks::bishop_attacks(to, occupied)
                    & (self.piece_bb(PieceType::Bishop) | self.piece_bb(PieceType::Queen));
            }
            if attacker_pt == PieceType::Rook || attacker_pt == PieceType::Queen {
                attackers |= crate::attacks::rook_attacks(to, occupied)
                    & (self.piece_bb(PieceType::Rook) | self.piece_bb(PieceType::Queen));
            }

            attackers &= occupied;
            stm = stm.flip();
        }

        res != 0
    }

    pub fn from_fen(fen: &str, nnue: std::sync::Arc<crate::nnue::NNUE>) -> Option<Self> {
        let mut board = Board::empty(nnue);
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let mut rank = 7_i32;
        let mut file = 0_i32;

        // 1. Pieces
        for ch in parts[0].chars() {
            if ch == '/' {
                rank -= 1;
                file = 0;
            } else if ch.is_ascii_digit() {
                file += ch.to_digit(10).unwrap() as i32;
            } else {
                let color = if ch.is_uppercase() {
                    Color::White
                } else {
                    Color::Black
                };
                let pt = match ch.to_ascii_lowercase() {
                    'p' => PieceType::Pawn,
                    'n' => PieceType::Knight,
                    'b' => PieceType::Bishop,
                    'r' => PieceType::Rook,
                    'q' => PieceType::Queen,
                    'k' => PieceType::King,
                    _ => return None,
                };
                let sq = Square::new((rank * 8 + file) as u8);
                board.put_piece(Piece::new(color, pt), sq);
                file += 1;
            }
        }

        // 2. Side to move
        board.side_to_move = if parts[1] == "w" {
            Color::White
        } else {
            Color::Black
        };
        if board.side_to_move == Color::Black {
            board.zobrist_key ^= zobrist::side_key();
        }

        // 3. Castling
        let mut c_rights = 0;
        for ch in parts[2].chars() {
            match ch {
                'K' => c_rights |= CastlingRights::WK,
                'Q' => c_rights |= CastlingRights::WQ,
                'k' => c_rights |= CastlingRights::BK,
                'q' => c_rights |= CastlingRights::BQ,
                _ => (),
            }
        }
        board.castling = CastlingRights::new(c_rights);
        board.zobrist_key ^= zobrist::castling_key(board.castling);

        // 4. En passant
        if parts[3] != "-" {
            let file = (parts[3].chars().nth(0).unwrap() as u8) - b'a';
            let rank = (parts[3].chars().nth(1).unwrap() as u8) - b'1';
            let sq = Square::new(rank * 8 + file);
            board.en_passant = Some(sq);
            board.zobrist_key ^= zobrist::ep_key(sq);
        }

        // 5 & 6. Halfmove / Fullmove
        if parts.len() >= 5 {
            board.halfmove_clock = parts[4].parse().unwrap_or(0);
        }
        if parts.len() >= 6 {
            board.fullmove_number = parts[5].parse().unwrap_or(1);
        }

        // Full hash calculation is more robust than incrementally modifying an empty board
        board.zobrist_key = board.compute_hash();

        Some(board)
    }

    pub fn fen(&self) -> String {
        let mut fen = String::new();
        let mut empty = 0;

        for rank in (0..8).rev() {
            for file in 0..8 {
                let sq = rank * 8 + file;
                if let Some(piece) = self.piece_on_sq[sq as usize] {
                    if empty > 0 {
                        fen.push_str(&empty.to_string());
                        empty = 0;
                    }
                    let ch = match piece.piece_type() {
                        PieceType::Pawn => 'p',
                        PieceType::Knight => 'n',
                        PieceType::Bishop => 'b',
                        PieceType::Rook => 'r',
                        PieceType::Queen => 'q',
                        PieceType::King => 'k',
                    };
                    if piece.color() == Color::White {
                        fen.push(ch.to_ascii_uppercase());
                    } else {
                        fen.push(ch);
                    }
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                fen.push_str(&empty.to_string());
                empty = 0;
            }
            if rank > 0 {
                fen.push('/');
            }
        }

        fen.push(' ');
        fen.push(if self.side_to_move == Color::White { 'w' } else { 'b' });
        fen.push(' ');

        let mut castling = String::new();
        if self.castling.has_wk() { castling.push('K'); }
        if self.castling.has_wq() { castling.push('Q'); }
        if self.castling.has_bk() { castling.push('k'); }
        if self.castling.has_bq() { castling.push('q'); }
        if castling.is_empty() { castling.push('-'); }
        fen.push_str(&castling);
        fen.push(' ');

        if let Some(ep) = self.en_passant {
            let file = (ep.0 % 8) as u8 + b'a';
            let rank = (ep.0 / 8) as u8 + b'1';
            fen.push(file as char);
            fen.push(rank as char);
        } else {
            fen.push('-');
        }

        fen.push_str(&format!(" {} {}", self.halfmove_clock, self.fullmove_number));

        fen
    }
}
