/// NNUE Accumulator — maintains partially-evaluated network input per side.
///
/// Two accumulators: one for White's perspective, one for Black's.
/// Each is TRANSFORMED_SIZE (256) i16 values.
/// 
/// Stack-based push/pop for make_move / unmake_move.

use super::feature::TRANSFORMED_SIZE;
use super::network::NetworkParams;

/// Single-perspective accumulator
#[derive(Clone)]
pub struct SideAccumulator {
    pub values: [i16; TRANSFORMED_SIZE],
}

impl SideAccumulator {
    pub const fn new() -> Self {
        SideAccumulator { values: [0; TRANSFORMED_SIZE] }
    }
}

/// Both perspectives
#[derive(Clone)]
pub struct Accumulator {
    pub white: SideAccumulator,
    pub black: SideAccumulator,
}

impl Accumulator {
    pub const fn new() -> Self {
        Accumulator {
            white: SideAccumulator::new(),
            black: SideAccumulator::new(),
        }
    }

    /// Initialize from biases (full refresh)
    #[inline]
    pub fn init_from_bias(&mut self, params: &NetworkParams) {
        self.white.values.copy_from_slice(&params.ft_bias[..TRANSFORMED_SIZE]);
        self.black.values.copy_from_slice(&params.ft_bias[..TRANSFORMED_SIZE]);
    }

    /// Full refresh: reset to bias, then add all active features
    pub fn refresh(&mut self, white_features: &[usize], black_features: &[usize], params: &NetworkParams) {
        self.init_from_bias(params);
        for &fi in white_features {
            let offset = fi * TRANSFORMED_SIZE;
            debug_assert!(offset + TRANSFORMED_SIZE <= params.ft_weight.len(),
                "White feature index {} out of bounds (max {})", fi, params.ft_weight.len() / TRANSFORMED_SIZE);
            for i in 0..TRANSFORMED_SIZE {
                self.white.values[i] += unsafe { *params.ft_weight.get_unchecked(offset + i) } as i16;
            }
        }
        for &fi in black_features {
            let offset = fi * TRANSFORMED_SIZE;
            debug_assert!(offset + TRANSFORMED_SIZE <= params.ft_weight.len(),
                "Black feature index {} out of bounds (max {})", fi, params.ft_weight.len() / TRANSFORMED_SIZE);
            for i in 0..TRANSFORMED_SIZE {
                self.black.values[i] += unsafe { *params.ft_weight.get_unchecked(offset + i) } as i16;
            }
        }
    }

    /// Add a feature to both perspectives
    #[inline(always)]
    pub fn add_feature(&mut self, white_idx: usize, black_idx: usize, params: &NetworkParams) {
        let w_offset = white_idx * TRANSFORMED_SIZE;
        let b_offset = black_idx * TRANSFORMED_SIZE;
        debug_assert!(w_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        debug_assert!(b_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        for i in 0..TRANSFORMED_SIZE {
            unsafe {
                *self.white.values.get_unchecked_mut(i) += *params.ft_weight.get_unchecked(w_offset + i) as i16;
                *self.black.values.get_unchecked_mut(i) += *params.ft_weight.get_unchecked(b_offset + i) as i16;
            }
        }
    }

    /// Remove a feature from both perspectives
    #[inline(always)]
    pub fn remove_feature(&mut self, white_idx: usize, black_idx: usize, params: &NetworkParams) {
        let w_offset = white_idx * TRANSFORMED_SIZE;
        let b_offset = black_idx * TRANSFORMED_SIZE;
        debug_assert!(w_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        debug_assert!(b_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        for i in 0..TRANSFORMED_SIZE {
            unsafe {
                *self.white.values.get_unchecked_mut(i) -= *params.ft_weight.get_unchecked(w_offset + i) as i16;
                *self.black.values.get_unchecked_mut(i) -= *params.ft_weight.get_unchecked(b_offset + i) as i16;
            }
        }
    }
}

/// Stack for O(1) unmake_move restoration
pub struct AccumulatorStack {
    stack: Vec<Accumulator>,
}

impl AccumulatorStack {
    pub fn new() -> Self {
        AccumulatorStack {
            stack: Vec::with_capacity(128),
        }
    }

    #[inline(always)]
    pub fn push(&mut self, acc: &Accumulator) {
        self.stack.push(acc.clone());
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Option<Accumulator> {
        self.stack.pop()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }
}
