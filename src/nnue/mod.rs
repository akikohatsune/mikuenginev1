pub mod accumulator;
/// NNUE Evaluation Module — HalfKAv2_hm Architecture
///
/// Sub-modules:
///   feature     — HalfKAv2_hm feature extraction with 32 king buckets
///   accumulator — dual-perspective accumulator with PSQT head
///   loader      — .nnue binary file parser (network module)
///   inference   — forward pass with CReLU + SqrCReLU + PSQT
///   incremental — delta-based incremental update helpers
///   simd        — AVX2 acceleration for linear layers
pub mod feature;
pub mod incremental;
pub mod inference;
pub mod simd;

/// Network parameters module (loader.rs contains NetworkParams)
pub mod network {
    pub use super::loader::*;
}
mod loader;

use crate::types::Color;
/// Top-level NNUE engine wrapper
pub struct NNUE {
    pub params: network::NetworkParams,
}

impl NNUE {
    /// Create with default (zero) weights — for testing without a .nnue file
    pub fn new() -> Self {
        NNUE {
            params: network::NetworkParams::new(),
        }
    }

    /// Load from a .nnue file
    pub fn load(path: &str) -> Result<Self, std::io::Error> {
        let params = network::NetworkParams::load(path)?;
        Ok(NNUE { params })
    }

    /// Evaluate the current position from the perspective of `side`
    pub fn evaluate(&self, side: Color, acc: &accumulator::Accumulator) -> i32 {
        inference::evaluate(side, acc, &self.params)
    }
}

// Re-exports for backward compatibility with existing board.rs code
pub use accumulator::Accumulator;
pub use feature::feature_index;
pub use feature::feature_index_for_perspective;
