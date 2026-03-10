use crate::board::Board;
use crate::types::{Color, PieceType, Square};

pub const PAWN_VALUE: i32 = 100;
pub const KNIGHT_VALUE: i32 = 300;
pub const BISHOP_VALUE: i32 = 320;
pub const ROOK_VALUE: i32 = 500;
pub const QUEEN_VALUE: i32 = 900;
pub const KING_VALUE: i32 = 20000;

pub const PIECE_VALUES: [i32; 6] = [
    PAWN_VALUE,
    KNIGHT_VALUE,
    BISHOP_VALUE,
    ROOK_VALUE,
    QUEEN_VALUE,
    KING_VALUE,
];

// Simplified Piece-Square Tables (based on PeSTO's evaluation)
const PST_PAWN: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 50, 50, 50, 50, 50, 50, 50, 50, 10, 10, 20, 30, 30, 20, 10, 10, 5, 5,
    10, 25, 25, 10, 5, 5, 0, 0, 0, 20, 20, 0, 0, 0, 5, -5, -10, 0, 0, -10, -5, 5, 5, 10, 10, -20,
    -20, 10, 10, 5, 0, 0, 0, 0, 0, 0, 0, 0,
];

const PST_KNIGHT: [i32; 64] = [
    -50, -40, -30, -30, -30, -30, -40, -50, -40, -20, 0, 0, 0, 0, -20, -40, -30, 0, 10, 15, 15, 10,
    0, -30, -30, 5, 15, 20, 20, 15, 5, -30, -30, 0, 15, 20, 20, 15, 0, -30, -30, 5, 10, 15, 15, 10,
    5, -30, -40, -20, 0, 5, 5, 0, -20, -40, -50, -40, -30, -30, -30, -30, -40, -50,
];

const PST_BISHOP: [i32; 64] = [
    -20, -10, -10, -10, -10, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 10, 10, 5, 0,
    -10, -10, 5, 5, 10, 10, 5, 5, -10, -10, 0, 10, 10, 10, 10, 0, -10, -10, 10, 10, 10, 10, 10, 10,
    -10, -10, 5, 0, 0, 0, 0, 5, -10, -20, -10, -10, -10, -10, -10, -10, -20,
];

const PST_ROOK: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 5, 10, 10, 10, 10, 10, 10, 5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0,
    0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, -5, 0, 0, 0, 0, 0, 0, -5, 0, 0,
    0, 5, 5, 0, 0, 0,
];

const PST_QUEEN: [i32; 64] = [
    -20, -10, -10, -5, -5, -10, -10, -20, -10, 0, 0, 0, 0, 0, 0, -10, -10, 0, 5, 5, 5, 5, 0, -10,
    -5, 0, 5, 5, 5, 5, 0, -5, 0, 0, 5, 5, 5, 5, 0, -5, -10, 5, 5, 5, 5, 5, 0, -10, -10, 0, 5, 0, 0,
    0, 0, -10, -20, -10, -10, -5, -5, -10, -10, -20,
];

const PST_KING_MG: [i32; 64] = [
    -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -30, -40, -40,
    -50, -50, -40, -40, -30, -30, -40, -40, -50, -50, -40, -40, -30, -20, -30, -30, -40, -40, -30,
    -30, -20, -10, -20, -20, -20, -20, -20, -20, -10, 20, 20, 0, 0, 0, 0, 20, 20, 20, 30, 10, 0, 0,
    10, 30, 20,
];

#[inline(always)]
fn get_pst_score(pt: PieceType, sq: Square, color: Color) -> i32 {
    let mut s = sq.0 as usize;
    if color == Color::Black {
        s ^= 56; // Flip rank
    }
    match pt {
        PieceType::Pawn => PST_PAWN[s],
        PieceType::Knight => PST_KNIGHT[s],
        PieceType::Bishop => PST_BISHOP[s],
        PieceType::Rook => PST_ROOK[s],
        PieceType::Queen => PST_QUEEN[s],
        PieceType::King => PST_KING_MG[s],
    }
}

pub fn evaluate(board: &Board) -> i32 {
    let mut score = 0;

    for pt in 0..6 {
        let piece_type = match pt {
            0 => PieceType::Pawn,
            1 => PieceType::Knight,
            2 => PieceType::Bishop,
            3 => PieceType::Rook,
            4 => PieceType::Queen,
            5 => PieceType::King,
            _ => unreachable!(),
        };

        // White pieces
        let mut white_bb = board.color_piece_bb(Color::White, piece_type);
        while white_bb.is_not_empty() {
            let sq = Square::new(white_bb.pop_lsb());
            score += PIECE_VALUES[pt];
            score += get_pst_score(piece_type, sq, Color::White);
        }

        // Black pieces
        let mut black_bb = board.color_piece_bb(Color::Black, piece_type);
        while black_bb.is_not_empty() {
            let sq = Square::new(black_bb.pop_lsb());
            score -= PIECE_VALUES[pt];
            score -= get_pst_score(piece_type, sq, Color::Black);
        }
    }

    if board.side_to_move == Color::Black {
        -score
    } else {
        score
    }
}

