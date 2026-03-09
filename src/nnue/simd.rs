/// SIMD-accelerated operations for NNUE inference
///
/// Uses AVX2 when available (256-bit SIMD = 32 bytes).
/// Accelerates the L1 layer dot product (512 i8 × u8 multiplies per neuron).
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// AVX2-accelerated linear forward for L1 layer
/// Computes: output[j] = bias[j] + sum(input[k] * weight[j][k]) for k in 0..512
///
/// Uses _mm256_maddubs_epi16 for u8 * i8 → i16 multiply-add, then horizontal sum.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn linear_forward_avx2(
    input: &[u8; 512],
    weights: &[[i8; 512]],
    biases: &[i32],
    output: &mut [i32],
) {
    let num_outputs = output.len();

    for j in 0..num_outputs {
        let mut acc = _mm256_setzero_si256();
        let w_ptr = weights[j].as_ptr();
        let i_ptr = input.as_ptr();

        // Process 32 elements per iteration (256 bits / 8 bits = 32)
        let mut k = 0;
        while k + 32 <= 512 {
            // Load 32 unsigned bytes from input
            let inp = _mm256_loadu_si256(i_ptr.add(k) as *const __m256i);
            // Load 32 signed bytes from weights
            let wgt = _mm256_loadu_si256(w_ptr.add(k) as *const __m256i);

            // Multiply u8 * i8 → i16 pairs and horizontal add adjacent pairs
            // _mm256_maddubs_epi16: treats first operand as unsigned, second as signed
            let product = _mm256_maddubs_epi16(inp, wgt);

            // Widen i16 → i32 by adding pairs
            let ones = _mm256_set1_epi16(1);
            let widened = _mm256_madd_epi16(product, ones);

            acc = _mm256_add_epi32(acc, widened);
            k += 32;
        }

        // Horizontal sum of 8 i32 lanes
        // acc = [a0, a1, a2, a3, a4, a5, a6, a7]
        let hi128 = _mm256_extracti128_si256(acc, 1);
        let lo128 = _mm256_castsi256_si128(acc);
        let sum128 = _mm_add_epi32(lo128, hi128);
        // sum128 = [s0, s1, s2, s3]
        let hi64 = _mm_unpackhi_epi64(sum128, sum128);
        let sum64 = _mm_add_epi32(sum128, hi64);
        // sum64 = [s0+s2, s1+s3, ...]
        let hi32 = _mm_shuffle_epi32(sum64, 1);
        let sum32 = _mm_add_epi32(sum64, hi32);

        let mut total = _mm_cvtsi128_si32(sum32);

        // Handle remaining elements (512 % 32 == 0, so normally none)
        while k < 512 {
            total += *i_ptr.add(k) as i32 * *w_ptr.add(k) as i32;
            k += 1;
        }

        output[j] = biases[j] + total;
    }
}

/// AVX2-accelerated Accumulator ReLU (i16 -> u8 clipped to [0, 127])
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn clipped_relu_avx2(input: &[i16; 256], output: &mut [u8; 256]) {
    let zeros = _mm256_setzero_si256();
    let max_vals = _mm256_set1_epi16(127);

    // Process 32 `i16` values at a time (64 bytes) -> outputs 32 `u8` values
    let mut i = 0;
    while i + 32 <= 256 {
        // Load first 16 i16s
        let v0 = _mm256_loadu_si256(input.as_ptr().add(i) as *const __m256i);
        // Load next 16 i16s
        let v1 = _mm256_loadu_si256(input.as_ptr().add(i + 16) as *const __m256i);

        // Clamp to [0, 127]
        let c0 = _mm256_min_epi16(_mm256_max_epi16(v0, zeros), max_vals);
        let c1 = _mm256_min_epi16(_mm256_max_epi16(v1, zeros), max_vals);

        // Pack i16 -> u8
        // AVX2 packing works within 128-bit lanes.
        // c0 = [A B] (each 128-bit). c1 = [C D]. Result = [pack(A,C) pack(B,D)]
        let packed = _mm256_packus_epi16(c0, c1);

        // Fix the cross-lane permutation: we want [pack(A,B) pack(C,D)]
        // Permute 128-bit lanes. Control byte: 0b_11_01_10_00 = 0xD8
        let permuted = _mm256_permute4x64_epi64(packed, 0xD8);

        // Store 32 u8s
        _mm256_storeu_si256(output.as_mut_ptr().add(i) as *mut __m256i, permuted);

        i += 32;
    }
}

