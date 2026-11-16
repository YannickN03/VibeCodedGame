//! The Vibe / Flow engine: a 0–100 meter rewarding fast, clean play.
//!
//! Flow only ever changes the **score multiplier** and presentation tempo.
//! It must never influence gravity (see `GameState::gravity_cells_per_second`).

/// Upper bound of the meter.
pub const FLOW_MAX: f32 = 100.0;
/// Bonus for locking a piece quickly after it spawned.
pub const FAST_LOCK_BONUS: f32 = 5.0;
/// "Quick lock" threshold in seconds since spawn.
pub const FAST_LOCK_WINDOW: f32 = 1.2;
/// Line clear bonuses (single, double, triple, quad).
pub const LINE_BONUS: [f32; 4] = [10.0, 16.0, 22.0, 30.0];
/// Bonus for any T-spin.
pub const TSPIN_BONUS: f32 = 25.0;
/// Bonus per combo step.
pub const COMBO_BONUS: f32 = 6.0;
/// Duration of the overdrive shield after maxing the meter.
pub const OVERDRIVE_SHIELD_SECS: f32 = 4.0;
/// Idle time before the meter starts to drain.
pub const IDLE_GRACE_SECS: f32 = 1.5;
/// Drain speed in flow units per second.
pub const DECAY_PER_SEC: f32 = 3.5;

/// The four flow tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlowTier {
    Chill,
    Groove,
    Sync,
    Overdrive,
}

impl FlowTier {
    pub const ALL: [FlowTier; 4] = [
        FlowTier::Chill,
        FlowTier::Groove,
        FlowTier::Sync,
        FlowTier::Overdrive,
    ];

    pub fn from_flow(flow: f32) -> FlowTier {
        if flow >= 90.0 {
            FlowTier::Overdrive
        } else if flow >= 60.0 {
            FlowTier::Sync
        } else if flow >= 25.0 {
            FlowTier::Groove
        } else {
            FlowTier::Chill
        }
    }

    /// Score multiplier.
    pub fn multiplier(self) -> f32 {
        match self {
            FlowTier::Chill => 1.0,
            FlowTier::Groove => 1.25,
            FlowTier::Sync => 1.5,
            FlowTier::Overdrive => 2.0,
        }
    }

    /// Tempo of the background shader pulse.
    pub fn bpm(self) -> f32 {
        match self {
            FlowTier::Chill => 110.0,
            FlowTier::Groove => 116.0,
            FlowTier::Sync => 124.0,
            FlowTier::Overdrive => 132.0,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            FlowTier::Chill => "CHILL",
            FlowTier::Groove => "GROOVE",
            FlowTier::Sync => "SYNC",
            FlowTier::Overdrive => "OVERDRIVE",
        }
    }
}

/// What happened on a lock, as far as flow is concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LockFlowInput {
    pub seconds_since_spawn: f32,
    pub lines: u32,
    pub tspin: bool,
    /// Current combo count (0 = first clear of a chain).
    pub combo: u32,
}

/// The reusable flow engine shared by every game of the arcade.
pub type FlowEngine = FlowMeter;

/// Flow meter state.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowMeter {
    pub flow: f32,
    pub idle_timer: f32,
    pub overdrive_shield: f32,
}

impl Default for FlowMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowMeter {
    pub fn new() -> Self {
        FlowMeter {
            flow: 0.0,
            idle_timer: 0.0,
            overdrive_shield: 0.0,
        }
    }

    pub fn tier(&self) -> FlowTier {
        FlowTier::from_flow(self.flow)
    }

    pub fn multiplier(&self) -> f32 {
        self.tier().multiplier()
    }

    /// Adds flow, clamping to the max and (re)arming the shield at 100.
    pub fn add(&mut self, amount: f32) {
        self.flow = (self.flow + amount).clamp(0.0, FLOW_MAX);
        if self.flow >= FLOW_MAX {
            self.overdrive_shield = OVERDRIVE_SHIELD_SECS;
        }
    }

