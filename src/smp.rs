use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::types::Move;
use crate::transposition::TranspositionTable;

#[derive(Clone, Copy)]
pub struct RootMove {
    pub m: Move,
    pub score: i32,
    pub search_depth: u8,
}

impl RootMove {
    pub fn new(m: Move) -> Self {
        RootMove {
            m,
            score: -50000,
            search_depth: 0,
        }
    }
}

pub struct RootMoveList {
    pub moves: Vec<RootMove>,
    pub current_depth: u8,
}

impl RootMoveList {
    pub fn new(moves: Vec<Move>) -> Self {
        let mut root_moves = Vec::with_capacity(moves.len());
        for m in moves {
            root_moves.push(RootMove::new(m));
        }
        RootMoveList {
            moves: root_moves,
            current_depth: 1,
        }
    }

    pub fn steal(&mut self, depth: u8) -> Option<Move> {
        if let Some(m) = self.moves.iter_mut().find(|m| m.search_depth < depth) {
            m.search_depth = depth;
            Some(m.m)
        } else {
            None
        }
    }
    
    pub fn update_score(&mut self, m: Move, score: i32) {
        if let Some(rm) = self.moves.iter_mut().find(|rm| rm.m.0 == m.0) {
            rm.score = score;
        }
        // Sort moves by score so stealing prioritizes best moves
        self.moves.sort_by(|a, b| b.score.cmp(&a.score));
    }
}

pub struct SharedState {
    pub tt: Arc<TranspositionTable>,
    pub tb: Option<Arc<pyrrhic_rs::TableBases<crate::search::MikuAdapter>>>,
    pub stop_flag: Arc<AtomicBool>,
    pub root_moves: Mutex<RootMoveList>,
    pub global_best_move: AtomicU64,
}

impl SharedState {
    pub fn new(tt: Arc<TranspositionTable>, tb: Option<Arc<pyrrhic_rs::TableBases<crate::search::MikuAdapter>>>, stop_flag: Arc<AtomicBool>, root_moves: Vec<Move>) -> Self {
        SharedState {
            tt,
            tb,
            stop_flag,
            root_moves: Mutex::new(RootMoveList::new(root_moves)),
            global_best_move: AtomicU64::new(0),
        }
    }
    
    pub fn get_best_move(&self) -> u16 {
        self.global_best_move.load(Ordering::Relaxed) as u16
    }
    
    pub fn set_best_move(&self, m: Move) {
        self.global_best_move.store(m.0 as u64, Ordering::Relaxed);
    }
}
