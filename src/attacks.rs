use crate::bitboard::{Bitboard, FILE_A, FILE_H};
use crate::types::{Color, Square};

// ──────────────────────────────────────────────────────────────────────────
//   PRECOMPUTED LEAP ATTACKS (pawn / knight / king) — compile-time tables
// ──────────────────────────────────────────────────────────────────────────

pub struct Attacks {
    pub pawn_attacks: [[Bitboard; 64]; 2],
    pub knight_attacks: [Bitboard; 64],
    pub king_attacks: [Bitboard; 64],
}

impl Default for Attacks {
    fn default() -> Self {
        Self::new()
    }
}

impl Attacks {
    pub const fn new() -> Self {
        let mut pawn_attacks = [[Bitboard::new(0); 64]; 2];
        let mut knight_attacks = [Bitboard::new(0); 64];
        let mut king_attacks = [Bitboard::new(0); 64];

        let mut sq = 0;
        while sq < 64 {
            let bb = Bitboard::new(1u64 << sq);

            // White pawn attacks (north-west / north-east)
            let nw = (bb.0 << 7) & !FILE_H;
            let ne = (bb.0 << 9) & !FILE_A;
            pawn_attacks[Color::White as usize][sq] = Bitboard::new(nw | ne);

            // Black pawn attacks (south-west / south-east)
            let sw = (bb.0 >> 9) & !FILE_H;
            let se = (bb.0 >> 7) & !FILE_A;
            pawn_attacks[Color::Black as usize][sq] = Bitboard::new(sw | se);

            // Knight
            let mut n = 0;
            n |= (bb.0 << 17) & !FILE_A;
            n |= (bb.0 << 15) & !FILE_H;
            n |= (bb.0 << 10) & !(FILE_A | (FILE_A << 1));
            n |= (bb.0 << 6) & !(FILE_H | (FILE_H >> 1));
            n |= (bb.0 >> 15) & !FILE_A;
            n |= (bb.0 >> 17) & !FILE_H;
            n |= (bb.0 >> 6) & !(FILE_A | (FILE_A << 1));
            n |= (bb.0 >> 10) & !(FILE_H | (FILE_H >> 1));
            knight_attacks[sq] = Bitboard::new(n);

            // King
            let mut k = 0;
            k |= bb.0 << 8;
            k |= bb.0 >> 8;
            k |= (bb.0 << 1) & !FILE_A;
            k |= (bb.0 >> 1) & !FILE_H;
            k |= (bb.0 << 9) & !FILE_A;
            k |= (bb.0 << 7) & !FILE_H;
            k |= (bb.0 >> 7) & !FILE_A;
            k |= (bb.0 >> 9) & !FILE_H;
            king_attacks[sq] = Bitboard::new(k);

            sq += 1;
        }

        Attacks {
            pawn_attacks,
            knight_attacks,
            king_attacks,
        }
    }
}

pub static ATTACKS: Attacks = Attacks::new();

#[inline(always)]
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    ATTACKS.pawn_attacks[color as usize][sq.0 as usize]
}

#[inline(always)]
pub fn knight_attacks(sq: Square) -> Bitboard {
    ATTACKS.knight_attacks[sq.0 as usize]
}

#[inline(always)]
pub fn king_attacks(sq: Square) -> Bitboard {
    ATTACKS.king_attacks[sq.0 as usize]
}

// ──────────────────────────────────────────────────────────────────────────
//   MAGIC BITBOARDS for sliding pieces
// ──────────────────────────────────────────────────────────────────────────

/// A single magic entry for one square.
#[derive(Copy, Clone)]
struct MagicEntry {
    mask: u64,
    magic: u64,
    shift: u32,
    offset: usize,
}

/// Global slider attack tables, initialised once at startup.
pub struct MagicTables {
    rook_magics: [MagicEntry; 64],
    bishop_magics: [MagicEntry; 64],
    table: Vec<Bitboard>,
}

