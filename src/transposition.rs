use crate::types::Move;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    Exact = 0,
    Alpha = 1,
    Beta = 2,
    None = 3,
}

impl NodeType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => NodeType::Exact,
            1 => NodeType::Alpha,
            2 => NodeType::Beta,
            _ => NodeType::None,
        }
    }
}

pub struct TTEntry {
    pub key: u64,
    pub score: i32,
    pub best_move: Move,
    pub depth: u8,
    pub node_type: NodeType,
    pub is_pv: bool,
}

impl TTEntry {
    pub const fn empty() -> Self {
        TTEntry {
            key: 0,
            score: 0,
            best_move: Move(0),
            depth: 0,
            node_type: NodeType::None,
            is_pv: false,
        }
    }

    // Pack data into a 64-bit unsigned integer
    // Bits 0-31: score (i32 as u32)
    // Bits 32-47: best_move (u16)
    // Bits 48-55: depth (u8)
    // Bits 56-62: node_type (u8)
    // Bit 63: is_pv (bool)
    #[inline(always)]
    fn pack(score: i32, best_move: Move, depth: u8, node_type: NodeType, is_pv: bool) -> u64 {
        let sc = (score as u32) as u64;
        let mv = (best_move.0 as u64) << 32;
        let dp = (depth as u64) << 48;
        let nt = ((node_type as u8 & 0x7F) as u64) << 56;
        let pv = if is_pv { 1u64 << 63 } else { 0 };
        sc | mv | dp | nt | pv
    }

    #[inline(always)]
    fn unpack(key: u64, data: u64) -> Self {
        let score = (data & 0xFFFFFFFF) as i32;
        let best_move = Move(((data >> 32) & 0xFFFF) as u16);
        let depth = ((data >> 48) & 0xFF) as u8;
        let node_type = NodeType::from_u8(((data >> 56) & 0x7F) as u8);
        let is_pv = (data & (1u64 << 63)) != 0;
        
        TTEntry {
            key,
            score,
            best_move,
            depth,
            node_type,
            is_pv,
        }
    }
}

pub struct TranspositionTable {
    pub keys: Vec<AtomicU64>,
    pub data: Vec<AtomicU64>,
    pub mask: usize,
}

impl TranspositionTable {
    pub fn new(mb: usize) -> Self {
        let entry_size = 16; // 8 bytes for key + 8 bytes for data
        let desired_entries = (mb * 1024 * 1024) / entry_size;
        
        let mut num_entries = desired_entries.next_power_of_two();
        if num_entries > desired_entries {
            num_entries /= 2;
        }
        let num_entries = num_entries.max(1);
        
        let mut keys = Vec::with_capacity(num_entries);
        let mut data = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            keys.push(AtomicU64::new(0));
            data.push(AtomicU64::new(0));
        }

        TranspositionTable {
            keys,
            data,
            mask: num_entries - 1,
        }
    }

    pub fn resize(&mut self, mb: usize) {
        *self = TranspositionTable::new(mb);
    }

    pub fn clear(&self) {
        for i in 0..self.keys.len() {
            self.keys[i].store(0, Ordering::Relaxed);
            self.data[i].store(0, Ordering::Relaxed);
        }
    }

    #[inline(always)]
    pub fn store(&self, key: u64, depth: u8, score: i32, node_type: NodeType, best_move: Move, is_pv: bool) {
        let index = (key as usize) & self.mask;
        
        // Retrieve current data to check depth replacement strategy safely
        let current_data = self.data[index].load(Ordering::Relaxed);
        let current_depth = ((current_data >> 48) & 0xFF) as u8;
        let current_key = self.keys[index].load(Ordering::Relaxed);
        
        if current_key == 0 || current_key == key || depth >= current_depth || is_pv {
            let packed_data = TTEntry::pack(score, best_move, depth, node_type, is_pv);
            
            // To prevent read tearing on other threads, store data first, then key
            self.data[index].store(packed_data, Ordering::Release);
            self.keys[index].store(key, Ordering::Release);
        }
    }

    #[inline(always)]
    pub fn probe(&self, key: u64) -> Option<TTEntry> {
        let index = (key as usize) & self.mask;
        
        // Read key first, then data
        let stored_key = self.keys[index].load(Ordering::Acquire);
        if stored_key == key {
            let data = self.data[index].load(Ordering::Acquire);
            
            // Re-verify key hasn't been torn by a racing write thread
            if self.keys[index].load(Ordering::Acquire) == key {
                let entry = TTEntry::unpack(key, data);
                if entry.node_type != NodeType::None {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// Returns how full the TT is, in permille (0 to 1000)
    pub fn hashfull(&self) -> u32 {
        let mut count = 0;
        let sample_size = self.keys.len().min(1000);
        if sample_size == 0 { return 0; }
        
        for i in 0..sample_size {
            let key = self.keys[i].load(Ordering::Relaxed);
            let data = self.data[i].load(Ordering::Relaxed);
            if key != 0 {
                let node_type = (data >> 56) & 0xFF;
                if node_type != 3 { // 3 == NodeType::None
                    count += 1;
                }
            }
        }
        (count * 1000 / sample_size) as u32
    }
}
