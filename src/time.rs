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
    /// Stockfish time management port
    pub fn init(&mut self, time_left: u128, increment: u128, moves_to_go: u32, ply: usize) {
        self.start_time = Instant::now();
        
        if time_left == 0 {
            return;
        }

        let move_overhead = 30; // 30ms

        let mut centi_mtg = if moves_to_go > 0 {
            (moves_to_go as u128 * 100).min(5000)
        } else {
            5051
        };

        if time_left < 1000 {
            centi_mtg = (time_left as f64 * 5.051) as u128;
        }

        let time_left_safe = 1.max(
            time_left as i128
                + (increment as i128 * (centi_mtg as i128 - 100) - move_overhead as i128 * (200 + centi_mtg as i128)) / 100
        ) as u128;

        let opt_scale: f64;
        let max_scale: f64;

        if moves_to_go == 0 {
            let log_time_in_sec = (time_left as f64 / 1000.0).log10();
            let opt_constant = (0.0032116 + 0.000321123 * log_time_in_sec).min(0.00508017);
            let max_constant = (3.3977 + 3.03950 * log_time_in_sec).max(2.94761);

            opt_scale = (0.0121431 + (ply as f64 + 2.94693).powf(0.461073) * opt_constant)
                .min(0.213035 * time_left as f64 / time_left_safe as f64);
            
            // originalTimeAdjust = 1.0 logic skipped (we assume 1.0 adjust since we don't carry state across games easily)

            max_scale = 6.67704_f64.min(max_constant + ply as f64 / 11.9847);
        } else {
            opt_scale = ((0.88 + ply as f64 / 116.4) / (centi_mtg as f64 / 100.0))
                .min(0.88 * time_left as f64 / time_left_safe as f64);
            max_scale = 1.3 + 0.11 * (centi_mtg as f64 / 100.0);
        }

        self.opt_time = (opt_scale * time_left_safe as f64) as u128;
        
        let max_time_possible = (0.825179 * time_left as f64 - move_overhead as f64).max(1.0);
        self.max_time = (max_time_possible).min(max_scale * self.opt_time as f64) as u128;
        self.max_time = self.max_time.saturating_sub(10); // -10 ms buffer from SF

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

    pub fn reset_to_base(&mut self) {
        self.opt_time = self.base_opt_time;
    }

    // Iterative Deepening Integration (Stop checks)
    pub fn should_stop(&self) -> bool {
        self.elapsed() >= self.max_time
    }

    // PV Stability Detection
    pub fn update_pv(&mut self, pv_move: u16) {
        if self.last_pv_move != 0 && self.last_pv_move != pv_move {
            self.opt_time = (self.opt_time as f64 * 1.5) as u128; // 1.5x on PV fail
            self.opt_time = self.opt_time.min(self.max_time);
        }
        self.last_pv_move = pv_move;
    }

    // Score Drop Panic Time
    pub fn update_score(&mut self, score: i32) {
        if self.last_score != -30000 {
            if self.last_score - score > 100 {
                self.opt_time = (self.opt_time as f64 * 2.0) as u128; // 2x on score drop
                self.opt_time = self.opt_time.min(self.max_time);
            }
        }
        self.last_score = score;
    }

    // Aspiration Window Fail Handling
    pub fn aspiration_fail(&mut self, fail_low: bool) {
        if fail_low {
            self.opt_time = (self.opt_time as f64 * 1.5) as u128;
        } else {
            self.opt_time = (self.opt_time as f64 * 1.3) as u128;
        }
        self.opt_time = self.opt_time.min(self.max_time);
    }

    // Move Importance Scaling
    pub fn move_importance_high(&mut self) {
        self.opt_time = (self.opt_time as f64 * 1.3) as u128;
        self.opt_time = self.opt_time.min(self.max_time);
    }

    // Node Speed Prediction
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