// Well-known magic numbers (from the Chess Programming Wiki / Stockfish sources)
const ROOK_MAGIC_NUMBERS: [u64; 64] = [
    0x0080001020400080,
    0x0040001000200040,
    0x0080081000200080,
    0x0080040800100080,
    0x0080020400080080,
    0x0080010200040080,
    0x0080008001000200,
    0x0080002040800100,
    0x0000800020400080,
    0x0000400020005000,
    0x0000801000200080,
    0x0000800800100080,
    0x0000800400080080,
    0x0000800200040080,
    0x0000800100020080,
    0x0000800040800100,
    0x0000208000400080,
    0x0000404000201000,
    0x0000808010002000,
    0x0000808008001000,
    0x0000808004000800,
    0x0000808002000400,
    0x0000010100020004,
    0x0000020000408104,
    0x0000208080004000,
    0x0000200040005000,
    0x0000100080200080,
    0x0000080080100080,
    0x0000040080080080,
    0x0000020080040080,
    0x0000010080800200,
    0x0000800080004100,
    0x0000204000800080,
    0x0000200040401000,
    0x0000100080802000,
    0x0000080080801000,
    0x0000040080800800,
    0x0000020080800400,
    0x0000020001010004,
    0x0000800040800100,
    0x0000204000808000,
    0x0000200040008080,
    0x0000100020008080,
    0x0000080010008080,
    0x0000040008008080,
    0x0000020004008080,
    0x0000010002008080,
    0x0000004081020004,
    0x0000204000800080,
    0x0000200040008080,
    0x0000100020008080,
    0x0000080010008080,
    0x0000040008008080,
    0x0000020004008080,
    0x0000800100020080,
    0x0000800041000080,
    0x00FFFCDDFCED714A,
    0x007FFCDDFCED714A,
    0x003FFFCDFFD88096,
    0x0000040810002101,
    0x0001000204080011,
    0x0001000204000801,
    0x0001000082000401,
    0x0001FFFAABFAD1A2,
];

const BISHOP_MAGIC_NUMBERS: [u64; 64] = [
    0x0002020202020200,
    0x0002020202020000,
    0x0004010202000000,
    0x0004040080000000,
    0x0001104000000000,
    0x0000821040000000,
    0x0000410410400000,
    0x0000104104104000,
    0x0000040404040400,
    0x0000020202020200,
    0x0000040102020000,
    0x0000040400800000,
    0x0000011040000000,
    0x0000008210400000,
    0x0000004104104000,
    0x0000002082082000,
    0x0004000808080800,
    0x0002000404040400,
    0x0001000202020200,
    0x0000800802004000,
    0x0000800400A00000,
    0x0000200100884000,
    0x0000400082082000,
    0x0000200041041000,
    0x0002080010101000,
    0x0001040008080800,
    0x0000208004010400,
    0x0000404004010200,
    0x0000840000802000,
    0x0000404002011000,
    0x0000808001041000,
    0x0000404000820800,
    0x0001041000202000,
    0x0000820800101000,
    0x0000104400080800,
    0x0000020080080080,
    0x0000404040040100,
    0x0000808100020100,
    0x0001010100020800,
    0x0000808080010400,
    0x0000820820004000,
    0x0000410410002000,
    0x0000082088001000,
    0x0000002011000800,
    0x0000080100400400,
    0x0001010101000200,
    0x0002020202000400,
    0x0001010101000200,
    0x0000410410400000,
    0x0000208208200000,
    0x0000002084100000,
    0x0000000020880000,
    0x0000001002020000,
    0x0000040408020000,
    0x0004040404040000,
    0x0002020202020000,
    0x0000104104104000,
    0x0000002082082000,
    0x0000000020841000,
    0x0000000000208800,
    0x0000000010020200,
    0x0000000404080200,
    0x0000040404040400,
    0x0002020202020200,
];

/// Number of relevant bits for rook on each square (determines table size).
const ROOK_BITS: [u32; 64] = [
    12, 11, 11, 11, 11, 11, 11, 12, 11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11, 11, 10, 10, 10, 10, 10, 10, 11,
    11, 10, 10, 10, 10, 10, 10, 11, 12, 11, 11, 11, 11, 11, 11, 12,
];

