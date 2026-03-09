use std::fs::File;
/// NNUE Network Parameters and Binary Loader
///
/// Parses Stockfish-compatible .nnue files in little-endian format.
/// Network architecture: HalfKP → FT(256) → L1(32) → L2(32) → Output(1)
use std::io::{self, BufReader, Read};

use super::feature::{HALFKP_FEATURES, TRANSFORMED_SIZE};

/// Hidden layer sizes
pub const L1_SIZE: usize = 32;
pub const L2_SIZE: usize = 32;

/// Network parameters — all stored as aligned arrays
pub struct NetworkParams {
    // Feature Transformer: HALFKP_FEATURES * TRANSFORMED_SIZE weights (i8)
    pub ft_weight: Vec<i8>,
    // Feature Transformer bias: TRANSFORMED_SIZE values (i16)
    pub ft_bias: [i16; TRANSFORMED_SIZE],

    // Layer 1: (2 * TRANSFORMED_SIZE) → L1_SIZE
    // Input is concatenation of [white_acc, black_acc] = 512 values
    pub l1_weight: [[i8; 512]; L1_SIZE],
    pub l1_bias: [i32; L1_SIZE],

    // Layer 2: L1_SIZE → L2_SIZE
    pub l2_weight: [[i8; L1_SIZE]; L2_SIZE],
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
            ft_bias: [0i16; TRANSFORMED_SIZE],
            l1_weight: [[0i8; 512]; L1_SIZE],
            l1_bias: [0i32; L1_SIZE],
            l2_weight: [[0i8; L1_SIZE]; L2_SIZE],
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
        // Magic: 4 bytes (version), Description: skip variable length
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;

        // Read description length + description (null-terminated string marker)
        // Stockfish NNUE format: after magic, there's a hash, then architecture description
        let mut hash = [0u8; 4];
        reader.read_exact(&mut hash)?;

        // Architecture description size (4 bytes LE)
        let mut desc_size_buf = [0u8; 4];
        reader.read_exact(&mut desc_size_buf)?;
        let desc_size = u32::from_le_bytes(desc_size_buf) as usize;

        // Skip description
        let mut desc = vec![0u8; desc_size];
        reader.read_exact(&mut desc)?;

        // --- Feature Transformer Header ---
        let mut ft_hash = [0u8; 4];
        reader.read_exact(&mut ft_hash)?;

        // FT Biases: TRANSFORMED_SIZE * 2 bytes (i16 LE)
        let mut bias_buf = [0u8; TRANSFORMED_SIZE * 2];
        reader.read_exact(&mut bias_buf)?;
        for i in 0..TRANSFORMED_SIZE {
            params.ft_bias[i] = i16::from_le_bytes([bias_buf[i * 2], bias_buf[i * 2 + 1]]);
        }

        // FT Weights: HALFKP_FEATURES * TRANSFORMED_SIZE bytes (i8)
        let ft_weight_count = HALFKP_FEATURES * TRANSFORMED_SIZE;
        let mut weight_buf = vec![0u8; ft_weight_count];
        reader.read_exact(&mut weight_buf)?;
        for i in 0..ft_weight_count {
            params.ft_weight[i] = weight_buf[i] as i8;
        }

        // --- Network Layers Header ---
        let mut net_hash = [0u8; 4];
        reader.read_exact(&mut net_hash)?;

        // Layer 1 biases: L1_SIZE * 4 bytes (i32 LE)
        for j in 0..L1_SIZE {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            params.l1_bias[j] = i32::from_le_bytes(buf);
        }

        // Layer 1 weights: L1_SIZE * 512 bytes (i8), stored row-major
        for j in 0..L1_SIZE {
            let mut row = [0u8; 512];
            reader.read_exact(&mut row)?;
            for k in 0..512 {
                params.l1_weight[j][k] = row[k] as i8;
            }
        }

        // Layer 2 biases: L2_SIZE * 4 bytes (i32 LE)
        for j in 0..L2_SIZE {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf)?;
            params.l2_bias[j] = i32::from_le_bytes(buf);
        }

        // Layer 2 weights: L2_SIZE * L1_SIZE bytes (i8)
        for j in 0..L2_SIZE {
            let mut row = [0u8; L1_SIZE];
            reader.read_exact(&mut row)?;
            for k in 0..L1_SIZE {
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
