/// HalfKAv2_hm feature extraction for NNUE
///
/// Feature set used by Stockfish NNUE (sf17+):
/// - 32 king buckets (horizontally mirrored: king always on e-h files)
/// - 11 piece types per bucket (including king as piece for opponent)
/// - 64 squares per piece type
///
/// Feature count: 32 buckets × 11 piece_types × 64 squares = 22,528
/// But indexed as: KingBucket[ksq] * (11*64) + PieceIndex[perspective][piece] + sq
///
/// Total: HALF_KA_V2_HM_FEATURES = 32 × 11 × 64 = 22,528 per perspective
/// Network FT array stores: 45,056 × TRANSFORMED_SIZE weights (but accessed
/// via bucket lookup).
use crate::types::{Color, PieceType, Square};

/// King bucket count (after horizontal mirror: king on e-h file = 32 possible squares)
pub const KING_BUCKETS: usize = 32;

/// Number of piece types (10 non-king + 1 king-as-piece for opponent in HalfKAv2)
pub const NUM_PIECE_TYPES_IN_FEATURES: usize = 11;

/// Features per king bucket: 11 × 64
pub const FEATURES_PER_BUCKET: usize = NUM_PIECE_TYPES_IN_FEATURES * 64;

/// Total input feature count for the feature transformer
pub const HALFKA_FEATURES: usize = KING_BUCKETS * FEATURES_PER_BUCKET; // 22,528

/// Keep this alias for compatibility with loader.rs
pub const HALFKP_FEATURES: usize = HALFKA_FEATURES;

/// Feature transformer output (accumulator) size — upgraded from 256 to 768
pub const TRANSFORMED_SIZE: usize = 768;

/// King bucket table: maps board square → bucket index [0..31]
/// Mirrors horizontally: king on e-h file stays, king on a-d file is mirrored to e-h.
/// Layout is: rank 0..7 from bottom to top, file 0..7 left to right.
/// Same table as Stockfish's KingBuckets with the B() macro expanded.
/// KingBuckets[sq] = bucket_index (32 buckets, 2 per rank per mirror half)
pub const KING_BUCKET_TABLE: [usize; 64] = [
    // rank 1 (sq 0-7)  white bottom
    0, 1, 2, 3, 3, 2, 1, 0,
    // rank 2 (sq 8-15)
    4, 5, 6, 7, 7, 6, 5, 4,
    // rank 3
    8, 9, 10, 11, 11, 10, 9, 8,
    // rank 4
    12, 13, 14, 15, 15, 14, 13, 12,
    // rank 5
    16, 17, 18, 19, 19, 18, 17, 16,
    // rank 6
    20, 21, 22, 23, 23, 22, 21, 20,
    // rank 7
    24, 25, 26, 27, 27, 26, 25, 24,
    // rank 8 (sq 56-63)
    28, 29, 30, 31, 31, 30, 29, 28,
];