/// Number of relevant bits for bishop on each square.
const BISHOP_BITS: [u32; 64] = [
    6, 5, 5, 5, 5, 5, 5, 6, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 7, 7, 7, 7, 5, 5, 5, 5, 7, 9, 9, 7, 5, 5,
    5, 5, 7, 9, 9, 7, 5, 5, 5, 5, 7, 7, 7, 7, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 5, 5, 5, 5, 5, 5, 6,
];

/// Compute the rook mask for a square (edges excluded).
fn rook_mask(sq: u32) -> u64 {
    let r = sq / 8;
    let f = sq % 8;
    let mut mask = 0u64;
    // North (rank increases)
    for i in (r + 1)..7 {
        mask |= 1u64 << (i * 8 + f);
    }
    // South
    for i in 1..r {
        mask |= 1u64 << (i * 8 + f);
    }
    // East
    for i in (f + 1)..7 {
        mask |= 1u64 << (r * 8 + i);
    }
    // West
    for i in 1..f {
        mask |= 1u64 << (r * 8 + i);
    }
    mask
}

/// Compute the bishop mask for a square (edges excluded).
fn bishop_mask(sq: u32) -> u64 {
    let r = sq as i32 / 8;
    let f = sq as i32 % 8;
    let mut mask = 0u64;
    let dirs: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    for (dr, df) in dirs {
        let mut cr = r + dr;
        let mut cf = f + df;
        while (1..=6).contains(&cr) && (1..=6).contains(&cf) {
            mask |= 1u64 << (cr * 8 + cf);
            cr += dr;
            cf += df;
        }
    }
    mask
}

/// Compute the actual sliding attacks for a rook on `sq` given `occ` (full ray until blocked).
fn rook_attacks_slow(sq: u32, occ: u64) -> u64 {
    let r = sq as i32 / 8;
    let f = sq as i32 % 8;
    let mut attacks = 0u64;
    // North
    let mut cr = r + 1;
    while cr <= 7 {
        let bit = 1u64 << (cr * 8 + f);
        attacks |= bit;
        if occ & bit != 0 {
            break;
        }
        cr += 1;
    }
    // South
    cr = r - 1;
    while cr >= 0 {
        let bit = 1u64 << (cr * 8 + f);
        attacks |= bit;
        if occ & bit != 0 {
            break;
        }
        cr -= 1;
    }
    // East
    let mut cf = f + 1;
    while cf <= 7 {
        let bit = 1u64 << (r * 8 + cf);
        attacks |= bit;
        if occ & bit != 0 {
            break;
        }
        cf += 1;
    }
    // West
    cf = f - 1;
    while cf >= 0 {
        let bit = 1u64 << (r * 8 + cf);
        attacks |= bit;
        if occ & bit != 0 {
            break;
        }
        cf -= 1;
    }
    attacks
}

/// Compute the actual sliding attacks for a bishop on `sq` given `occ`.
fn bishop_attacks_slow(sq: u32, occ: u64) -> u64 {
    let r = sq as i32 / 8;
    let f = sq as i32 % 8;
    let mut attacks = 0u64;
    let dirs: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    for (dr, df) in dirs {
        let mut cr = r + dr;
        let mut cf = f + df;
        while (0..=7).contains(&cr) && (0..=7).contains(&cf) {
            let bit = 1u64 << (cr * 8 + cf);
            attacks |= bit;
            if occ & bit != 0 {
                break;
            }
            cr += dr;
            cf += df;
        }
    }
    attacks
}

/// Enumerate all subsets of `mask` (Carry-Rippler) and populate the attack table.
fn enumerate_subsets(mask: u64) -> Vec<u64> {
    let mut subsets = Vec::new();
    let mut subset = 0u64;
    loop {
        subsets.push(subset);
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 {
            break;
        }
    }
    subsets
}

use std::sync::OnceLock;

static MAGIC_TABLES: OnceLock<MagicTables> = OnceLock::new();

