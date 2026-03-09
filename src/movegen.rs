use crate::attacks;
use crate::bitboard::Bitboard;
use crate::board::Board;
use crate::types::{Color, Move, PieceType, Square};

pub struct MoveList {
    pub moves: [Move; 256],
    pub count: usize,
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveList {
    pub fn new() -> Self {
        MoveList {
            moves: [Move::new(0, 0, 0); 256],
            count: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        self.moves[self.count] = m;
        self.count += 1;
    }
}

pub fn generate_pseudo_legal_moves(board: &Board, list: &mut MoveList) {
    let side = board.side_to_move;
    let us = board.color_occupancy(side);
    let them = board.color_occupancy(side.flip());
    let empty = !(us | them);

    // PAWNS
    let pawns = board.color_piece_bb(side, PieceType::Pawn);
    if side == Color::White {
        // Single push
        let pushes = (pawns << 8) & empty;
        let mut bb = pushes;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to - 8;
            if to >= 56 {
                // Promotion
                list.push(Move::new(from, to, Move::FLAG_PR_QUEEN));
                list.push(Move::new(from, to, Move::FLAG_PR_ROOK));
                list.push(Move::new(from, to, Move::FLAG_PR_BISHOP));
                list.push(Move::new(from, to, Move::FLAG_PR_KNIGHT));
            } else {
                list.push(Move::new(from, to, Move::FLAG_QUIET));
            }
        }

        // Double push
        let double_pushes = ((pushes & Bitboard::new(crate::bitboard::RANK_3)) << 8) & empty;
        let mut bb = double_pushes;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to - 16;
            list.push(Move::new(from, to, Move::FLAG_DBL_PUSH));
        }

        // Captures
        let _targets = attacks::pawn_attacks(Color::Black, Square::new(0)); // We'll loop individually for accuracy
        let mut pawns_bb = pawns;
        while pawns_bb.is_not_empty() {
            let from = pawns_bb.pop_lsb();
            let pawn_attacks = attacks::pawn_attacks(side, Square::new(from));
            let attacks = pawn_attacks & them;
            let mut att_bb = attacks;
            while att_bb.is_not_empty() {
                let to = att_bb.pop_lsb();
                if to >= 56 {
                    // Promo capture
                    list.push(Move::new(from, to, Move::FLAG_PC_QUEEN));
                    list.push(Move::new(from, to, Move::FLAG_PC_ROOK));
                    list.push(Move::new(from, to, Move::FLAG_PC_BISHOP));
                    list.push(Move::new(from, to, Move::FLAG_PC_KNIGHT));
                } else {
                    list.push(Move::new(from, to, Move::FLAG_CAPTURE));
                }
            }

            // En passant
            if let Some(ep) = board.en_passant {
                if (pawn_attacks & Bitboard::new(1u64 << ep.0)).is_not_empty() {
                    list.push(Move::new(from, ep.0, Move::FLAG_EP));
                }
            }
        }
    } else {
        // Black pawns
        let pushes = (pawns >> 8) & empty;
        let mut bb = pushes;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to + 8;
            if to <= 7 {
                // Promotion
                list.push(Move::new(from, to, Move::FLAG_PR_QUEEN));
                list.push(Move::new(from, to, Move::FLAG_PR_ROOK));
                list.push(Move::new(from, to, Move::FLAG_PR_BISHOP));
                list.push(Move::new(from, to, Move::FLAG_PR_KNIGHT));
            } else {
                list.push(Move::new(from, to, Move::FLAG_QUIET));
            }
        }

