/// NNUE Accumulator — maintains partially-evaluated network input per side.
///
/// Upgraded to HalfKAv2_hm: 768-dim FT + 8-bucket PSQT accumulation.
use super::feature::TRANSFORMED_SIZE;
use super::network::{NetworkParams, PSQT_BUCKETS};

/// Single-perspective accumulator (768 i16 values)
#[derive(Clone)]
pub struct SideAccumulator {
    pub values: Vec<i16>,
    /// PSQT accumulation: one i32 per bucket
    pub psqt: [i32; PSQT_BUCKETS],
}

impl Default for SideAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl SideAccumulator {
    pub fn new() -> Self {
        SideAccumulator {
            values: vec![0i16; TRANSFORMED_SIZE],
            psqt: [0i32; PSQT_BUCKETS],
        }
    }
}

/// Both perspectives
#[derive(Clone)]
pub struct Accumulator {
    pub white: SideAccumulator,
    pub black: SideAccumulator,
}

impl Default for Accumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Accumulator {
    pub fn new() -> Self {
        Accumulator {
            white: SideAccumulator::new(),
            black: SideAccumulator::new(),
        }
    }

    /// Initialize from biases (full refresh)
    #[inline]
    pub fn init_from_bias(&mut self, params: &NetworkParams) {
        self.white.values.iter_mut().zip(params.ft_bias.iter()).for_each(|(a, &b)| *a = b);
        self.black.values.iter_mut().zip(params.ft_bias.iter()).for_each(|(a, &b)| *a = b);
        self.white.psqt = [0i32; PSQT_BUCKETS];
        self.black.psqt = [0i32; PSQT_BUCKETS];
    }

    /// Full refresh: reset to bias, then add all active features
    pub fn refresh(
        &mut self,
        white_features: &[usize],
        black_features: &[usize],
        params: &NetworkParams,
    ) {
        self.init_from_bias(params);
        for &fi in white_features {
            let offset = fi * TRANSFORMED_SIZE;
            debug_assert!(
                offset + TRANSFORMED_SIZE <= params.ft_weight.len(),
                "White feature index {} out of bounds (max {})",
                fi,
                params.ft_weight.len() / TRANSFORMED_SIZE
            );
            for i in 0..TRANSFORMED_SIZE {
                self.white.values[i] +=
                    unsafe { *params.ft_weight.get_unchecked(offset + i) } as i16;
            }
            // PSQT accumulation
            let psqt_offset = fi * PSQT_BUCKETS;
            for b in 0..PSQT_BUCKETS {
                self.white.psqt[b] += unsafe { *params.psqt_weight.get_unchecked(psqt_offset + b) };
            }
        }
        for &fi in black_features {
            let offset = fi * TRANSFORMED_SIZE;
            debug_assert!(
                offset + TRANSFORMED_SIZE <= params.ft_weight.len(),
                "Black feature index {} out of bounds (max {})",
                fi,
                params.ft_weight.len() / TRANSFORMED_SIZE
            );
            for i in 0..TRANSFORMED_SIZE {
                self.black.values[i] +=
                    unsafe { *params.ft_weight.get_unchecked(offset + i) } as i16;
            }
            let psqt_offset = fi * PSQT_BUCKETS;
            for b in 0..PSQT_BUCKETS {
                self.black.psqt[b] += unsafe { *params.psqt_weight.get_unchecked(psqt_offset + b) };
            }
        }
    }

    /// Add a feature — separate white and black indices (for incremental updates)
    #[inline(always)]
    pub fn add_feature(&mut self, white_idx: usize, black_idx: usize, params: &NetworkParams) {
        let w_offset = white_idx * TRANSFORMED_SIZE;
        let b_offset = black_idx * TRANSFORMED_SIZE;
        debug_assert!(w_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        debug_assert!(b_offset + TRANSFORMED_SIZE <= params.ft_weight.len());
        for i in 0..TRANSFORMED_SIZE {
            unsafe {
                *self.white.values.get_unchecked_mut(i) +=
                    *params.ft_weight.get_unchecked(w_offset + i) as i16;
                *self.black.values.get_unchecked_mut(i) +=
                    *params.ft_weight.get_unchecked(b_offset + i) as i16;
            }
        }
        // PSQT updates
        let w_psqt = white_idx * PSQT_BUCKETS;
        let b_psqt = black_idx * PSQT_BUCKETS;
        for b in 0..PSQT_BUCKETS {
            unsafe {
                self.white.psqt[b] += *params.psqt_weight.get_unchecked(w_psqt + b);
                self.black.psqt[b] += *params.psqt_weight.get_unchecked(b_psqt + b);
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
                *self.white.values.get_unchecked_mut(i) -=
                    *params.ft_weight.get_unchecked(w_offset + i) as i16;
                *self.black.values.get_unchecked_mut(i) -=
                    *params.ft_weight.get_unchecked(b_offset + i) as i16;
            }
        }
        let w_psqt = white_idx * PSQT_BUCKETS;
        let b_psqt = black_idx * PSQT_BUCKETS;
        for b in 0..PSQT_BUCKETS {
            unsafe {
                self.white.psqt[b] -= *params.psqt_weight.get_unchecked(w_psqt + b);
                self.black.psqt[b] -= *params.psqt_weight.get_unchecked(b_psqt + b);
            }
        }
    }
}

/// Stack for O(1) unmake_move restoration
pub struct AccumulatorStack {
    stack: Vec<Accumulator>,
}

impl Default for AccumulatorStack {
    fn default() -> Self {
        Self::new()
    }
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
