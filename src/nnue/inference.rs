use super::accumulator::Accumulator;
use super::network::{NetworkParams, L1_SIZE, L2_SIZE, PSQT_BUCKETS};
/// NNUE Forward Pass Inference — HalfKAv2_hm architecture
///
/// Pipeline (Stockfish-aligned):
///   1. Clamp FT(768) accumulators with CReLU and SqrCReLU
///   2. Concatenate [stm_crelu || nstm_crelu || stm_sqr || nstm_sqr] → L1 input
///   3. L1 = fc_0(concat) → SqrCReLU + CReLU (concatenated for fc_1)
///   4. L2 = fc_1(L1_cat)  → CReLU
///   5. output = fc_2(L2) + PSQT_contribution
///
/// All integer arithmetic as in Stockfish.
use crate::types::Color;

/// Count non-pawn pieces for PSQT bucket selection (same as SF)
fn psqt_bucket(piece_count: u32) -> usize {
    ((piece_count.saturating_sub(1)) / 4).min(PSQT_BUCKETS as u32 - 1) as usize
}

/// Apply CReLU to a slice of i16 accumulator values, writing u8 output
#[inline(always)]
fn crelu_slice(input: &[i16], output: &mut [u8]) {
    for (o, &i) in output.iter_mut().zip(input.iter()) {
        *o = i.clamp(0, 127) as u8;
    }
}

/// Apply SqrCReLU: clamp(x,0,127)^2 / 128, written as u8
/// Stockfish: sqr_clipped_relu output = clamp(x,0,127)*clamp(x,0,127) >> 7
#[inline(always)]
fn sqr_crelu_slice(input: &[i16], output: &mut [u8]) {
    for (o, &i) in output.iter_mut().zip(input.iter()) {
        let c = i.clamp(0, 127) as i32;
        *o = ((c * c) >> 7) as u8;
    }
}



/// Full forward pass
/// Returns evaluation in centipawns from perspective of `side_to_move`
pub fn evaluate(side: Color, acc: &Accumulator, params: &NetworkParams, piece_count: u32) -> i32 {
    let ts = super::feature::TRANSFORMED_SIZE; // 768

    let (stm_acc, nstm_acc) = match side {
        Color::White => (&acc.white, &acc.black),
        Color::Black => (&acc.black, &acc.white),
    };

    // Step 1: CReLU + SqrCReLU on both perspectives
    let mut stm_crelu  = [0u8; 768];
    let mut nstm_crelu = [0u8; 768];
    let mut stm_sqr    = [0u8; 768];
    let mut nstm_sqr   = [0u8; 768];

    crelu_slice(&stm_acc.values, &mut stm_crelu);
    crelu_slice(&nstm_acc.values, &mut nstm_crelu);
    sqr_crelu_slice(&stm_acc.values, &mut stm_sqr);
    sqr_crelu_slice(&nstm_acc.values, &mut nstm_sqr);

    // Step 2: Concatenate into L1 input
    // Layout: [stm_crelu | nstm_crelu | stm_sqr | nstm_sqr]
    let mut l1_input = [0u8; 768 * 4];
    l1_input[..768].copy_from_slice(&stm_crelu);
    l1_input[768..2*768].copy_from_slice(&nstm_crelu);
    l1_input[2*768..3*768].copy_from_slice(&stm_sqr);
    l1_input[3*768..4*768].copy_from_slice(&nstm_sqr);

    // Step 3: L1 linear layer
    // l1_weight has shape [L1_SIZE][L1_INPUT_SIZE], but L1_INPUT_SIZE = 4*ts = 3072
    // We only use the first L1_INPUT_SIZE elements
    let l1_input_size = params.l1_weight.get(0).map(|w| w.len()).unwrap_or(0);
    let effective_input_len = l1_input.len().min(l1_input_size);
    let mut l1_out = [0i32; L1_SIZE];
    for j in 0..L1_SIZE {
        let mut sum = params.l1_bias[j];
        for k in 0..effective_input_len {
            sum += l1_input[k] as i32 * params.l1_weight[j][k] as i32;
        }
        l1_out[j] = sum;
    }

    // CReLU + SqrCReLU on L1 output, then concatenate → L2 input
    let mut l1_crelu = [0u8; L1_SIZE];
    let mut l1_sqr   = [0u8; L1_SIZE];
    for j in 0..L1_SIZE {
        let val = (l1_out[j] >> 6).clamp(0, 127);
        l1_crelu[j] = val as u8;
        l1_sqr[j]   = ((val * val) >> 7) as u8;
    }
    // Concatenate [l1_crelu | l1_sqr] → size L1_SIZE*2
    let mut l2_input = [0u8; L1_SIZE * 2];
    l2_input[..L1_SIZE].copy_from_slice(&l1_crelu);
    l2_input[L1_SIZE..].copy_from_slice(&l1_sqr);

    // Step 4: L2 linear layer
    let mut l2_out = [0i32; L2_SIZE];
    for j in 0..L2_SIZE {
        let mut sum = params.l2_bias[j];
        for k in 0..(L1_SIZE * 2) {
            sum += l2_input[k] as i32 * params.l2_weight[j][k] as i32;
        }
        l2_out[j] = sum;
    }

    // CReLU on L2
    let mut l2_crelu = [0u8; L2_SIZE];
    for j in 0..L2_SIZE {
        l2_crelu[j] = (l2_out[j] >> 6).clamp(0, 127) as u8;
    }

    // Step 5: Output layer + PSQT
    let mut output = params.out_bias as i64;
    for k in 0..L2_SIZE {
        output += l2_crelu[k] as i64 * params.out_weight[k] as i64;
    }

    // Add PSQT contribution (select bucket based on piece count)
    let bucket = psqt_bucket(piece_count);
    let psqt_stm  = stm_acc.psqt[bucket] as i64;
    let psqt_nstm = nstm_acc.psqt[bucket] as i64;
    let psqt_val = (psqt_stm - psqt_nstm) / 2;

    // Scale: Stockfish uses OutputScale=16, WeightScaleBits=6
    // output is in units of 127*(1<<6). We scale to centipawns.
    // PSQT is already in centipawns * some factor; approximate divide by 128.
    let total = (output + psqt_val) / 128;

    total.clamp(-4000, 4000) as i32
}
