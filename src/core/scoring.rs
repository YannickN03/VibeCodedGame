//! Guideline scoring with level, back-to-back, combo and the flow multiplier.

/// Base points of a clear before level/B2B/flow multipliers.
pub fn base_points(lines: u32, tspin: bool) -> u32 {
    match (tspin, lines) {
        (false, 0) => 0,
        (false, 1) => 100,
        (false, 2) => 300,
        (false, 3) => 500,
        (false, _) => 800,
        (true, 0) => 400,
        (true, 1) => 800,
        (true, 2) => 1200,
        (true, _) => 1600,
    }
}

/// "Difficult" clears keep (or start) a back-to-back chain.
pub fn is_difficult(lines: u32, tspin: bool) -> bool {
    lines > 0 && (lines >= 4 || tspin)
}

/// Back-to-back multiplier.
pub const B2B_MULTIPLIER: f64 = 1.5;
/// Combo bonus per step (× combo × level).
pub const COMBO_POINTS: u64 = 50;

/// Human-readable callout for a clear.
pub fn clear_label(lines: u32, tspin: bool) -> Option<&'static str> {
    match (tspin, lines) {
        (true, 0) => Some("T-SPIN!"),
        (true, 1) => Some("T-SPIN SINGLE!"),
        (true, 2) => Some("T-SPIN DOUBLE!"),
        (true, _) => Some("T-SPIN TRIPLE!"),
        (false, 0) => None,
        (false, 1) => Some("SINGLE"),
        (false, 2) => Some("DOUBLE"),
        (false, 3) => Some("TRIPLE"),
        (false, _) => Some("QUAD!"),
    }
}

/// Outcome of scoring one lock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearResult {
    pub lines: u32,
    pub tspin: bool,
    pub back_to_back: bool,
    /// Combo count after this lock (`0` = first clear, `-1` = chain broken).
    pub combo: i32,
    pub points: u64,
}

/// Running score state.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreTracker {
    pub score: u64,
    /// `-1` when no chain is running.
    pub combo: i32,
    pub b2b_active: bool,
    /// Number of consecutive B2B bonuses awarded.
    pub b2b_streak: u32,
}

impl Default for ScoreTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoreTracker {
    pub fn new() -> Self {
        ScoreTracker {
            score: 0,
            combo: -1,
            b2b_active: false,
            b2b_streak: 0,
        }
    }

    /// Soft drop: 1 point per cell. Hard drop: 2 points per cell.
    pub fn add_drop(&mut self, cells: u32, hard: bool) -> u64 {
        let pts = cells as u64 * if hard { 2 } else { 1 };
        self.score += pts;
        pts
    }

    /// Scores a lock. `flow_multiplier` comes from the flow tier.
    pub fn register_lock(
        &mut self,
        lines: u32,
        tspin: bool,
        level: u32,
        flow_multiplier: f32,
    ) -> ClearResult {
        let level = level.max(1) as f64;
        let difficult = is_difficult(lines, tspin);
        let back_to_back = difficult && self.b2b_active;

        let mut action = base_points(lines, tspin) as f64 * level;
        if back_to_back {
            action *= B2B_MULTIPLIER;
        }

        let mut combo_pts = 0.0;
        if lines > 0 {
            self.combo += 1;
            if self.combo > 0 {
                combo_pts = (COMBO_POINTS * self.combo as u64) as f64 * level;
            }
            // Normal singles/doubles/triples break B2B; difficult clears keep it.
            self.b2b_active = difficult;
            if back_to_back {
                self.b2b_streak += 1;
            } else if !difficult {
                self.b2b_streak = 0;
            }
        } else {
            self.combo = -1;
        }

        let points = ((action + combo_pts) * flow_multiplier as f64).round() as u64;
        self.score += points;
        ClearResult {
            lines,
            tspin,
            back_to_back,
            combo: self.combo,
            points,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_table() {
        assert_eq!(base_points(1, false), 100);
        assert_eq!(base_points(2, false), 300);
        assert_eq!(base_points(3, false), 500);
        assert_eq!(base_points(4, false), 800);
        assert_eq!(base_points(0, true), 400);
        assert_eq!(base_points(1, true), 800);
        assert_eq!(base_points(2, true), 1200);
        assert_eq!(base_points(3, true), 1600);
    }

    #[test]
    fn level_and_flow_multiply() {
        let mut s = ScoreTracker::new();
        let r = s.register_lock(4, false, 3, 2.0);
        assert_eq!(r.points, 800 * 3 * 2);
    }

    #[test]
    fn back_to_back_quads() {
        let mut s = ScoreTracker::new();
        s.register_lock(4, false, 1, 1.0);
        s.register_lock(0, false, 1, 1.0); // no clear: B2B survives, combo breaks
        let r = s.register_lock(4, false, 1, 1.0);
        assert!(r.back_to_back);
        assert_eq!(r.points, 1200);
        assert_eq!(s.b2b_streak, 1);
    }

    #[test]
    fn single_breaks_back_to_back() {
        let mut s = ScoreTracker::new();
        s.register_lock(2, true, 1, 1.0);
        s.register_lock(0, false, 1, 1.0);
        s.register_lock(1, false, 1, 1.0);
        s.register_lock(0, false, 1, 1.0);
        let r = s.register_lock(4, false, 1, 1.0);
        assert!(!r.back_to_back);
        assert_eq!(r.points, 800);
    }

    #[test]
    fn combo_bonus() {
        let mut s = ScoreTracker::new();
        let a = s.register_lock(1, false, 2, 1.0);
        assert_eq!(a.combo, 0);
        assert_eq!(a.points, 200);
        let b = s.register_lock(1, false, 2, 1.0);
        assert_eq!(b.combo, 1);
        assert_eq!(b.points, 200 + 50 * 2);
        let c = s.register_lock(0, false, 2, 1.0);
        assert_eq!(c.combo, -1);
    }

    #[test]
    fn drop_points() {
        let mut s = ScoreTracker::new();
        s.add_drop(10, true);
        s.add_drop(5, false);
        assert_eq!(s.score, 25);
    }
}