    /// Registers a generic scoring event of any game (an apple eaten, a
    /// combo...): adds flow and restarts the idle drain timer.
    pub fn register_event(&mut self, amount: f32) {
        self.idle_timer = 0.0;
        self.add(amount);
    }

    /// Registers a piece lock. Returns the amount gained.
    pub fn on_lock(&mut self, input: LockFlowInput) -> f32 {
        let mut gain = 0.0;
        if input.seconds_since_spawn < FAST_LOCK_WINDOW {
            gain += FAST_LOCK_BONUS;
        }
        if input.lines > 0 {
            gain += LINE_BONUS[(input.lines.min(4) - 1) as usize];
        }
        if input.tspin {
            gain += TSPIN_BONUS;
        }
        if input.lines > 0 && input.combo > 0 {
            gain += COMBO_BONUS * input.combo as f32;
        }
        self.idle_timer = 0.0;
        self.add(gain);
        gain
    }

    /// Advances timers; drains after the idle grace period unless shielded.
    pub fn update(&mut self, dt: f32) {
        self.idle_timer += dt;
        if self.overdrive_shield > 0.0 {
            self.overdrive_shield = (self.overdrive_shield - dt).max(0.0);
        } else if self.idle_timer > IDLE_GRACE_SECS {
            self.flow = (self.flow - DECAY_PER_SEC * dt).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_and_multipliers() {
        assert_eq!(FlowTier::from_flow(0.0), FlowTier::Chill);
        assert_eq!(FlowTier::from_flow(24.99), FlowTier::Chill);
        assert_eq!(FlowTier::from_flow(25.0), FlowTier::Groove);
        assert_eq!(FlowTier::from_flow(60.0), FlowTier::Sync);
        assert_eq!(FlowTier::from_flow(89.9), FlowTier::Sync);
        assert_eq!(FlowTier::from_flow(90.0), FlowTier::Overdrive);
        assert_eq!(FlowTier::from_flow(100.0), FlowTier::Overdrive);
        let m: Vec<f32> = FlowTier::ALL.iter().map(|t| t.multiplier()).collect();
        assert_eq!(m, vec![1.0, 1.25, 1.5, 2.0]);
        let b: Vec<f32> = FlowTier::ALL.iter().map(|t| t.bpm()).collect();
        assert_eq!(b, vec![110.0, 116.0, 124.0, 132.0]);
    }

    #[test]
    fn gains_follow_spec() {
        let mut f = FlowMeter::new();
        let g = f.on_lock(LockFlowInput {
            seconds_since_spawn: 0.5,
            lines: 4,
            tspin: false,
            combo: 2,
        });
        assert_eq!(g, 5.0 + 30.0 + 12.0);
        let mut f = FlowMeter::new();
        let g = f.on_lock(LockFlowInput {
            seconds_since_spawn: 3.0,
            lines: 2,
            tspin: true,
            combo: 0,
        });
        assert_eq!(g, 16.0 + 25.0);
    }

    #[test]
    fn decays_after_idle_grace() {
        let mut f = FlowMeter::new();
        f.add(50.0);
        f.update(1.4);
        assert_eq!(f.flow, 50.0);
        f.update(0.2); // idle now 1.6 > 1.5
        assert!((f.flow - (50.0 - 0.7)).abs() < 1e-4);
    }

    #[test]
    fn overdrive_shield_blocks_decay_for_four_seconds() {
        let mut f = FlowMeter::new();
        f.add(150.0);
        assert_eq!(f.flow, 100.0);
        assert_eq!(f.overdrive_shield, 4.0);
        for _ in 0..39 {
            f.update(0.1);
        }
        assert_eq!(f.flow, 100.0, "shield still active");
        for _ in 0..11 {
            f.update(0.1);
        }
        assert!(f.flow < 100.0, "drains after shield expired");
    }

    #[test]
    fn never_negative() {
        let mut f = FlowMeter::new();
        f.add(1.0);
        for _ in 0..100 {
            f.update(1.0);
        }
        assert_eq!(f.flow, 0.0);
    }
}