/// Must be called once before any search. Builds all magic lookup tables.
pub fn init_magics() {
    MAGIC_TABLES.get_or_init(|| {
        let mut rook_entries = [MagicEntry {
            mask: 0,
            magic: 0,
            shift: 0,
            offset: 0,
        }; 64];
        let mut bishop_entries = [MagicEntry {
            mask: 0,
            magic: 0,
            shift: 0,
            offset: 0,
        }; 64];

        // Calculate total table size needed
        let mut total_size = 0usize;
        for sq in 0..64 {
            total_size += 1usize << ROOK_BITS[sq];
        }
        for sq in 0..64 {
            total_size += 1usize << BISHOP_BITS[sq];
        }

        let mut table = vec![Bitboard::new(0); total_size];
        let mut offset = 0;

        // Build rook tables
        for sq in 0..64u32 {
            let mask = rook_mask(sq);
            let bits = ROOK_BITS[sq as usize];
            let magic = ROOK_MAGIC_NUMBERS[sq as usize];
            let shift = 64 - bits;
            let table_size = 1usize << bits;

            rook_entries[sq as usize] = MagicEntry {
                mask,
                magic,
                shift,
                offset,
            };

            let subsets = enumerate_subsets(mask);
            for occ in subsets {
                let idx = ((occ.wrapping_mul(magic)) >> shift) as usize;
                debug_assert!(idx < table_size, "rook magic collision at sq={}", sq);
                let attacks = rook_attacks_slow(sq, occ);
                let dest = &mut table[offset + idx];
                if dest.0 == 0 || dest.0 == attacks {
                    *dest = Bitboard::new(attacks);
                } else {
                    // Constructive collision: magics map different occupancies to the same
                    // index only if they produce the same attack set. If not, the magic is bad.
                    // Fall back — this should not happen with verified magics.
                    // We allow overlapping entries if attacks match (constructive collision).
                    debug_assert!(
                        dest.0 == attacks,
                        "destructive rook magic collision at sq={}",
                        sq
                    );
                }
            }

            offset += table_size;
        }

        // Build bishop tables
        for sq in 0..64u32 {
            let mask = bishop_mask(sq);
            let bits = BISHOP_BITS[sq as usize];
            let magic = BISHOP_MAGIC_NUMBERS[sq as usize];
            let shift = 64 - bits;
            let table_size = 1usize << bits;

            bishop_entries[sq as usize] = MagicEntry {
                mask,
                magic,
                shift,
                offset,
            };

            let subsets = enumerate_subsets(mask);
            for occ in subsets {
                let idx = ((occ.wrapping_mul(magic)) >> shift) as usize;
                let attacks = bishop_attacks_slow(sq, occ);
                let dest = &mut table[offset + idx];
                if dest.0 == 0 || dest.0 == attacks {
                    *dest = Bitboard::new(attacks);
                } else {
                    debug_assert!(
                        dest.0 == attacks,
                        "destructive bishop magic collision at sq={}",
                        sq
                    );
                }
            }

            offset += table_size;
        }

        MagicTables {
            rook_magics: rook_entries,
            bishop_magics: bishop_entries,
            table,
        }
    });
}

// ──────────────────────────────────────────────────────────────────────────
//   PUBLIC SLIDER ATTACK FUNCTIONS  (hot path — must be as fast as possible)
// ──────────────────────────────────────────────────────────────────────────

#[inline(always)]
pub fn bishop_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let tables = unsafe { MAGIC_TABLES.get().unwrap_unchecked() };
    let entry = unsafe { tables.bishop_magics.get_unchecked(sq.0 as usize) };
    let idx = (((occ.0 & entry.mask).wrapping_mul(entry.magic)) >> entry.shift) as usize;
    unsafe { *tables.table.get_unchecked(entry.offset + idx) }
}

#[inline(always)]
pub fn rook_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    let tables = unsafe { MAGIC_TABLES.get().unwrap_unchecked() };
    let entry = unsafe { tables.rook_magics.get_unchecked(sq.0 as usize) };
    let idx = (((occ.0 & entry.mask).wrapping_mul(entry.magic)) >> entry.shift) as usize;
    unsafe { *tables.table.get_unchecked(entry.offset + idx) }
}

#[inline(always)]
pub fn queen_attacks(sq: Square, occ: Bitboard) -> Bitboard {
    bishop_attacks(sq, occ) | rook_attacks(sq, occ)
}

// Keep SliderAttacks type for backward compatibility but it's no longer used
pub struct SliderAttacks;
pub static SLIDER_ATTACKS: std::sync::OnceLock<SliderAttacks> = std::sync::OnceLock::new();
