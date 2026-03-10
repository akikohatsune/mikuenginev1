use std::fs::File;
/// NNUE Network Parameters and Binary Loader
///
/// HalfKAv2_hm architecture (Stockfish-compatible):
///   FT: HALFKA_FEATURES(22528) × TRANSFORMED_SIZE(768) — i8 weights, i16 biases
///   PSQT: HALFKA_FEATURES × PSQT_BUCKETS — i32 weights (8 buckets)
///   L1: (2 × TRANSFORMED_SIZE × 2) → L1_SIZE (SqrCReLU doubles input dim)
///   L2: L1_SIZE → L2_SIZE
///   Output: L2_SIZE → 1 scalar
use std::io::{self, BufReader, Read};

use super::feature::{HALFKP_FEATURES, TRANSFORMED_SIZE};

/// Hidden layer sizes (same as Stockfish's Big network)
pub const L1_SIZE: usize = 16;  // FC_0_OUTPUTS
pub const L2_SIZE: usize = 32;  // FC_1_OUTPUTS

/// PSQT buckets (8, like Stockfish)
pub const PSQT_BUCKETS: usize = 8;

/// L1 input size: SqrCReLU doubles FT output (2*TRANSFORMED_SIZE) × 2 = 4*TRANSFORMED_SIZE
/// But we concatenate both sides (stm + nstm): 2 * TRANSFORMED_SIZE each side.
/// After SqrCReLU: each half produces L1_SIZE_HALF = 2 * TRANSFORMED_SIZE outputs.
/// Here we use the flat concatenated size: 2 * TRANSFORMED_SIZE * 2 sides
pub const L1_INPUT_SIZE: usize = 2 * TRANSFORMED_SIZE * 2;  // = 3072

/// Network parameters — all stored as aligned arrays
pub struct NetworkParams {
    // Feature Transformer: HALFKA_FEATURES × TRANSFORMED_SIZE weights (i8)
    pub ft_weight: Vec<i8>,
    // Feature Transformer bias: TRANSFORMED_SIZE values (i16)
    pub ft_bias: Vec<i16>,

    // PSQT weights: HALFKA_FEATURES × PSQT_BUCKETS (i32)
    pub psqt_weight: Vec<i32>,

    // Layer 1: L1_INPUT_SIZE → L1_SIZE
    pub l1_weight: Vec<[i8; L1_INPUT_SIZE]>,
    pub l1_bias: [i32; L1_SIZE],

    // Layer 2: L1_SIZE × 2 → L2_SIZE (×2 for SqrCReLU concatenation)
    pub l2_weight: Vec<[i8; L1_SIZE * 2]>,
    pub l2_bias: [i32; L2_SIZE],

    // Output: L2_SIZE → 1
    pub out_weight: [i8; L2_SIZE],
    pub out_bias: i32,
}

impl Default for NetworkParams {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkParams {
    pub fn new() -> Self {
        let ft_weight_size = HALFKP_FEATURES * TRANSFORMED_SIZE;
        NetworkParams {
            ft_weight: vec![0i8; ft_weight_size],
            ft_bias: vec![0i16; TRANSFORMED_SIZE],
            psqt_weight: vec![0i32; HALFKP_FEATURES * PSQT_BUCKETS],
            l1_weight: vec![[0i8; L1_INPUT_SIZE]; L1_SIZE],
            l1_bias: [0i32; L1_SIZE],
            l2_weight: vec![[0i8; L1_SIZE * 2]; L2_SIZE],
            l2_bias: [0i32; L2_SIZE],
            out_weight: [0i8; L2_SIZE],
            out_bias: 0,
        }
    }

    /// Load network from a .nnue binary file
    pub fn load(path: &str) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut params = Self::new();

        // --- Header ---
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;

        let mut hash = [0u8; 4];
        reader.read_exact(&mut hash)?;

        let mut desc_size_buf = [0u8; 4];
        reader.read_exact(&mut desc_size_buf)?;
        let desc_size = u32::from_le_bytes(desc_size_buf) as usize;

        let mut desc = vec![0u8; desc_size];
        reader.read_exact(&mut desc)?;

        // --- Feature Transformer Header ---
        let mut ft_hash = [0u8; 4];
        reader.read_exact(&mut ft_hash)?;

        // PSQT Weights: HALFKA_FEATURES × PSQT_BUCKETS i32 values
        let psqt_count = HALFKP_FEATURES * PSQT_BUCKETS;
        let mut psqt_buf = vec![0u8; psqt_count * 4];
        reader.read_exact(&mut psqt_buf)?;
        for i in 0..psqt_count {
            params.psqt_weight[i] = i32::from_le_bytes([
                psqt_buf[i * 4],
                psqt_buf[i * 4 + 1],
                psqt_buf[i * 4 + 2],
                psqt_buf[i * 4 + 3],
            ]);
        }

        // FT Biases: TRANSFORMED_SIZE × 2 bytes (i16 LE)
        let mut bias_buf = vec![0u8; TRANSFORMED_SIZE * 2];
        reader.read_exact(&mut bias_buf)?;
        for i in 0..TRANSFORMED_SIZE {
            params.ft_bias[i] = i16::from_le_bytes([bias_buf[i * 2], bias_buf[i * 2 + 1]]);
        }

        // FT Weights: HALFKA_FEATURES × TRANSFORMED_SIZE bytes (i8)
        let ft_weight_count = HALFKP_FEATURES * TRANSFORMED_SIZE;
        let mut weight_buf = vec![0u8; ft_weight_count];
        reader.read_exact(&mut weight_buf)?;
        for i in 0..ft_weight_count {
            params.ft_weight[i] = weight_buf[i] as i8;
        }

        // --- Network Layers Header ---
        let mut net_hash = [0u8; 4];
        reader.read_exact(&mut net_hash)?;

        // Layer 1 biases: L1_SIZE × 4 bytes (i32 LE)
        for j in 0..L1_SIZE {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            params.l1_bias[j] = i32::from_le_bytes(buf);
        }

        // Layer 1 weights: L1_SIZE × L1_INPUT_SIZE bytes (i8), stored row-major
        for j in 0..L1_SIZE {
            let mut row = vec![0u8; L1_INPUT_SIZE];
            reader.read_exact(&mut row)?;
            for k in 0..L1_INPUT_SIZE {
                params.l1_weight[j][k] = row[k] as i8;
            }
        }

        // Layer 2 biases: L2_SIZE × 4 bytes (i32 LE)
        for j in 0..L2_SIZE {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            params.l2_bias[j] = i32::from_le_bytes(buf);
        }

        // Layer 2 weights: L2_SIZE × (L1_SIZE*2) bytes (i8)
        for j in 0..L2_SIZE {
            let mut row = vec![0u8; L1_SIZE * 2];
            reader.read_exact(&mut row)?;
            for k in 0..(L1_SIZE * 2) {
                params.l2_weight[j][k] = row[k] as i8;
            }
        }

        // Output bias: 4 bytes (i32 LE)
        let mut out_bias_buf = [0u8; 4];
        reader.read_exact(&mut out_bias_buf)?;
        params.out_bias = i32::from_le_bytes(out_bias_buf);

        // Output weights: L2_SIZE bytes (i8)
        let mut out_w_buf = [0u8; L2_SIZE];
        reader.read_exact(&mut out_w_buf)?;
        for k in 0..L2_SIZE {
            params.out_weight[k] = out_w_buf[k] as i8;
        }

        Ok(params)
    }
}
