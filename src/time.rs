use std::time::Instant;

#[derive(Clone)]
pub struct TimeManager {
    start_time: Instant,
    pub opt_time: u128,
    pub max_time: u128,
    pub last_score: i32,
    pub last_pv_move: u16,
    base_opt_time: u128,
}

impl Default for TimeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeManager {
    pub fn new() -> Self {
        TimeManager {
            start_time: Instant::now(),
            opt_time: u128::MAX,
            max_time: u128::MAX,
            last_score: -30000,
            last_pv_move: 0,
            base_opt_time: u128::MAX,
        }
    }

    /// Primary initialization for a standard game turn.
    pub fn init(&mut self, time_left: u128, increment: u128, moves_played: usize) {
        self.start_time = Instant::now();
        
        // 1. Base Time Allocation
        // base_time = remaining_time / 20
        let base_time = time_left / 20;
        let time_pool = base_time + (increment / 2);

        self.opt_time = (time_pool as f64 * 0.6) as u128;
        self.max_time = (time_pool as f64 * 3.0) as u128;

        // Ensure we never exceed actual safe limits
        self.max_time = self.max_time.min(time_left.saturating_sub(50).max(10));
        self.opt_time = self.opt_time.min(self.max_time);

        // 7. Opening Time Saving
        if moves_played < 15 {
            self.opt_time = (self.opt_time as f64 * 0.7) as u128;
        }

        // 8. Endgame Safety (time < 10 seconds = 10,000 ms)
        if time_left < 10000 {
            self.opt_time = time_left / 8;
            self.max_time = time_left / 4;
        }

        self.base_opt_time = self.opt_time;
        self.last_score = -30000;
        self.last_pv_move = 0;
    }

    /// Hard limit override (e.g., specific 'movetime' or 'go time' commands)
    pub fn set_exact_limits(&mut self, soft_limit: u128, hard_limit: u128) {
        self.start_time = Instant::now();
        self.opt_time = soft_limit;
        self.max_time = hard_limit;
        self.base_opt_time = soft_limit;
        self.last_score = -30000;
        self.last_pv_move = 0;
    }

    pub fn elapsed(&self) -> u128 {
        self.start_time.elapsed().as_millis()
    }

    // 2. Iterative Deepening Integration (Stop checks)
    pub fn should_stop(&self) -> bool {
        self.elapsed() >= self.max_time
    }

    // 3. PV Stability Detection
    pub fn update_pv(&mut self, pv_move: u16) {
        if self.last_pv_move != 0 && self.last_pv_move != pv_move {
            self.opt_time = (self.opt_time * 15) / 10;
        }
        self.last_pv_move = pv_move;
    }

    // 4. Score Drop Panic Time
    pub fn update_score(&mut self, score: i32) {
        if self.last_score != -30000 {
            if self.last_score - score > 100 {
                self.opt_time = self.opt_time.saturating_mul(2);
            }
        }
        self.last_score = score;
    }

    // 5. Aspiration Window Fail Handling
    pub fn aspiration_fail(&mut self, fail_low: bool) {
        if fail_low {
            self.opt_time = (self.opt_time * 15) / 10;
        } else {
            self.opt_time = (self.opt_time * 13) / 10;
        }
    }

    // 6. Move Importance Scaling
    pub fn move_importance_high(&mut self) {
        self.opt_time = (self.opt_time * 13) / 10;
    }

    // 9. Node Speed Prediction
    pub fn can_start_next_iteration(&mut self, nodes_this_iter: u64, nps: u64) -> bool {
        if self.elapsed() >= self.opt_time {
            return false;
        }

        if nps > 0 {
            // Assume the next iteration takes roughly 2.0x nodes
            let predicted_nodes = nodes_this_iter.saturating_mul(2);
            let predicted_time_needed_ms = (predicted_nodes as u128 * 1000) / (nps as u128);
            
            if self.elapsed() + predicted_time_needed_ms > self.opt_time {
                return false;
            }
        }

        true
    }
}
