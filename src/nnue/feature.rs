/// HalfKP feature extraction for NNUE
/// 
/// Feature index = king_sq * NUM_FEATURES_PER_KING + piece_index * 64 + piece_sq
/// 
/// piece_index encodes (piece_color, piece_type) excluding kings:
///   White Pawn=0, White Knight=1, White Bishop=2, White Rook=3, White Queen=4
///   Black Pawn=5, Black Knight=6, Black Bishop=7, Black Rook=8, Black Queen=9
///
/// Total features per perspective = 64 * 10 * 64 = 40960

use crate::types::{Color, PieceType, Square};

pub const NUM_FEATURES_PER_KING: usize = 10 * 64; // 640
pub const HALFKP_FEATURES: usize = 64 * NUM_FEATURES_PER_KING; // 40960
pub const TRANSFORMED_SIZE: usize = 256;

/// Map (color, piece_type) to piece_index [0..9], excluding King
#[inline(always)]
pub fn piece_index(color: Color, pt: PieceType) -> usize {
    let base = match pt {
        PieceType::Pawn   => 0,
        PieceType::Knight => 1,
        PieceType::Bishop => 2,
        PieceType::Rook   => 3,
        PieceType::Queen  => 4,
        _ => 0, // King excluded from features
    };
    if color == Color::Black { base + 5 } else { base }
}

/// Compute HalfKP feature index from a given perspective king square.
/// `king_sq`: the square of the perspective's own king
/// `piece_sq`: the square of the piece
/// `pt`: the piece type (must not be King)
/// `piece_color`: the color of the piece
#[inline(always)]
pub fn feature_index(king_sq: Square, piece_sq: Square, pt: PieceType, piece_color: Color) -> usize {
    let ki = king_sq.0 as usize;
    let pi = piece_index(piece_color, pt);
    let sq = piece_sq.0 as usize;
    ki * NUM_FEATURES_PER_KING + pi * 64 + sq
}

/// Orient a square for black's perspective (flip vertically)
#[inline(always)]
pub fn orient_square(sq: Square, perspective: Color) -> Square {
    if perspective == Color::Black {
        Square::new(sq.0 ^ 56) // Flip rank
    } else {
        sq
    }
}

/// Compute oriented feature index (flips for black perspective)
#[inline(always)]
pub fn oriented_feature_index(king_sq: Square, piece_sq: Square, pt: PieceType, piece_color: Color, perspective: Color) -> usize {
    let oriented_king = orient_square(king_sq, perspective);
    let oriented_piece = orient_square(piece_sq, perspective);
    let oriented_color = if perspective == Color::Black { piece_color.flip() } else { piece_color };
    feature_index(oriented_king, oriented_piece, pt, oriented_color)
}
