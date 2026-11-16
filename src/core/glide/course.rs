//! The pipe course and the gust schedule.
//!
//! Both are pure functions of the seed (separate RNG streams), advanced only
//! by "the next pipe" / "time", never by what the player does. So a `Daily
//! Vibe` is the same course for everybody.

use crate::core::rng::SplitMix64;

pub const PIPE_W: f32 = 76.0;
pub const LIP_OVERHANG: f32 = 8.0;
pub const LIP_H: f32 = 26.0;
pub const PIPE_SPACING: f32 = 310.0;
pub const GAP_BASE: f32 = 168.0;
pub const GAP_MIN: f32 = 138.0;
pub const GAP_Y_MIN: f32 = 135.0;
pub const GAP_Y_MAX: f32 = 475.0;
pub const MAX_DELTA_Y: f32 = 195.0;
pub const RING_R: f32 = 24.0;
/// Vertical reach of a Vibe ring (it is a tall ellipse).
pub const RING_HALF_H: f32 = 30.0;
/// Pipes from this index on may move in Kinetic Shift.
pub const KINETIC_FROM: u32 = 5;
pub const MAX_AMPLITUDE: f32 = 50.0;

const PIPE_SALT: u64 = 0x0611_DE00_01C0_DE01;
const GUST_SALT: u64 = 0x0611_DE00_01C0_DE02;

/// Sinusoidal vertical motion of a pipe's gap (Kinetic Shift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub amp: f32,
    pub omega: f32,
    pub phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipeSpec {
    pub index: u32,
    /// Centre of the gap (before any motion).
    pub gap_y: f32,
    pub gap_h: f32,
    /// Vertical offset of the Vibe ring from the gap centre (every 2nd pipe).
    pub ring: Option<f32>,
    pub motion: Option<Motion>,
}

impl PipeSpec {
    /// Gap centre at world time `t` (static unless the pipe has motion and
    /// `kinetic` is on). Always keeps the gap clear of floor and ceiling.
    pub fn gap_center(&self, t: f32, kinetic: bool, gap_h: f32) -> f32 {
        let mut y = self.gap_y;
        if let (true, Some(m)) = (kinetic, self.motion) {
            y += m.amp * (m.omega * t + m.phase).sin();
        }
        let half = gap_h * 0.5;
        y.clamp(20.0 + half + 10.0, 590.0 - half - 10.0)
    }
}

/// Generates the pipes one by one.
#[derive(Clone, Debug)]
pub struct Course {
    rng: SplitMix64,
    made: u32,
    last_gap_y: f32,
    last_amp: f32,
}

impl Course {
    pub fn new(seed: u64) -> Self {
        Course {
            rng: SplitMix64::new(seed ^ PIPE_SALT),
            made: 0,
            last_gap_y: 300.0,
            last_amp: 0.0,
        }
    }

    pub fn made(&self) -> u32 {
        self.made
    }

    /// Gap height shrinks gently from 168 to 138 over the first 30 pipes.
    pub fn gap_height(index: u32) -> f32 {
        (GAP_BASE - index as f32).max(GAP_MIN)
    }

    /// The next pipe of the course. The draw order is fixed, so the base
    /// course (gaps, rings) is identical in every mode.
    pub fn next_pipe(&mut self) -> PipeSpec {
        let index = self.made;
        let gap_h = Self::gap_height(index);
        // 1. Gap position, never more than MAX_DELTA_Y from the previous one.
        let roll = self.rng.next_f32();
        let gap_y = if index == 0 {
            230.0 + 140.0 * roll
        } else {
            // A moving predecessor leaves less room for the next jump, so the
            // effective distance stays within MAX_DELTA_Y at any time.
            let reach = MAX_DELTA_Y - self.last_amp;
            let lo = (self.last_gap_y - reach).max(GAP_Y_MIN);
            let hi = (self.last_gap_y + reach).min(GAP_Y_MAX);
            lo + (hi - lo) * roll
        };
        // 2. Ring (every 2nd gap), slightly off-centre.
        let ring_roll = self.rng.next_f32();
        let ring = (index % 2 == 1).then(|| {
            let reach = gap_h * 0.5 - RING_R - 10.0;
            (ring_roll * 2.0 - 1.0) * reach
        });
        // 3. Motion (always drawn; only used from KINETIC_FROM on).
        let (use_roll, amp_roll, omega_roll, phase_roll) = (
            self.rng.next_f32(),
            self.rng.next_f32(),
            self.rng.next_f32(),
            self.rng.next_f32(),
        );
        let delta = (gap_y - self.last_gap_y).abs();
        // Neighbouring amplitudes must not push the effective distance of
        // two gaps beyond MAX_DELTA_Y.
        let room = (MAX_DELTA_Y - delta - self.last_amp).max(0.0);
        let amp = (20.0 + 35.0 * amp_roll).min(MAX_AMPLITUDE).min(room);
        let motion = (index >= KINETIC_FROM && use_roll < 0.5 && amp >= 12.0).then_some(Motion {
            amp,
            // Gentle: at most 50 * 1.5 = 75 units/s of vertical movement.
            omega: 0.9 + 0.6 * omega_roll,
            phase: phase_roll * std::f32::consts::TAU,
        });
        self.last_amp = motion.map_or(0.0, |m| m.amp);
        self.last_gap_y = gap_y;
        self.made += 1;
        PipeSpec {
            index,
            gap_y,
            gap_h,
            ring,
            motion,
        }
    }
}