        // Double push
        let double_pushes = ((pushes & Bitboard::new(crate::bitboard::RANK_6)) >> 8) & empty;
        let mut bb = double_pushes;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to + 16;
            list.push(Move::new(from, to, Move::FLAG_DBL_PUSH));
        }

        // Captures
        let mut pawns_bb = pawns;
        while pawns_bb.is_not_empty() {
            let from = pawns_bb.pop_lsb();
            let pawn_attacks = attacks::pawn_attacks(side, Square::new(from));
            let mut att_bb = pawn_attacks & them;
            while att_bb.is_not_empty() {
                let to = att_bb.pop_lsb();
                if to <= 7 {
                    // Promo cap
                    list.push(Move::new(from, to, Move::FLAG_PC_QUEEN));
                    list.push(Move::new(from, to, Move::FLAG_PC_ROOK));
                    list.push(Move::new(from, to, Move::FLAG_PC_BISHOP));
                    list.push(Move::new(from, to, Move::FLAG_PC_KNIGHT));
                } else {
                    list.push(Move::new(from, to, Move::FLAG_CAPTURE));
                }
            }

            // En passant
            if let Some(ep) = board.en_passant {
                if (pawn_attacks & Bitboard::new(1u64 << ep.0)).is_not_empty() {
                    list.push(Move::new(from, ep.0, Move::FLAG_EP));
                }
            }
        }
    }

    // KNIGHTS
    let mut knights = board.color_piece_bb(side, PieceType::Knight);
    while knights.is_not_empty() {
        let from = knights.pop_lsb();
        let attacks = attacks::knight_attacks(Square::new(from)) & !us;
        let mut captures = attacks & them;
        let mut quiets = attacks & empty;

        while captures.is_not_empty() {
            let to = captures.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_CAPTURE));
        }
        while quiets.is_not_empty() {
            let to = quiets.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_QUIET));
        }
    }

    // SLIDERS
    let occ = us | them;
    let mut bishops = board.color_piece_bb(side, PieceType::Bishop);
    while bishops.is_not_empty() {
        let from = bishops.pop_lsb();
        let attacks = attacks::bishop_attacks(Square::new(from), occ) & !us;
        let mut captures = attacks & them;
        let mut quiets = attacks & empty;
        while captures.is_not_empty() {
            let to = captures.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_CAPTURE));
        }
        while quiets.is_not_empty() {
            let to = quiets.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_QUIET));
        }
    }

    let mut rooks = board.color_piece_bb(side, PieceType::Rook);
    while rooks.is_not_empty() {
        let from = rooks.pop_lsb();
        let attacks = attacks::rook_attacks(Square::new(from), occ) & !us;
        let mut captures = attacks & them;
        let mut quiets = attacks & empty;
        while captures.is_not_empty() {
            let to = captures.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_CAPTURE));
        }
        while quiets.is_not_empty() {
            let to = quiets.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_QUIET));
        }
    }

    let mut queens = board.color_piece_bb(side, PieceType::Queen);
    while queens.is_not_empty() {
        let from = queens.pop_lsb();
        let attacks = attacks::queen_attacks(Square::new(from), occ) & !us;
        let mut captures = attacks & them;
        let mut quiets = attacks & empty;
        while captures.is_not_empty() {
            let to = captures.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_CAPTURE));
        }
        while quiets.is_not_empty() {
            let to = quiets.pop_lsb();
            list.push(Move::new(from, to, Move::FLAG_QUIET));
        }
    }

    // KING
    let mut kings = board.color_piece_bb(side, PieceType::King);
    let k_sq = if kings.is_not_empty() {
        kings.pop_lsb()
    } else {
        return;
    };
    let attacks = attacks::king_attacks(Square::new(k_sq)) & !us;
    let mut captures = attacks & them;
    let mut quiets = attacks & empty;

    while captures.is_not_empty() {
        let to = captures.pop_lsb();
        list.push(Move::new(k_sq, to, Move::FLAG_CAPTURE));
    }
    while quiets.is_not_empty() {
        let to = quiets.pop_lsb();
        list.push(Move::new(k_sq, to, Move::FLAG_QUIET));
    }

    // CASTLING (simplified, legality checked later)
    if side == Color::White {
        if board.castling.has_wk()
            && (occ.0 & 0x60) == 0 {
                list.push(Move::new(4, 6, Move::FLAG_K_CASTLE));
            }
        if board.castling.has_wq()
            && (occ.0 & 0xE) == 0 {
                list.push(Move::new(4, 2, Move::FLAG_Q_CASTLE));
            }
    } else {
        if board.castling.has_bk()
            && (occ.0 & 0x6000000000000000) == 0 {
                list.push(Move::new(60, 62, Move::FLAG_K_CASTLE));
            }
        if board.castling.has_bq()
            && (occ.0 & 0x0E00000000000000) == 0 {
                list.push(Move::new(60, 58, Move::FLAG_Q_CASTLE));
            }
    }
}