/// AVX2-accelerated L2 Matrix Multiply (32 length u8 * 32 length i8)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn l2_forward_avx2(
    input: &[u8; 32],
    weights: &[[i8; 32]],
    biases: &[i32],
    output: &mut [i32],
) {
    let num_outputs = output.len();

    // Load input once: 32 bytes = 256 bits exactly
    let inp = _mm256_loadu_si256(input.as_ptr() as *const __m256i);
    let ones = _mm256_set1_epi16(1);

    for j in 0..num_outputs {
        let wgt = _mm256_loadu_si256(weights[j].as_ptr() as *const __m256i);

        // Multiply u8 * i8 -> i16 pairs
        let product = _mm256_maddubs_epi16(inp, wgt);

        // Widen i16 -> i32 by adding adjacent pairs
        let widened = _mm256_madd_epi16(product, ones);

        // Horizontal sum of the 8 i32 lanes
        let hi128 = _mm256_extracti128_si256(widened, 1);
        let lo128 = _mm256_castsi256_si128(widened);
        let sum128 = _mm_add_epi32(lo128, hi128);

        let hi64 = _mm_unpackhi_epi64(sum128, sum128);
        let sum64 = _mm_add_epi32(sum128, hi64);

        let hi32 = _mm_shuffle_epi32(sum64, 1);
        let sum32 = _mm_add_epi32(sum64, hi32);

        let total = _mm_cvtsi128_si32(sum32);
        output[j] = biases[j] + total;
    }
}

/// AVX2-accelerated L1 Output Scaling (i32 >> 6 -> u8 clipped [0, 127])
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn l1_clipped_avx2(input: &[i32; 32], output: &mut [u8; 32]) {
    let zeros = _mm256_setzero_si256();
    let max_vals = _mm256_set1_epi16(127);

    // Process 8 `i32`s per turn x 4 turns = 32 ints total (256 bits per load)
    let mut i = 0;
    while i + 8 <= 32 {
        let v0 = _mm256_loadu_si256(input.as_ptr().add(i) as *const __m256i);
        let v1 = _mm256_loadu_si256(input.as_ptr().add(i + 8) as *const __m256i);

        // Shift right by 6
        let s0 = _mm256_srai_epi32(v0, 6);
        let s1 = _mm256_srai_epi32(v1, 6);

        // Pack i32 -> i16 (Requires v0 and v1)
        // Result is [ 4xi16 from s0 | 4xi16 from s1 | 4xi16 from s0 | 4xi16 from s1 ]
        // (Due to AVX2 packing behavior on 128-bit blocks)
        let mut p16 = _mm256_packs_epi32(s0, s1);

        // Clamp to [0, 127]
        p16 = _mm256_min_epi16(_mm256_max_epi16(p16, zeros), max_vals);

        // Pack i16 -> u8
        // We need 16 total elements. We can pack `p16` with zero.
        let p8 = _mm256_packus_epi16(p16, zeros);

        // Fix the cross-lane permutation for 16-bit packed results
        let permuted = _mm256_permute4x64_epi64(p8, 0xD8);

        // We only want the first 16 bytes (128 bits) -> store via SSE
        let lower128 = _mm256_castsi256_si128(permuted);

        // Actually, we process 16 outputs at a time (8 + 8). Let's step by 16.
        _mm_storeu_si128(output.as_mut_ptr().add(i) as *mut __m128i, lower128);

        i += 16;
    }
}

/// Scalar fallback — used when AVX2 is not available or on non-x86
pub fn linear_forward_scalar(
    input: &[u8; 512],
    weights: &[[i8; 512]],
    biases: &[i32],
    output: &mut [i32],
) {
    let num_outputs = output.len();
    for j in 0..num_outputs {
        let mut sum = biases[j];
        for k in 0..512 {
            sum += input[k] as i32 * weights[j][k] as i32;
        }
        output[j] = sum;
    }
}

pub fn clipped_relu_scalar(input: &[i16; 256], output: &mut [u8; 256]) {
    for i in 0..256 {
        let val = input[i];
        if val <= 0 {
            output[i] = 0;
        } else if val >= 127 {
            output[i] = 127;
        } else {
            output[i] = val as u8;
        }
    }
}

pub fn l1_clipped_scalar(input: &[i32; 32], output: &mut [u8; 32]) {
    for i in 0..32 {
        let val = input[i] >> 6;
        if val <= 0 {
            output[i] = 0;
        } else if val >= 127 {
            output[i] = 127;
        } else {
            output[i] = val as u8;
        }
    }
}

pub fn l2_forward_scalar(
    input: &[u8; 32],
    weights: &[[i8; 32]],
    biases: &[i32],
    output: &mut [i32],
) {
    let num_outputs = output.len();
    for j in 0..num_outputs {
        let mut sum = biases[j];
        for k in 0..32 {
            sum += input[k] as i32 * weights[j][k] as i32;
        }
        output[j] = sum;
    }
}