/// PieceSquareIndex table — maps (perspective, piece) → base index within bucket
/// Encoding consistent with Stockfish HalfKAv2_hm:
///   PS_W_PAWN=0, PS_B_PAWN=64, PS_W_KNIGHT=128, PS_B_KNIGHT=192, ...
///   PS_W_QUEEN=512, PS_B_QUEEN=576, PS_KING=640
///
/// Indexing: [perspective_is_black][piece_color_is_black][piece_type]
/// Returns the base offset (0..11*64), the caller adds `piece_sq`.
#[inline(always)]
pub fn piece_sq_index(perspective: Color, piece_color: Color, pt: PieceType) -> usize {
    // From perspective view: "us"/"them" swap gives the friendly/opponent indexing.
    // Stockfish uses: if side==white → us=white, them=black else flipped.
    let friendly = perspective == piece_color;

    // Compact encoding matching Stockfish's HalfKAv2_hm PieceSquareIndex table:
    // W_PAWN=0, B_PAWN=1*64, W_KNIGHT=2*64, B_KNIGHT=3*64,
    // W_BISHOP=4*64, B_BISHOP=5*64, W_ROOK=6*64, B_ROOK=7*64,
    // W_QUEEN=8*64, B_QUEEN=9*64, KING=10*64
    let base = match (pt, friendly, perspective) {
        (PieceType::Pawn,   true,  Color::White) => 0 * 64,   // W_PAWN
        (PieceType::Pawn,   false, Color::White) => 1 * 64,   // B_PAWN (their pawn)
        (PieceType::Pawn,   true,  Color::Black) => 1 * 64,   // B_PAWN (us=black pawn, their=white)
        (PieceType::Pawn,   false, Color::Black) => 0 * 64,   // W_PAWN (their pawn)
        (PieceType::Knight, true,  Color::White) => 2 * 64,
        (PieceType::Knight, false, Color::White) => 3 * 64,
        (PieceType::Knight, true,  Color::Black) => 3 * 64,
        (PieceType::Knight, false, Color::Black) => 2 * 64,
        (PieceType::Bishop, true,  Color::White) => 4 * 64,
        (PieceType::Bishop, false, Color::White) => 5 * 64,
        (PieceType::Bishop, true,  Color::Black) => 5 * 64,
        (PieceType::Bishop, false, Color::Black) => 4 * 64,
        (PieceType::Rook,   true,  Color::White) => 6 * 64,
        (PieceType::Rook,   false, Color::White) => 7 * 64,
        (PieceType::Rook,   true,  Color::Black) => 7 * 64,
        (PieceType::Rook,   false, Color::Black) => 6 * 64,
        (PieceType::Queen,  true,  Color::White) => 8 * 64,
        (PieceType::Queen,  false, Color::White) => 9 * 64,
        (PieceType::Queen,  true,  Color::Black) => 9 * 64,
        (PieceType::Queen,  false, Color::Black) => 8 * 64,
        (PieceType::King,   _,     _           ) => 10 * 64,  // king always maps here
    };
    base
}

/// Orient a square for a given perspective.
/// For White: sq stays the same.
/// For Black: flip rank (XOR with 56) AND mirror file (XOR with 7) = full 180° rotation.
#[inline(always)]
pub fn orient_sq(sq: Square, perspective: Color) -> usize {
    if perspective == Color::White {
        sq.0 as usize
    } else {
        (sq.0 ^ 63) as usize // 180-degree rotation (flip rank AND file)
    }
}

/// Orient the king square for bucket lookup.
/// If king is on a-d files (file < 4), mirror to e-h by XORing file with 7.
#[inline(always)]
pub fn orient_king_sq(ksq: Square) -> usize {
    let sq = ksq.0 as usize;
    let file = sq & 7;
    if file < 4 {
        sq ^ 7 // mirror file
    } else {
        sq
    }
}

/// Compute the HalfKAv2_hm feature index for a given perspective.
///
/// - `perspective`: which side's accumulator we're updating
/// - `ksq`: king square for this perspective (used for bucket lookup)
/// - `piece_sq`: square of the piece being added/removed
/// - `pt`: piece type
/// - `piece_color`: color of the piece
#[inline(always)]
pub fn feature_index(
    ksq: Square,       // king sq for this perspective (already oriented)
    piece_sq: Square,
    pt: PieceType,
    piece_color: Color,
) -> usize {
    // This is called from board.rs which passes ksq and piece_sq from white's perspective.
    // We compute both white and black oriented indices here separately.
    // For compatibility with existing board.rs, we output a white-perspective index.
    // Board.rs calls refresh_accumulator which calls feature_index_for_perspective.
    let oriented_ksq = orient_king_sq(ksq);
    let bucket = KING_BUCKET_TABLE[oriented_ksq];
    let psi = piece_sq_index(Color::White, piece_color, pt);
    let sq = piece_sq.0 as usize;
    bucket * FEATURES_PER_BUCKET + psi + sq
}

/// Compute feature index for a specific perspective (used in refresh)
#[inline(always)]
pub fn feature_index_for_perspective(
    perspective: Color,
    ksq: Square,
    piece_sq: Square,
    pt: PieceType,
    piece_color: Color,
) -> usize {
    // Orient king square for bucket lookup
    let oriented_ksq = if perspective == Color::White {
        orient_king_sq(ksq)
    } else {
        // For black: flip the king square 180° first, then apply king mirror
        orient_king_sq(Square::new(ksq.0 ^ 63))
    };
    let bucket = KING_BUCKET_TABLE[oriented_ksq];

    // Orient piece square for piece lookup
    let oriented_piece_sq = orient_sq(piece_sq, perspective);

    let psi = piece_sq_index(perspective, piece_color, pt);
    bucket * FEATURES_PER_BUCKET + psi + oriented_piece_sq
}