pub fn generate_pseudo_legal_captures(board: &Board, list: &mut MoveList) {
    let side = board.side_to_move;
    let us = board.color_occupancy(side);
    let them = board.color_occupancy(side.flip());

    // PAWNS
    let pawns = board.color_piece_bb(side, PieceType::Pawn);
    if side == Color::White {
        // Promotions (some might be quiets, but usually all are considered tactical)
        let pushes = (pawns << 8) & !(us | them);
        let mut bb = pushes & Bitboard::new(crate::bitboard::RANK_8);
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to - 8;
            list.push(Move::new(from, to, Move::FLAG_PR_QUEEN));
            list.push(Move::new(from, to, Move::FLAG_PR_ROOK));
            list.push(Move::new(from, to, Move::FLAG_PR_BISHOP));
            list.push(Move::new(from, to, Move::FLAG_PR_KNIGHT));
        }

        // Captures
        let mut pawns_bb = pawns;
        while pawns_bb.is_not_empty() {
            let from = pawns_bb.pop_lsb();
            let pawn_attacks = attacks::pawn_attacks(side, Square::new(from));
            let mut att_bb = pawn_attacks & them;
            while att_bb.is_not_empty() {
                let to = att_bb.pop_lsb();
                if to >= 56 {
                    list.push(Move::new(from, to, Move::FLAG_PC_QUEEN));
                    list.push(Move::new(from, to, Move::FLAG_PC_ROOK));
                    list.push(Move::new(from, to, Move::FLAG_PC_BISHOP));
                    list.push(Move::new(from, to, Move::FLAG_PC_KNIGHT));
                } else {
                    list.push(Move::new(from, to, Move::FLAG_CAPTURE));
                }
            }
            if let Some(ep) = board.en_passant {
                if (pawn_attacks & Bitboard::new(1u64 << ep.0)).is_not_empty() {
                    list.push(Move::new(from, ep.0, Move::FLAG_EP));
                }
            }
        }
    } else {
        // Black pawns
        let pushes = (pawns >> 8) & !(us | them);
        let mut bb = pushes & Bitboard::new(crate::bitboard::RANK_1);
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            let from = to + 8;
            list.push(Move::new(from, to, Move::FLAG_PR_QUEEN));
            list.push(Move::new(from, to, Move::FLAG_PR_ROOK));
            list.push(Move::new(from, to, Move::FLAG_PR_BISHOP));
            list.push(Move::new(from, to, Move::FLAG_PR_KNIGHT));
        }

        let mut pawns_bb = pawns;
        while pawns_bb.is_not_empty() {
            let from = pawns_bb.pop_lsb();
            let pawn_attacks = attacks::pawn_attacks(side, Square::new(from));
            let mut att_bb = pawn_attacks & them;
            while att_bb.is_not_empty() {
                let to = att_bb.pop_lsb();
                if to <= 7 {
                    list.push(Move::new(from, to, Move::FLAG_PC_QUEEN));
                    list.push(Move::new(from, to, Move::FLAG_PC_ROOK));
                    list.push(Move::new(from, to, Move::FLAG_PC_BISHOP));
                    list.push(Move::new(from, to, Move::FLAG_PC_KNIGHT));
                } else {
                    list.push(Move::new(from, to, Move::FLAG_CAPTURE));
                }
            }
            if let Some(ep) = board.en_passant {
                if (pawn_attacks & Bitboard::new(1u64 << ep.0)).is_not_empty() {
                    list.push(Move::new(from, ep.0, Move::FLAG_EP));
                }
            }
        }
    }

    // KNIGHTS
    let mut knights = board.color_piece_bb(side, PieceType::Knight);
    while knights.is_not_empty() {
        let from = knights.pop_lsb();
        let mut captures = attacks::knight_attacks(Square::new(from)) & them;
        while captures.is_not_empty() {
            list.push(Move::new(from, captures.pop_lsb(), Move::FLAG_CAPTURE));
        }
    }

    // SLIDERS
    let occ = us | them;
    let mut bishops = board.color_piece_bb(side, PieceType::Bishop);
    while bishops.is_not_empty() {
        let from = bishops.pop_lsb();
        let mut captures = attacks::bishop_attacks(Square::new(from), occ) & them;
        while captures.is_not_empty() {
            list.push(Move::new(from, captures.pop_lsb(), Move::FLAG_CAPTURE));
        }
    }

    let mut rooks = board.color_piece_bb(side, PieceType::Rook);
    while rooks.is_not_empty() {
        let from = rooks.pop_lsb();
        let mut captures = attacks::rook_attacks(Square::new(from), occ) & them;
        while captures.is_not_empty() {
            list.push(Move::new(from, captures.pop_lsb(), Move::FLAG_CAPTURE));
        }
    }

    let mut queens = board.color_piece_bb(side, PieceType::Queen);
    while queens.is_not_empty() {
        let from = queens.pop_lsb();
        let mut captures = attacks::queen_attacks(Square::new(from), occ) & them;
        while captures.is_not_empty() {
            list.push(Move::new(from, captures.pop_lsb(), Move::FLAG_CAPTURE));
        }
    }

    // KING
    let mut kings = board.color_piece_bb(side, PieceType::King);
    if kings.is_not_empty() {
        let k_sq = kings.pop_lsb();
        let mut captures = attacks::king_attacks(Square::new(k_sq)) & them;
        while captures.is_not_empty() {
            list.push(Move::new(k_sq, captures.pop_lsb(), Move::FLAG_CAPTURE));
        }
    }
}
