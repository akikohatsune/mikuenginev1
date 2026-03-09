use crate::types::{Color, Move, PieceType, Square};

pub const MAX_PLY: usize = 128;
const HISTORY_MAX: i32 = 16384;

pub struct Heuristics {
    pub history: [[[i32; 64]; 6]; 2],  // [Color][PieceType][ToSquare]
    pub killers: [[Move; 2]; MAX_PLY], // [Ply][Slot]
    pub countermoves: [[Move; 64]; 6], // [PrevPieceType][PrevToSq] -> Move
    pub capture_history: [[[i32; 6]; 64]; 6], // [AttackerPT][ToSq][VictimPT]
    pub cont_history: [[[[i32; 64]; 6]; 64]; 6], // [PrevPT][PrevTo][CurPT][CurTo]
    pub pawn_history: [[[i32; 64]; 64]; 2], // [Color][FromSq][ToSq]
    pub minor_piece_history: [[[i32; 64]; 64]; 2], // [Color][FromSq][ToSq]
    pub low_ply_history: [[i32; 4096]; 16], // [Ply][Move::raw()]
    pub static_evals: [i32; MAX_PLY],  // Static eval at each ply for improving

    // New Stockfish Heuristics
    pub non_pawn_corr: [[i32; 16384]; 2], // [Color][MaterialHash % 16384]
    pub cont_corr: [[[[i32; 64]; 6]; 64]; 6], // [PrevPT][PrevTo][CurPT][CurTo]
}

impl Default for Heuristics {
    fn default() -> Self {
        Self::new()
    }
}

impl Heuristics {
    pub fn new() -> Self {
        Heuristics {
            history: [[[0; 64]; 6]; 2],
            killers: [[Move::new(0, 0, 0); 2]; MAX_PLY],
            countermoves: [[Move::new(0, 0, 0); 64]; 6],
            capture_history: [[[0; 6]; 64]; 6],
            cont_history: [[[[0; 64]; 6]; 64]; 6],
            pawn_history: [[[0; 64]; 64]; 2],
            minor_piece_history: [[[0; 64]; 64]; 2],
            low_ply_history: [[0; 4096]; 16],
            static_evals: [0; MAX_PLY],
            non_pawn_corr: [[0; 16384]; 2],
            cont_corr: [[[[0; 64]; 6]; 64]; 6],
        }
    }

    pub fn clear(&mut self) {
        self.history = [[[0; 64]; 6]; 2];
        self.killers = [[Move::new(0, 0, 0); 2]; MAX_PLY];
        self.countermoves = [[Move::new(0, 0, 0); 64]; 6];
        self.capture_history = [[[0; 6]; 64]; 6];
        self.cont_history = [[[[0; 64]; 6]; 64]; 6];
        self.pawn_history = [[[0; 64]; 64]; 2];
        self.minor_piece_history = [[[0; 64]; 64]; 2];
        self.low_ply_history = [[0; 4096]; 16];
        self.static_evals = [0; MAX_PLY];
        self.non_pawn_corr = [[0; 16384]; 2];
        self.cont_corr = [[[[0; 64]; 6]; 64]; 6];
    }

    /// History Gravity update
    #[inline(always)]
    fn gravity_update(entry: &mut i32, bonus: i32) {
        *entry += bonus - *entry * bonus.abs() / HISTORY_MAX;
    }

    #[inline(always)]
    pub fn update_history(&mut self, color: Color, pt: PieceType, to: Square, depth: u8) {
        let bonus = (depth as i32) * (depth as i32);
        Self::gravity_update(
            &mut self.history[color as usize][pt as usize][to.0 as usize],
            bonus,
        );
    }

    /// Penalize quiet moves that didn't cause cutoff (negative history)
    #[inline(always)]
    pub fn penalize_history(&mut self, color: Color, pt: PieceType, to: Square, depth: u8) {
        let bonus = -((depth as i32) * (depth as i32));
        Self::gravity_update(
            &mut self.history[color as usize][pt as usize][to.0 as usize],
            bonus,
        );
    }

    #[inline(always)]
    pub fn get_history(&self, color: Color, pt: PieceType, to: Square) -> i32 {
        self.history[color as usize][pt as usize][to.0 as usize]
    }

    #[inline(always)]
    pub fn update_capture_history(
        &mut self,
        attacker: PieceType,
        to: Square,
        victim: PieceType,
        depth: u8,
    ) {
        let bonus = (depth as i32) * (depth as i32);
        Self::gravity_update(
            &mut self.capture_history[attacker as usize][to.0 as usize][victim as usize],
            bonus,
        );
    }

    #[inline(always)]
    pub fn get_capture_history(&self, attacker: PieceType, to: Square, victim: PieceType) -> i32 {
        self.capture_history[attacker as usize][to.0 as usize][victim as usize]
    }

    #[inline(always)]
    pub fn update_continuation(
        &mut self,
        prev_pt: PieceType,
        prev_to: Square,
        cur_pt: PieceType,
        cur_to: Square,
        depth: u8,
    ) {
        let bonus = (depth as i32) * (depth as i32);
        Self::gravity_update(
            &mut self.cont_history[prev_pt as usize][prev_to.0 as usize][cur_pt as usize]
                [cur_to.0 as usize],
            bonus,
        );
    }

    #[inline(always)]
    pub fn get_continuation(
        &self,
        prev_pt: PieceType,
        prev_to: Square,
        cur_pt: PieceType,
        cur_to: Square,
    ) -> i32 {
        self.cont_history[prev_pt as usize][prev_to.0 as usize][cur_pt as usize][cur_to.0 as usize]
    }

