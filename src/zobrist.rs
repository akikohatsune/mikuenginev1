use crate::types::{Color, PieceType, Square, CastlingRights};

pub const NUM_PIECES: usize = 12; // 2 colors * 6 types
pub const NUM_SQUARES: usize = 64;

pub struct ZobristKeys {
    pub pieces: [[u64; NUM_SQUARES]; NUM_PIECES],
    pub side_to_move: u64,
    pub castling: [u64; 16],
    pub en_passant: [u64; 8], // File 0-7
}

impl ZobristKeys {
    pub const fn new() -> Self {
        let mut seed = 1070372_u64;
        
        const fn rand64(s: &mut u64) -> u64 {
            let mut x = *s;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            *s = x;
            x.wrapping_mul(2685821657736338717)
        }

        let mut pieces = [[0; NUM_SQUARES]; NUM_PIECES];
        let mut castling = [0; 16];
        let mut en_passant = [0; 8];

        let mut p = 0;
        while p < NUM_PIECES {
            let mut sq = 0;
            while sq < NUM_SQUARES {
                pieces[p][sq] = rand64(&mut seed);
                sq += 1;
            }
            p += 1;
        }

        let mut c = 0;
        while c < 16 {
            castling[c] = rand64(&mut seed);
            c += 1;
        }

        let mut f = 0;
        while f < 8 {
            en_passant[f] = rand64(&mut seed);
            f += 1;
        }

        let side_to_move = rand64(&mut seed);

        ZobristKeys {
            pieces,
            side_to_move,
            castling,
            en_passant,
        }
    }
}

pub static KEYS: ZobristKeys = ZobristKeys::new();

#[inline(always)]
pub fn piece_key(color: Color, pt: PieceType, sq: Square) -> u64 {
    let p_idx = (color as usize) * 6 + (pt as usize);
    KEYS.pieces[p_idx][sq.0 as usize]
}

#[inline(always)]
pub fn side_key() -> u64 {
    KEYS.side_to_move
}

#[inline(always)]
pub fn castling_key(rights: CastlingRights) -> u64 {
    KEYS.castling[(rights.0 & 15) as usize]
}

#[inline(always)]
pub fn ep_key(ep_square: Square) -> u64 {
    KEYS.en_passant[ep_square.file() as usize]
}