// Statically scale or override NNUE evaluation for known endgames
pub fn endgame_evaluate(board: &Board, raw_eval: i32) -> i32 {
    let w_non_pawn = board.non_pawn_material(Color::White);
    let b_non_pawn = board.non_pawn_material(Color::Black);
    let side = board.side_to_move;
    
    // Pawn endgames (King and Pawns only)
    if w_non_pawn == 0 && b_non_pawn == 0 {
        let mut our_pawns = board.color_piece_bb(side, PieceType::Pawn);
        let enemy_king = Square::new((board.color_piece_bb(side.flip(), PieceType::King)).lsb());
        
        let mut passed_bonus = 0;
        while our_pawns.is_not_empty() {
            let sq = Square::new(our_pawns.lsb());
            our_pawns.clear_bit(sq.0);
            
            let prom_y = if side == Color::White { 7 } else { 0 };
            let dist_to_prom = (sq.rank() as i32 - prom_y as i32).abs();
            let king_dist = (enemy_king.rank() as i32 - prom_y as i32).abs().max((enemy_king.file() as i32 - sq.file() as i32).abs());
            
            let pawn_dist = if (side == Color::White && sq.rank() == 1) || (side == Color::Black && sq.rank() == 6) {
                dist_to_prom - 2
            } else {
                dist_to_prom.max(1) - 1
            };
            
            if pawn_dist < king_dist {
                passed_bonus += 800;
            }
        }
        
        let new_eval = raw_eval + passed_bonus;
        if passed_bonus > 0 {
            return new_eval + 200;
        } else if new_eval > 0 {
            return new_eval + 100;
        }
    }

    // Opposite Colored Bishops (OCB) endgame
    // If each side has exactly 1 bishop, no other pieces (except pawns/kings), and they are opposite colors
    if w_non_pawn == BISHOP_VALUE && b_non_pawn == BISHOP_VALUE {
        let w_b_sq = Square::new(board.color_piece_bb(Color::White, PieceType::Bishop).lsb());
        let b_b_sq = Square::new(board.color_piece_bb(Color::Black, PieceType::Bishop).lsb());
        
        let w_color = (w_b_sq.rank() + w_b_sq.file()) % 2;
        let b_color = (b_b_sq.rank() + b_b_sq.file()) % 2;
        
        if w_color != b_color {
            // Opposite colored bishops are very drawish. Scale down eval.
            return raw_eval / 2;
        }
    }
    
    // Default EPS: if one side has no pawns and is down material, scale to win/draw faster
    let w_pawns = board.color_piece_bb(Color::White, PieceType::Pawn).count();
    let b_pawns = board.color_piece_bb(Color::Black, PieceType::Pawn).count();
    
    if w_pawns == 0 && b_pawns == 0 {
        // No pawns left. Is there enough material to mate?
        // KRvK, KBBvK, KBNvK are mate. KBvK, KNvK, KNNvK are draws.
        let w_mat = w_non_pawn;
        let b_mat = b_non_pawn;
        
        if w_mat < KNIGHT_VALUE + 100 && b_mat < KNIGHT_VALUE + 100 {
            // Insufficient material
            return 0; // Return exactly draw score regardless of NNUE
        }
    } else if w_pawns == 0 && raw_eval > 50 {
        // White has no pawns but is winning? Hard to win without pawns unless huge mat advantage
        if w_non_pawn < ROOK_VALUE {
            return raw_eval / 2;
        }
    } else if b_pawns == 0 && raw_eval < -50 {
        // Black has no pawns but is winning?
        if b_non_pawn < ROOK_VALUE {
            return raw_eval / 2;
        }
    }

    raw_eval
}

// Stockfish WDL Logistic Curve Model
pub fn win_rate_model(v: i32, board: &Board) -> i32 {
    let pawns = board.color_piece_bb(Color::White, PieceType::Pawn).count()
              + board.color_piece_bb(Color::Black, PieceType::Pawn).count();
    let knights = board.color_piece_bb(Color::White, PieceType::Knight).count()
                + board.color_piece_bb(Color::Black, PieceType::Knight).count();
    let bishops = board.color_piece_bb(Color::White, PieceType::Bishop).count()
                + board.color_piece_bb(Color::Black, PieceType::Bishop).count();
    let rooks = board.color_piece_bb(Color::White, PieceType::Rook).count()
              + board.color_piece_bb(Color::Black, PieceType::Rook).count();
    let queens = board.color_piece_bb(Color::White, PieceType::Queen).count()
               + board.color_piece_bb(Color::Black, PieceType::Queen).count();
               
    let material = pawns + 3 * knights + 3 * bishops + 5 * rooks + 9 * queens;
    // The fitted model only uses data for material counts in [17, 78], anchored at count 58.
    let m = (material as f64).clamp(17.0, 78.0) / 58.0;
    
    let a_s = [-72.32565836, 185.93832038, -144.58862193, 416.44950446];
    let b_s = [83.86794042, -136.06112997, 69.98820887, 47.62901433];
    
    let a = (((a_s[0] * m + a_s[1]) * m + a_s[2]) * m) + a_s[3];
    let b = (((b_s[0] * m + b_s[1]) * m + b_s[2]) * m) + b_s[3];
    
    (0.5 + 1000.0 / (1.0 + ((a - v as f64) / b).exp())) as i32
}