/// One wind gust: global vertical acceleration for a short time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gust {
    pub start: f32,
    pub duration: f32,
    /// Acceleration in units/s^2 (negative = up).
    pub lift: f32,
}

/// Lazily generated, seed-only gust schedule.
#[derive(Clone, Debug)]
pub struct GustPlan {
    rng: SplitMix64,
    gusts: Vec<Gust>,
    next_free: f32,
}

impl GustPlan {
    pub fn new(seed: u64) -> Self {
        GustPlan {
            rng: SplitMix64::new(seed ^ GUST_SALT),
            gusts: Vec::new(),
            next_free: 8.0,
        }
    }

    fn extend_to(&mut self, t: f32) {
        while self.next_free <= t + 12.0 {
            let start = self.next_free + 5.0 * self.rng.next_f32();
            let duration = 1.0 + 0.6 * self.rng.next_f32();
            let lift = (450.0 + 200.0 * self.rng.next_f32())
                * if self.rng.next_below(2) == 0 {
                    -1.0
                } else {
                    1.0
                };
            self.gusts.push(Gust {
                start,
                duration,
                lift,
            });
            self.next_free = start + duration + 4.0;
        }
    }

    /// The gust active at world time `t`, if any.
    pub fn at(&mut self, t: f32) -> Option<Gust> {
        self.extend_to(t);
        self.gusts
            .iter()
            .find(|g| t >= g.start && t < g.start + g.duration)
            .copied()
    }

    /// The gust active at `t` among the already generated ones (read-only).
    pub fn peek(&self, t: f32) -> Option<Gust> {
        self.gusts
            .iter()
            .find(|g| t >= g.start && t < g.start + g.duration)
            .copied()
    }

    /// The first gust that ends after `t`.
    pub fn next_after(&mut self, t: f32) -> Option<Gust> {
        self.extend_to(t);
        self.gusts
            .iter()
            .find(|g| g.start + g.duration > t)
            .copied()
    }

    pub fn lift_at(&mut self, t: f32) -> f32 {
        self.at(t).map_or(0.0, |g| g.lift)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_gaps_are_reachable_and_in_range() {
        for seed in 0..60u64 {
            let mut c = Course::new(seed);
            let mut prev: Option<PipeSpec> = None;
            for _ in 0..200 {
                let p = c.next_pipe();
                assert!(
                    (GAP_Y_MIN..=GAP_Y_MAX).contains(&p.gap_y),
                    "range {}",
                    p.gap_y
                );
                if let Some(q) = prev {
                    assert!((p.gap_y - q.gap_y).abs() <= MAX_DELTA_Y + 1e-3, "delta");
                }
                prev = Some(p);
            }
        }
    }

    #[test]
    fn the_bound_also_holds_for_the_moving_gaps_at_any_time() {
        for seed in 0..30u64 {
            let mut c = Course::new(seed);
            let pipes: Vec<PipeSpec> = (0..120).map(|_| c.next_pipe()).collect();
            assert!(pipes.iter().any(|p| p.motion.is_some()), "some pipes move");
            for w in pipes.windows(2) {
                for ta in 0..30 {
                    for tb in 0..30 {
                        let (ta, tb) = (ta as f32 * 0.37, tb as f32 * 0.41);
                        let a = w[0].gap_center(ta, true, w[0].gap_h);
                        let b = w[1].gap_center(tb, true, w[1].gap_h);
                        assert!((a - b).abs() <= MAX_DELTA_Y + 1e-2, "{a} vs {b}");
                    }
                }
            }
        }
    }

    #[test]
    fn dimensions_follow_the_spec() {
        assert_eq!(Course::gap_height(0), 168.0);
        assert_eq!(Course::gap_height(15), 153.0);
        assert_eq!(Course::gap_height(30), 138.0);
        assert_eq!(Course::gap_height(500), 138.0);
        let mut c = Course::new(1);
        let p: Vec<PipeSpec> = (0..8).map(|_| c.next_pipe()).collect();
        assert!(
            p.iter()
                .enumerate()
                .all(|(i, s)| s.ring.is_some() == (i % 2 == 1))
        );
        assert!(
            p.iter().take(5).all(|s| s.motion.is_none()),
            "no motion before pipe 5"
        );
        for s in p.iter().filter_map(|s| s.ring) {
            assert!(
                s.abs() <= 168.0 * 0.5 - RING_R - 10.0 + 1e-3,
                "ring stays inside"
            );
        }
    }

    #[test]
    fn the_base_course_is_a_pure_function_of_the_seed() {
        let make = |seed| {
            let mut c = Course::new(seed);
            (0..50).map(|_| c.next_pipe()).collect::<Vec<_>>()
        };
        assert_eq!(make(20261116), make(20261116));
        assert_ne!(make(20261116), make(20261117));
    }

    #[test]
    fn gusts_are_deterministic_and_spaced() {
        let mut a = GustPlan::new(9);
        let mut b = GustPlan::new(9);
        let mut hits = 0;
        for i in 0..6000 {
            let t = i as f32 * 0.01;
            assert_eq!(a.at(t), b.at(t));
            hits += a.at(t).is_some() as u32;
        }
        assert!(hits > 100, "gusts occur within a minute");
        assert_eq!(GustPlan::new(9).at(0.0), None, "calm start");
        let g = a.gusts.clone();
        assert!(
            g.windows(2)
                .all(|w| w[1].start >= w[0].start + w[0].duration + 4.0)
        );
        assert!(g.iter().all(|g| (450.0..=650.0).contains(&g.lift.abs())));
    }
}
