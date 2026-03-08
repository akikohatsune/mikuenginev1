/// Incremental NNUE update API
/// 
/// Provides helper functions for integrating accumulator updates
/// into make_move / unmake_move.
///
/// Each move produces a set of (add, remove) feature changes.
/// We apply these deltas incrementally instead of recomputing.

use crate::types::{Color, PieceType, Square};
use super::accumulator::Accumulator;
use super::network::NetworkParams;
use super::feature::feature_index;

/// Describes a single feature delta
pub enum FeatureDelta {
    Add(usize, usize),    // (white_idx, black_idx)
    Remove(usize, usize), // (white_idx, black_idx)
}

/// Apply a batch of feature deltas to the accumulator
pub fn apply_deltas(acc: &mut Accumulator, deltas: &[FeatureDelta], params: &NetworkParams) {
    for delta in deltas {
        match delta {
            FeatureDelta::Add(w, b) => acc.add_feature(*w, *b, params),
            FeatureDelta::Remove(w, b) => acc.remove_feature(*w, *b, params),
        }
    }
}

/// Compute feature indices for a quiet move (non-capture, non-special)
pub fn quiet_move_deltas(
    wk: Square, bk: Square,
    from: Square, to: Square,
    pt: PieceType, color: Color,
) -> [FeatureDelta; 2] {
    let rm_w = feature_index(wk, from, pt, color);
    let rm_b = feature_index(bk, from, pt, color);
    let add_w = feature_index(wk, to, pt, color);
    let add_b = feature_index(bk, to, pt, color);
    [FeatureDelta::Remove(rm_w, rm_b), FeatureDelta::Add(add_w, add_b)]
}

/// Compute feature indices for a capture move
pub fn capture_deltas(
    wk: Square, bk: Square,
    from: Square, to: Square,
    pt: PieceType, color: Color,
    victim_pt: PieceType, victim_color: Color,
) -> [FeatureDelta; 3] {
    let rm_self_w = feature_index(wk, from, pt, color);
    let rm_self_b = feature_index(bk, from, pt, color);
    let rm_victim_w = feature_index(wk, to, victim_pt, victim_color);
    let rm_victim_b = feature_index(bk, to, victim_pt, victim_color);
    let add_w = feature_index(wk, to, pt, color);
    let add_b = feature_index(bk, to, pt, color);
    [
        FeatureDelta::Remove(rm_self_w, rm_self_b),
        FeatureDelta::Remove(rm_victim_w, rm_victim_b),
        FeatureDelta::Add(add_w, add_b),
    ]
}

/// Compute deltas for en passant capture
pub fn ep_capture_deltas(
    wk: Square, bk: Square,
    from: Square, to: Square,
    color: Color, ep_cap_sq: Square,
) -> [FeatureDelta; 3] {
    let rm_self_w = feature_index(wk, from, PieceType::Pawn, color);
    let rm_self_b = feature_index(bk, from, PieceType::Pawn, color);
    let rm_ep_w = feature_index(wk, ep_cap_sq, PieceType::Pawn, color.flip());
    let rm_ep_b = feature_index(bk, ep_cap_sq, PieceType::Pawn, color.flip());
    let add_w = feature_index(wk, to, PieceType::Pawn, color);
    let add_b = feature_index(bk, to, PieceType::Pawn, color);
    [
        FeatureDelta::Remove(rm_self_w, rm_self_b),
        FeatureDelta::Remove(rm_ep_w, rm_ep_b),
        FeatureDelta::Add(add_w, add_b),
    ]
}

/// Compute deltas for castling (king + rook move)
pub fn castling_deltas(
    wk: Square, bk: Square,
    _king_from: Square, _king_to: Square,
    rook_from: Square, rook_to: Square,
    color: Color,
) -> [FeatureDelta; 2] {
    // King is handled via full refresh, so only need rook deltas
    let rm_w = feature_index(wk, rook_from, PieceType::Rook, color);
    let rm_b = feature_index(bk, rook_from, PieceType::Rook, color);
    let add_w = feature_index(wk, rook_to, PieceType::Rook, color);
    let add_b = feature_index(bk, rook_to, PieceType::Rook, color);
    [FeatureDelta::Remove(rm_w, rm_b), FeatureDelta::Add(add_w, add_b)]
}

/// Compute deltas for pawn promotion
pub fn promotion_deltas(
    wk: Square, bk: Square,
    to: Square,
    color: Color,
    promo_pt: PieceType,
) -> [FeatureDelta; 2] {
    // Remove pawn, add promoted piece (both at 'to' square)
    let rm_w = feature_index(wk, to, PieceType::Pawn, color);
    let rm_b = feature_index(bk, to, PieceType::Pawn, color);
    let add_w = feature_index(wk, to, promo_pt, color);
    let add_b = feature_index(bk, to, promo_pt, color);
    [FeatureDelta::Remove(rm_w, rm_b), FeatureDelta::Add(add_w, add_b)]
}
