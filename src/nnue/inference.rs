use super::accumulator::Accumulator;
use super::network::{NetworkParams, L1_SIZE, L2_SIZE};
use super::simd;
/// NNUE Forward Pass Inference
///
/// Pipeline:
///   accumulator → clipped ReLU → L1 → clipped ReLU → L2 → clipped ReLU → output
///
/// All integer arithmetic, no floats.
use crate::types::Color;

/// Full forward pass through the network
/// Returns evaluation in centipawns from perspective of `side_to_move`
#[cfg(target_arch = "x86_64")]
pub fn evaluate(side: Color, acc: &Accumulator, params: &NetworkParams) -> i32 {
    let mut input = [0u8; 512];

    let (stm_acc, nstm_acc) = match side {
        Color::White => (&acc.white, &acc.black),
        Color::Black => (&acc.black, &acc.white),
    };

    // Step 1: Apply clipped ReLU to both accumulators and concatenate
    unsafe {
        let in_ptr = input.as_mut_ptr() as *mut [u8; 256];
        simd::clipped_relu_avx2(&stm_acc.values, &mut *in_ptr);
        simd::clipped_relu_avx2(&nstm_acc.values, &mut *in_ptr.add(1));
    }

    // Step 2: L1 = clipped_relu(W1 * input + b1)
    let mut l1_out = [0i32; L1_SIZE];
    unsafe {
        simd::linear_forward_avx2(&input, &params.l1_weight, &params.l1_bias, &mut l1_out);
    }

    // Apply AVX2 scaling clipped ReLU to L1 output
    let mut l1_clipped = [0u8; L1_SIZE];
    unsafe {
        simd::l1_clipped_avx2(&l1_out, &mut l1_clipped);
    }

    // Step 3: L2 = clipped_relu(W2 * l1_out + b2)
    let mut l2_out = [0i32; L2_SIZE];
    unsafe {
        simd::l2_forward_avx2(&l1_clipped, &params.l2_weight, &params.l2_bias, &mut l2_out);
    }

    // Step 4: L2 scaling and Output Forward Pass
    let mut output = params.out_bias;

    for i in 0..L2_SIZE {
        let scaled = l2_out[i] >> 6;
        let l2_c = if scaled <= 0 {
            0
        } else if scaled >= 127 {
            127
        } else {
            scaled as u8
        };
        output += l2_c as i32 * params.out_weight[i] as i32;
    }

    // Scale output to centipawns (divide by 600 to match standard WDL-based NNUE scale)
    (output / 600).clamp(-4000, 4000)
}

#[cfg(not(target_arch = "x86_64"))]
pub fn evaluate(side: Color, acc: &Accumulator, params: &NetworkParams) -> i32 {
    let mut input = [0u8; 512];

    let (stm_acc, nstm_acc) = match side {
        Color::White => (&acc.white, &acc.black),
        Color::Black => (&acc.black, &acc.white),
    };

    // Step 1: Apply clipped ReLU to both accumulators and concatenate
    let (stm_in, nstm_in) = input.split_at_mut(256);
    simd::clipped_relu_scalar(&stm_acc.values, stm_in.try_into().unwrap());
    simd::clipped_relu_scalar(&nstm_acc.values, nstm_in.try_into().unwrap());

    // Step 2: L1 = clipped_relu(W1 * input + b1)
    let mut l1_out = [0i32; L1_SIZE];
    simd::linear_forward_scalar(&input, &params.l1_weight, &params.l1_bias, &mut l1_out);

    // Apply scaling clipped ReLU to L1 output
    let mut l1_clipped = [0u8; L1_SIZE];
    simd::l1_clipped_scalar(&l1_out, &mut l1_clipped);

    // Step 3: L2 = clipped_relu(W2 * l1_out + b2)
    let mut l2_out = [0i32; L2_SIZE];
    simd::l2_forward_scalar(&l1_clipped, &params.l2_weight, &params.l2_bias, &mut l2_out);

    // Step 4: L2 scaling and Output Forward Pass
    let mut output = params.out_bias;

    for i in 0..L2_SIZE {
        let scaled = l2_out[i] >> 6;
        let l2_c = if scaled <= 0 {
            0
        } else if scaled >= 127 {
            127
        } else {
            scaled as u8
        };
        output += l2_c as i32 * params.out_weight[i] as i32;
    }

    // Scale output to centipawns (divide by 600 to match standard WDL-based NNUE scale)
    (output / 600).clamp(-4000, 4000)
}