    #[inline(always)]
    pub fn update_killer(&mut self, m: Move, ply: usize) {
        if ply >= MAX_PLY {
            return;
        }
        if self.killers[ply][0].0 != m.0 {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = m;
        }
    }

    #[inline(always)]
    pub fn is_killer(&self, m: Move, ply: usize) -> bool {
        if ply >= MAX_PLY {
            return false;
        }
        self.killers[ply][0].0 == m.0 || self.killers[ply][1].0 == m.0
    }

    #[inline(always)]
    pub fn killer_slot(&self, m: Move, ply: usize) -> usize {
        if ply >= MAX_PLY {
            return 0;
        }
        if self.killers[ply][0].0 == m.0 {
            return 1;
        }
        if self.killers[ply][1].0 == m.0 {
            return 2;
        }
        0
    }

    #[inline(always)]
    pub fn update_countermove(&mut self, prev_pt: PieceType, prev_to: Square, m: Move) {
        self.countermoves[prev_pt as usize][prev_to.0 as usize] = m;
    }

    #[inline(always)]
    pub fn get_countermove(&self, prev_pt: PieceType, prev_to: Square) -> Move {
        self.countermoves[prev_pt as usize][prev_to.0 as usize]
    }

    #[inline(always)]
    pub fn update_pawn_history(&mut self, color: Color, from: Square, to: Square, depth: u8) {
        let bonus = (depth as i32) * (depth as i32);
        Self::gravity_update(
            &mut self.pawn_history[color as usize][from.0 as usize][to.0 as usize],
            bonus,
        );
    }

    #[inline(always)]
    pub fn get_pawn_history(&self, color: Color, from: Square, to: Square) -> i32 {
        self.pawn_history[color as usize][from.0 as usize][to.0 as usize]
    }

    #[inline(always)]
    pub fn penalize_pawn_history(&mut self, color: Color, from: Square, to: Square, depth: u8) {
        let penalty = -((depth as i32) * (depth as i32));
        Self::gravity_update(
            &mut self.pawn_history[color as usize][from.0 as usize][to.0 as usize],
            penalty,
        );
    }

    #[inline(always)]
    pub fn update_minor_piece_history(
        &mut self,
        color: Color,
        from: Square,
        to: Square,
        depth: u8,
    ) {
        let bonus = (depth as i32) * (depth as i32);
        Self::gravity_update(
            &mut self.minor_piece_history[color as usize][from.0 as usize][to.0 as usize],
            bonus,
        );
    }

    #[inline(always)]
    pub fn get_minor_piece_history(&self, color: Color, from: Square, to: Square) -> i32 {
        self.minor_piece_history[color as usize][from.0 as usize][to.0 as usize]
    }

    #[inline(always)]
    pub fn penalize_minor_piece_history(
        &mut self,
        color: Color,
        from: Square,
        to: Square,
        depth: u8,
    ) {
        let penalty = -((depth as i32) * (depth as i32));
        Self::gravity_update(
            &mut self.minor_piece_history[color as usize][from.0 as usize][to.0 as usize],
            penalty,
        );
    }

    #[inline(always)]
    pub fn update_low_ply_history(&mut self, m: Move, ply: usize, depth: u8) {
        if ply < 16 {
            let bonus = (depth as i32) * (depth as i32);
            Self::gravity_update(
                &mut self.low_ply_history[ply][(m.0 & 0xFFF) as usize],
                bonus,
            );
        }
    }

    #[inline(always)]
    pub fn get_low_ply_history(&self, m: Move, ply: usize) -> i32 {
        if ply < 16 {
            self.low_ply_history[ply][(m.0 & 0xFFF) as usize]
        } else {
            0
        }
    }

    #[inline(always)]
    pub fn penalize_low_ply_history(&mut self, m: Move, ply: usize, depth: u8) {
        if ply < 16 {
            let penalty = -((depth as i32) * (depth as i32));
            Self::gravity_update(
                &mut self.low_ply_history[ply][(m.0 & 0xFFF) as usize],
                penalty,
            );
        }
    }

    /// Check if current eval is improving compared to 2 plies ago
    #[inline(always)]
    pub fn is_improving(&self, ply: usize, eval: i32) -> bool {
        if ply >= 2 {
            eval > self.static_evals[ply - 2]
        } else {
            true
        }
    }

    #[inline(always)]
    pub fn update_non_pawn_correction(
        &mut self,
        color: Color,
        non_pawn_material_hash: usize,
        diff: i32,
        weight: i32,
    ) {
        let entry = &mut self.non_pawn_corr[color as usize][non_pawn_material_hash % 16384];
        *entry += (diff - *entry) * weight / 256;
    }

    #[inline(always)]
    pub fn get_non_pawn_correction(&self, color: Color, non_pawn_material_hash: usize) -> i32 {
        self.non_pawn_corr[color as usize][non_pawn_material_hash % 16384]
    }

    #[inline(always)]
    pub fn update_cont_correction(
        &mut self,
        prev_pt: PieceType,
        prev_to: Square,
        cur_pt: PieceType,
        cur_to: Square,
        diff: i32,
        weight: i32,
    ) {
        let entry = &mut self.cont_corr[prev_pt as usize][prev_to.0 as usize][cur_pt as usize]
            [cur_to.0 as usize];
        *entry += (diff - *entry) * weight / 256;
    }

    #[inline(always)]
    pub fn get_cont_correction(
        &self,
        prev_pt: PieceType,
        prev_to: Square,
        cur_pt: PieceType,
        cur_to: Square,
    ) -> i32 {
        self.cont_corr[prev_pt as usize][prev_to.0 as usize][cur_pt as usize][cur_to.0 as usize]
    }
}
