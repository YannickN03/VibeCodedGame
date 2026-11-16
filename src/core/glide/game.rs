//! `GlideGame`: the complete, deterministic rules of VibeCoded Glide.
//!
//! Physics runs in fixed 1/120 s steps, so jump height and flight path are
//! identical on 60 Hz and 144 Hz monitors. The glider stays at a fixed x
//! while the pipes scroll towards it. No rendering, no `macroquad`.

use crate::core::flow::{FlowEngine, FlowTier};

use super::course::{
    Course, GustPlan, LIP_H, LIP_OVERHANG, PIPE_SPACING, PIPE_W, PipeSpec, RING_HALF_H,
};
use super::geom::{Aabb, circle_vs_aabb};

pub const ARENA_W: f32 = 960.0;
pub const ARENA_H: f32 = 640.0;
pub const FLOOR_Y: f32 = 590.0;
pub const CEIL_Y: f32 = 20.0;
pub const FIXED_DT: f32 = 1.0 / 120.0;
pub const MAX_FRAME_DT: f32 = 0.05;

pub const GLIDER_X: f32 = 260.0;
pub const GLIDER_R: f32 = 15.0;
pub const GRAVITY: f32 = 1500.0;
pub const DIVE_FACTOR: f32 = 1.65;
pub const FLAP_VY: f32 = -430.0;
pub const MAX_FALL: f32 = 760.0;
pub const INPUT_BUFFER: f32 = 0.08;

pub const DASH_IFRAMES: f32 = 0.22;
pub const DASH_COOLDOWN: f32 = 2.5;
/// While dashing the world scrolls this much faster: without it 0.22 s of
/// i-frames could not carry the glider through a 122-unit pipe.
pub const DASH_BOOST: f32 = 2.6;

pub const SPEED_MIN: f32 = 220.0;
pub const SPEED_MAX: f32 = 340.0;
pub const FIRST_PIPE_X: f32 = GLIDER_X + 520.0;
/// Extra gap height while in Overdrive.
pub const OVERDRIVE_WIDEN: f32 = 16.0;
/// Pipes closer than this (ahead of the glider) never change their gap.
pub const WIDEN_FREEZE_DISTANCE: f32 = 300.0;
pub const SKIM_DISTANCE: f32 = 10.0;

pub const PASS_SCORE: u64 = 100;
pub const RING_SCORE: u64 = 150;
pub const SKIM_SCORE: u64 = 50;
pub const PASS_FLOW: f32 = 3.0;
pub const RING_FLOW: f32 = 15.0;
pub const SKIM_FLOW: f32 = 5.0;

/// Horizontal scroll speed for a number of passed pipes: 220 -> 340, monotonic.
pub fn base_speed(passed: u32) -> f32 {
    SPEED_MIN + (SPEED_MAX - SPEED_MIN) * (1.0 - (-(passed as f32) / 25.0).exp())
}

/// Nose angle in radians (positive = nose down) for a vertical velocity.
pub fn pitch_angle(vy: f32) -> f32 {
    (vy / 600.0).clamp(-0.7, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlideMode {
    Classic,
    KineticShift,
    DailyVibe,
    Zen,
}

impl GlideMode {
    pub const ALL: [GlideMode; 4] = [
        GlideMode::Classic,
        GlideMode::KineticShift,
        GlideMode::DailyVibe,
        GlideMode::Zen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            GlideMode::Classic => "classic",
            GlideMode::KineticShift => "kinetic",
            GlideMode::DailyVibe => "daily",
            GlideMode::Zen => "zen",
        }
    }

    pub fn from_id(id: &str) -> GlideMode {
        GlideMode::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(GlideMode::Classic)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Pipe,
    Ground,
    Ceiling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Dead(Cause),
}

/// One frame (or one step) of input.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlideInput {
    /// Rising edge of the flap button.
    pub flap_pressed: bool,
    /// Dive / fast-drop key held.
    pub dive_held: bool,
    /// Rising edge of the dash button.
    pub dash_pressed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pipe {
    pub spec: PipeSpec,
    /// Left edge of the pipe body (lips overhang by 8 on each side).
    pub x: f32,
    /// Extra gap height from Overdrive.
    pub widen: f32,
    pub scored: bool,
    pub ring_resolved: bool,
    pub touched: bool,
    pub skim_paid: bool,
    skim_min: f32,
}

impl Pipe {
    pub fn new(spec: PipeSpec, x: f32, widen: f32) -> Self {
        Pipe {
            spec,
            x,
            widen,
            scored: false,
            ring_resolved: false,
            touched: false,
            skim_paid: false,
            skim_min: f32::INFINITY,
        }
    }

    pub fn gap_h(&self) -> f32 {
        self.spec.gap_h + self.widen
    }

    pub fn center_x(&self) -> f32 {
        self.x + PIPE_W * 0.5
    }

    pub fn gap_center(&self, t: f32, kinetic: bool) -> f32 {
        self.spec.gap_center(t, kinetic, self.gap_h())
    }

    /// Absolute y of the ring (if any) at world time `t`.
    pub fn ring_y(&self, t: f32, kinetic: bool) -> Option<f32> {
        self.spec.ring.map(|o| self.gap_center(t, kinetic) + o)
    }

    /// `[top body, top lip, bottom lip, bottom body]`.
    pub fn rects(&self, t: f32, kinetic: bool) -> [Aabb; 4] {
        let c = self.gap_center(t, kinetic);
        let half = self.gap_h() * 0.5;
        let (top, bottom) = (c - half, c + half);
        let w = PIPE_W + 2.0 * LIP_OVERHANG;
        [
            Aabb::new(self.x, 0.0, PIPE_W, (top - LIP_H).max(0.0)),
            Aabb::new(self.x - LIP_OVERHANG, top - LIP_H, w, LIP_H),
            Aabb::new(self.x - LIP_OVERHANG, bottom, w, LIP_H),
            Aabb::new(self.x, bottom + LIP_H, PIPE_W, ARENA_H - bottom - LIP_H),
        ]
    }

    fn lips_x(&self) -> (f32, f32) {
        (self.x - LIP_OVERHANG, self.x + PIPE_W + LIP_OVERHANG)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GlideEvent {
    Started,
    Flap,
    Dash,
    Passed {
        y: f32,
        points: u64,
    },
    Ring {
        y: f32,
        points: u64,
        perfect: bool,
        combo: u32,
    },
    RingMissed,
    Skim {
        y: f32,
        points: u64,
    },
    Bounce(Cause),
    GustStart {
        lift: f32,
    },
    Died(Cause),
}

#[derive(Clone, Debug)]
pub struct GlideGame {
    pub mode: GlideMode,
    pub seed: u64,
    pub status: Status,
    /// `false` while waiting for the first flap.
    pub started: bool,
    pub y: f32,
    pub vy: f32,
    pub pipes: Vec<Pipe>,
    pub score: u64,
    pub passed: u32,
    pub rings: u32,
    /// Consecutive rings.
    pub combo: u32,
    pub best_combo: u32,
    pub skims: u32,
    pub bounces: u32,
    pub flow: FlowEngine,
    pub elapsed: f64,
    pub distance: f32,
    pub steps: u64,
    pub iframes: f32,
    pub dash_cd: f32,
    pub dash_t: f32,
    /// Remaining time of the gust currently blowing (rendering).
    pub gust: Option<f32>,
    course: Course,
    gusts: GustPlan,
    acc: f64,
    flap_buf: f32,
    dash_buf: f32,
    last_gust_start: f32,
    events: Vec<GlideEvent>,
}

impl GlideGame {
    pub fn new(mode: GlideMode, seed: u64) -> Self {
        let mut g = GlideGame {
            mode,
            seed,
            status: Status::Playing,
            started: false,
            y: 300.0,
            vy: 0.0,
            pipes: Vec::new(),
            score: 0,
            passed: 0,
            rings: 0,
            combo: 0,
            best_combo: 0,
            skims: 0,
            bounces: 0,
            flow: FlowEngine::new(),
            elapsed: 0.0,
            distance: 0.0,
            steps: 0,
            iframes: 0.0,
            dash_cd: 0.0,
            dash_t: 0.0,
            gust: None,
            course: Course::new(seed),
            gusts: GustPlan::new(seed),
            acc: 0.0,
            flap_buf: 0.0,
            dash_buf: 0.0,
            last_gust_start: -1.0,
            events: Vec::new(),
        };
        g.fill_pipes();
        g
    }

    // ------------------------------------------------------------ queries

    pub fn is_playing(&self) -> bool {
        self.status == Status::Playing
    }

    pub fn tier(&self) -> FlowTier {
        self.flow.tier()
    }

    pub fn overdrive(&self) -> bool {
        self.tier() == FlowTier::Overdrive
    }

    pub fn dash_ready(&self) -> bool {
        self.dash_cd <= 0.0
    }

    pub fn speed(&self) -> f32 {
        base_speed(self.passed)
    }

    pub fn kinetic(&self) -> bool {
        self.mode == GlideMode::KineticShift
    }

    /// Vertical wind acceleration right now (negative = up); `0` outside
    /// Kinetic Shift or before the first gust.
    pub fn lift_now(&self) -> f32 {
        if self.kinetic() && self.passed >= 5 {
            self.gusts.peek(self.world_time()).map_or(0.0, |g| g.lift)
        } else {
            0.0
        }
    }

    pub fn world_time(&self) -> f32 {
        self.elapsed as f32
    }

    /// Developer aid: jump to the middle of the first wind gust (Kinetic).
    pub fn skip_to_gust(&mut self) {
        let t = self.world_time();
        if let Some(g) = self.gusts.next_after(t) {
            self.elapsed = (g.start + 0.3) as f64;
        }
    }

    #[cfg(test)]
    pub(crate) fn push_test_event(&mut self, e: GlideEvent) {
        self.events.push(e);
    }

    pub fn drain_events(&mut self) -> Vec<GlideEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn pitch(&self) -> f32 {
        pitch_angle(self.vy)
    }

    // ------------------------------------------------------------ main loop

    /// Advances by a frame of `dt` seconds (capped at 0.05 s). Presses are
    /// buffered for 80 ms, so a tap between two steps is never lost.
    pub fn update(&mut self, dt: f32, input: &GlideInput) {
        if !self.is_playing() || !dt.is_finite() {
            return;
        }
        self.buffer(input);
        self.acc += dt.clamp(0.0, MAX_FRAME_DT) as f64;
        // 1 microsecond of tolerance: a frame boundary that coincides with a
        // step boundary must not lose a step to f32 rounding.
        while self.acc + 1e-6 >= FIXED_DT as f64 && self.is_playing() {
            self.acc -= FIXED_DT as f64;
            self.step_inner(input.dive_held);
        }
    }

    fn buffer(&mut self, input: &GlideInput) {
        if input.flap_pressed {
            self.flap_buf = INPUT_BUFFER;
        }
        if input.dash_pressed {
            self.dash_buf = INPUT_BUFFER;
        }
    }

    /// One fixed step with explicit inputs (tests, bots).
    pub fn step(&mut self, input: &GlideInput) {
        self.buffer(input);
        self.step_inner(input.dive_held);
    }

    fn award(&mut self, base: u64) -> u64 {
        let pts = (base as f32 * self.flow.multiplier()).round() as u64;
        self.score += pts;
        pts
    }

    fn fill_pipes(&mut self) {
        loop {
            let x = match self.pipes.last() {
                None => FIRST_PIPE_X,
                Some(p) if p.x < ARENA_W + 80.0 => p.x + PIPE_SPACING,
                Some(_) => break,
            };
            let spec = self.course.next_pipe();
            let widen = if self.overdrive() {
                OVERDRIVE_WIDEN
            } else {
                0.0
            };
            self.pipes.push(Pipe::new(spec, x, widen));
        }
    }

    fn step_inner(&mut self, dive: bool) {
        if !self.is_playing() {
            return;
        }
        self.steps += 1;
        let dt = FIXED_DT;
        if !self.started {
            if self.flap_buf <= 0.0 {
                self.end_timers(dt);
                return;
            }
            self.started = true;
            self.events.push(GlideEvent::Started);
        }
        self.elapsed += dt as f64;
        let t = self.world_time();

        if self.flap_buf > 0.0 {
            // Deterministic impulse: the velocity is SET, never added to.
            self.vy = FLAP_VY;
            self.flap_buf = 0.0;
            self.events.push(GlideEvent::Flap);
        }
        if self.dash_buf > 0.0 && self.dash_cd <= 0.0 {
            self.dash_buf = 0.0;
            self.dash_t = DASH_IFRAMES;
            self.iframes = self.iframes.max(DASH_IFRAMES);
            self.dash_cd = DASH_COOLDOWN;
            self.events.push(GlideEvent::Dash);
        }

        // Gravity (+65 % while diving) and wind gusts.
        let mut accel = GRAVITY * if dive { DIVE_FACTOR } else { 1.0 };
        if self.kinetic() && self.passed >= 5 {
            let gust = self.gusts.at(t);
            accel += gust.map_or(0.0, |g| g.lift);
            self.gust = gust.map(|g| g.start + g.duration - t);
            if let Some(g) = gust
                && g.start != self.last_gust_start
            {
                self.last_gust_start = g.start;
                self.events.push(GlideEvent::GustStart { lift: g.lift });
            }
        }
        self.vy = (self.vy + accel * dt).clamp(-900.0, MAX_FALL);
        self.y += self.vy * dt;

        // The world scrolls (faster while dashing).
        let mult = if self.dash_t > 0.0 { DASH_BOOST } else { 1.0 };
        let dx = self.speed() * mult * dt;
        for p in &mut self.pipes {
            p.x -= dx;
        }
        self.distance += dx;
        self.pipes.retain(|p| p.x + PIPE_W + LIP_OVERHANG > -40.0);
        self.fill_pipes();
        self.update_widen(dt);

        self.resolve_pipes(t);
        self.check_collisions(t);
        self.flow.update(dt);
        self.end_timers(dt);
    }

    fn end_timers(&mut self, dt: f32) {
        self.flap_buf = (self.flap_buf - dt).max(0.0);
        self.dash_buf = (self.dash_buf - dt).max(0.0);
        self.iframes = (self.iframes - dt).max(0.0);
        self.dash_cd = (self.dash_cd - dt).max(0.0);
        self.dash_t = (self.dash_t - dt).max(0.0);
    }

    /// Overdrive widens the gaps of pipes still far ahead. Near pipes keep
    /// their gap, so it never narrows under the player's feet.
    fn update_widen(&mut self, dt: f32) {
        let target = if self.overdrive() {
            OVERDRIVE_WIDEN
        } else {
            0.0
        };
        for p in &mut self.pipes {
            if p.x > GLIDER_X + WIDEN_FREEZE_DISTANCE {
                let step = 40.0 * dt;
                p.widen += (target - p.widen).clamp(-step, step);
            }
        }
    }

    // ------------------------------------------------------------ scoring

    fn resolve_pipes(&mut self, t: f32) {
        let kinetic = self.kinetic();
        let (gx, gy, r) = (GLIDER_X, self.y, GLIDER_R);
        for i in 0..self.pipes.len() {
            let (lip_l, lip_r) = self.pipes[i].lips_x();
            let in_range = gx + r > lip_l && gx - r < lip_r;
            if in_range {
                let rects = self.pipes[i].rects(t, kinetic);
                for rect in &rects {
                    if circle_vs_aabb(gx, gy, r, rect) {
                        self.pipes[i].touched = true;
                    }
                }
                // Clearance to the lips (the closest parts of the pipe).
                let clearance = rects[1]
                    .distance_to(gx, gy)
                    .min(rects[2].distance_to(gx, gy))
                    - r;
                let p = &mut self.pipes[i];
                p.skim_min = p.skim_min.min(clearance);
            }
            // Passage: the glider crosses the pipe's centre line.
            if !self.pipes[i].scored && self.pipes[i].center_x() < gx {
                self.pipes[i].scored = true;
                self.passed += 1;
                let pts = self.award(PASS_SCORE);
                self.flow.register_event(PASS_FLOW);
                self.events.push(GlideEvent::Passed { y: gy, points: pts });
                if !self.pipes[i].ring_resolved {
                    self.pipes[i].ring_resolved = true;
                    if let Some(ry) = self.pipes[i].ring_y(t, kinetic) {
                        let dy = (gy - ry).abs();
                        if dy <= RING_HALF_H {
                            self.collect_ring(ry, dy);
                        } else {
                            self.combo = 0;
                            self.events.push(GlideEvent::RingMissed);
                        }
                    }
                }
            }
            // Skim: paid once when the glider has fully passed the lips.
            let p = &self.pipes[i];
            if !p.skim_paid && gx - r >= lip_r && !p.touched && p.skim_min < SKIM_DISTANCE {
                self.pipes[i].skim_paid = true;
                self.skims += 1;
                let pts = self.award_flat(SKIM_SCORE_FLAT);
                self.flow.register_event(SKIM_FLOW);
                self.events.push(GlideEvent::Skim { y: gy, points: pts });
            }
        }
    }

    fn award_flat(&mut self, pts: u64) -> u64 {
        self.score += pts;
        pts
    }

    fn collect_ring(&mut self, ring_y: f32, dy: f32) {
        let pts = self.award(RING_SCORE);
        self.flow.register_event(RING_FLOW);
        self.combo += 1;
        self.best_combo = self.best_combo.max(self.combo);
        self.rings += 1;
        self.events.push(GlideEvent::Ring {
            y: ring_y,
            points: pts,
            perfect: dy <= 10.0,
            combo: self.combo,
        });
    }

    // ------------------------------------------------------------ collisions

    fn check_collisions(&mut self, t: f32) {
        let kinetic = self.kinetic();
        let (gx, r) = (GLIDER_X, GLIDER_R);
        let mut hit_pipe = false;
        if self.iframes <= 0.0 {
            for p in &self.pipes {
                let (l, rr) = p.lips_x();
                if gx + r <= l || gx - r >= rr {
                    continue;
                }
                if p.rects(t, kinetic)
                    .iter()
                    .any(|rect| circle_vs_aabb(gx, self.y, r, rect))
                {
                    hit_pipe = true;
                    break;
                }
            }
        }
        // Floor and ceiling stay deadly even during the dash.
        let ground = self.y + r >= FLOOR_Y;
        let ceiling = self.y - r <= CEIL_Y;
        if !(hit_pipe || ground || ceiling) {
            return;
        }
        let cause = if hit_pipe {
            Cause::Pipe
        } else if ground {
            Cause::Ground
        } else {
            Cause::Ceiling
        };
        if self.mode == GlideMode::Zen {
            self.bounce(cause);
        } else {
            self.status = Status::Dead(cause);
            self.events.push(GlideEvent::Died(cause));
        }
    }

    /// Zen: bounce off softly with an EMP flash and fly on. After a pipe hit
    /// the glider is shielded long enough to slip out of the pipe.
    fn bounce(&mut self, cause: Cause) {
        match cause {
            Cause::Pipe => {
                self.vy = -280.0;
                self.iframes = self.iframes.max(0.7);
            }
            Cause::Ground => {
                self.y = FLOOR_Y - GLIDER_R - 0.01;
                self.vy = -360.0;
            }
            Cause::Ceiling => {
                self.y = CEIL_Y + GLIDER_R + 0.01;
                self.vy = 200.0;
            }
        }
        self.combo = 0;
        self.bounces += 1;
        self.events.push(GlideEvent::Bounce(cause));
    }
}

const SKIM_SCORE_FLAT: u64 = SKIM_SCORE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::glide::course::Motion;

    fn idle() -> GlideInput {
        GlideInput::default()
    }

    fn flap() -> GlideInput {
        GlideInput {
            flap_pressed: true,
            ..idle()
        }
    }

    fn started(mode: GlideMode) -> GlideGame {
        let mut g = GlideGame::new(mode, 7);
        g.pipes.clear();
        // One pipe far away keeps the course from spawning new ones.
        g.pipes
            .push(Pipe::new(spec(300.0, 168.0, None), 1.0e7, 0.0));
        g.started = true;
        g
    }

    fn spec(gap_y: f32, gap_h: f32, ring: Option<f32>) -> PipeSpec {
        PipeSpec {
            index: 0,
            gap_y,
            gap_h,
            ring,
            motion: None,
        }
    }

    /// A game with a single pipe at `x` (no new pipes interfere for ~1 s).
    fn with_pipe(mode: GlideMode, s: PipeSpec, x: f32) -> GlideGame {
        let mut g = started(mode);
        g.pipes.push(Pipe::new(s, x, 0.0));
        g.pipes
            .push(Pipe::new(spec(300.0, 168.0, None), x + 2000.0, 0.0)); // far away
        g.y = s.gap_y;
        g
    }

    fn run(g: &mut GlideGame, steps: usize, input: GlideInput) {
        for _ in 0..steps {
            g.step(&input);
        }
    }

    #[test]
    fn flap_sets_the_velocity_instead_of_adding_to_it() {
        let mut g = started(GlideMode::Classic);
        g.vy = 300.0;
        g.step(&flap());
        // vy = -430 then one step of gravity.
        assert!(
            (g.vy - (FLAP_VY + GRAVITY * FIXED_DT)).abs() < 1e-3,
            "{}",
            g.vy
        );
        // Flapping again immediately gives the very same velocity.
        let first = g.vy;
        g.vy = -900.0;
        g.step(&flap());
        assert!((g.vy - first).abs() < 1e-3);
    }

    #[test]
    fn dive_adds_65_percent_gravity() {
        let mut a = started(GlideMode::Classic);
        let mut b = started(GlideMode::Classic);
        a.step(&idle());
        b.step(&GlideInput {
            dive_held: true,
            ..idle()
        });
        assert!((a.vy - GRAVITY * FIXED_DT).abs() < 1e-3);
        assert!((b.vy / a.vy - 1.65).abs() < 1e-3);
    }

    #[test]
    fn physics_is_identical_at_60_and_144_hz() {
        // Frame boundaries every 1/12 s are common to both frame rates.
        fn drive(hz: u32) -> Vec<(u32, u32, u64)> {
            let mut g = started(GlideMode::Zen);
            let mut out = Vec::new();
            let frames_per_block = hz / 12;
            let dt = 1.0 / hz as f32;
            for block in 0..60 {
                for f in 0..frames_per_block {
                    // Flap on the first frame of every 7th block; dive in blocks 20..30.
                    let input = GlideInput {
                        flap_pressed: f == 0 && block % 7 == 0,
                        dive_held: (20..30).contains(&block),
                        dash_pressed: f == 0 && block == 40,
                    };
                    g.update(dt, &input);
                }
                out.push((g.y.to_bits(), g.vy.to_bits(), g.steps));
            }
            out
        }
        let a = drive(60);
        let b = drive(144);
        assert_eq!(a.len(), 60);
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x, y, "block {i}: y/vy/steps differ between 60 and 144 Hz");
        }
        assert_eq!(a[59].2, 600, "5 s = 600 fixed steps");
    }

    #[test]
    fn the_same_per_step_inputs_give_bit_identical_runs() {
        let run_it = || {
            let mut g = started(GlideMode::Zen);
            for i in 0..2000 {
                g.step(&GlideInput {
                    flap_pressed: i % 37 == 0,
                    dive_held: i % 200 > 150,
                    dash_pressed: i % 400 == 5,
                });
            }
            (g.y.to_bits(), g.vy.to_bits(), g.score, g.passed)
        };
        assert_eq!(run_it(), run_it());
    }

    #[test]
    fn a_flap_before_the_run_starts_is_buffered_for_80_ms() {
        let mut g = GlideGame::new(GlideMode::Classic, 3);
        assert!(!g.started);
        // Idle while waiting: nothing moves.
        run(&mut g, 30, idle());
        assert!(!g.started && g.y == 300.0 && g.elapsed == 0.0);
        // A press arrives; the run starts on the very next step.
        g.step(&flap());
        assert!(g.started);
        assert!(g.drain_events().contains(&GlideEvent::Started));
        assert!(g.vy < -400.0);
        // A press 80 ms before nothing else happened is not lost by update():
        let mut h = GlideGame::new(GlideMode::Classic, 3);
        h.update(0.001, &flap()); // shorter than a step: only buffered
        assert!(!h.started);
        h.update(0.02, &idle());
        assert!(h.started, "the buffered tap started the run");
    }

    #[test]
    fn pipes_are_geometrically_exact() {
        let p = Pipe::new(spec(300.0, 168.0, None), 500.0, 0.0);
        let [tb, tl, bl, bb] = p.rects(0.0, false);
        assert_eq!((tb.x, tb.w), (500.0, 76.0));
        assert_eq!((tl.x, tl.w), (492.0, 92.0), "lips overhang by 8");
        assert_eq!(tl.bottom(), 300.0 - 84.0, "top lip ends at the gap");
        assert_eq!(bl.y, 300.0 + 84.0, "bottom lip starts at the gap");
        assert_eq!(bb.bottom(), ARENA_H);
        assert!((bl.y - tl.bottom() - 168.0).abs() < 1e-4, "gap height 168");
    }

    #[test]
    fn passing_a_pipe_scores_100_times_the_flow_multiplier() {
        let mut g = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, None),
            GLIDER_X + 60.0,
        );
        g.vy = 0.0;
        for _ in 0..200 {
            g.y = 300.0;
            g.vy = 0.0;
            g.step(&idle());
            if g.passed > 0 {
                break;
            }
        }
        assert_eq!(g.passed, 1);
        assert_eq!(g.score, 100);
        assert!((g.flow.flow - PASS_FLOW).abs() < 0.1);
        // In Sync (1.5x) a passage is worth 150.
        let mut s = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, None),
            GLIDER_X + 60.0,
        );
        s.flow.flow = 70.0;
        for _ in 0..200 {
            s.y = 300.0;
            s.vy = 0.0;
            s.step(&idle());
            if s.passed > 0 {
                break;
            }
        }
        assert_eq!(s.score, 150);
    }

    fn fly_through(g: &mut GlideGame, y: f32, max_steps: usize) {
        for _ in 0..max_steps {
            g.y = y;
            g.vy = 0.0;
            g.step(&idle());
            if g.passed > 0 || !g.is_playing() {
                break;
            }
        }
    }

    #[test]
    fn a_ring_pays_150_with_flow_and_raises_the_combo() {
        let mut g = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, Some(20.0)),
            GLIDER_X + 60.0,
        );
        fly_through(&mut g, 320.0, 300);
        assert_eq!((g.rings, g.combo), (1, 1));
        assert_eq!(g.score, 100 + 150);
        assert!((g.flow.flow - (PASS_FLOW + RING_FLOW)).abs() < 0.2);
        let ev = g.drain_events();
        assert!(ev.iter().any(|e| matches!(
            e,
            GlideEvent::Ring {
                perfect: true,
                combo: 1,
                ..
            }
        )));
    }

    #[test]
    fn a_missed_ring_resets_the_combo_without_dying() {
        let mut g = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, Some(-40.0)),
            GLIDER_X + 60.0,
        );
        g.combo = 4;
        // Ring at y = 260; fly 60 units away from it.
        fly_through(&mut g, 320.0, 300);
        assert!(g.is_playing(), "missing a ring is harmless");
        assert_eq!((g.combo, g.rings), (0, 0));
        assert_eq!(g.score, 100, "only the passage scored");
        assert!(g.drain_events().contains(&GlideEvent::RingMissed));
    }

    #[test]
    fn a_ring_just_inside_and_just_outside_its_reach() {
        for (dy, collected) in [(29.0, true), (31.0, false)] {
            let mut g = with_pipe(
                GlideMode::Classic,
                spec(300.0, 168.0, Some(0.0)),
                GLIDER_X + 60.0,
            );
            fly_through(&mut g, 300.0 + dy, 300);
            assert_eq!(g.rings == 1, collected, "dy {dy}");
        }
    }

    #[test]
    fn a_skim_pays_once_per_pipe_under_10_units() {
        // Gap 168 centred on 300: lip edge at 216. Glider at 216 + 15 + 5.
        let mut g = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, None),
            GLIDER_X + 60.0,
        );
        for _ in 0..400 {
            g.y = 216.0 + 15.0 + 5.0;
            g.vy = 0.0;
            g.step(&idle());
        }
        assert!(g.is_playing());
        assert_eq!(g.skims, 1, "exactly once");
        assert_eq!(g.score, 100 + 50);
        let ev = g.drain_events();
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, GlideEvent::Skim { .. }))
                .count(),
            1
        );
        // 12 units of clearance is not a skim.
        let mut f = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, None),
            GLIDER_X + 60.0,
        );
        for _ in 0..400 {
            f.y = 216.0 + 15.0 + 12.0;
            f.vy = 0.0;
            f.step(&idle());
        }
        assert_eq!(f.skims, 0);
    }

    #[test]
    fn no_skim_after_contact_or_during_a_dash_overlap() {
        // Touch the lip in Zen (survivable): the pipe is "touched".
        let mut g = with_pipe(GlideMode::Zen, spec(300.0, 168.0, None), GLIDER_X + 60.0);
        for i in 0..400 {
            g.y = if i == 40 {
                216.0 + 3.0
            } else {
                216.0 + 15.0 + 5.0
            };
            g.vy = 0.0;
            g.step(&idle());
        }
        assert_eq!(g.skims, 0, "contact voids the skim");
        // During a dash the glider ghosts through a closed pipe: no skim.
        let mut d = with_pipe(
            GlideMode::Classic,
            spec(100.0, 138.0, None),
            GLIDER_X + 24.0,
        );
        d.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        for _ in 0..100 {
            d.y = 400.0;
            d.vy = 0.0;
            d.step(&idle());
        }
        assert!(d.is_playing());
        assert_eq!(d.skims, 0);
    }

    #[test]
    fn a_pipe_hit_kills_in_classic_and_the_lip_counts() {
        // Body-only would miss: the glider is 10 units left of the body but
        // inside the 8-unit lip overhang plus its radius.
        let mut g = with_pipe(
            GlideMode::Classic,
            spec(300.0, 168.0, None),
            GLIDER_X + 15.0 + 8.0 - 1.0 + 50.0,
        );
        g.y = 216.0 - 5.0; // beside the top lip
        for _ in 0..40 {
            g.y = 216.0 - 5.0;
            g.vy = 0.0;
            g.step(&idle());
            if !g.is_playing() {
                break;
            }
        }
        assert_eq!(g.status, Status::Dead(Cause::Pipe));
    }

    #[test]
    fn floor_and_ceiling_kill_even_during_a_dash() {
        let mut g = started(GlideMode::Classic);
        g.y = FLOOR_Y - GLIDER_R - 0.5;
        g.vy = 200.0;
        run(&mut g, 5, idle());
        assert_eq!(g.status, Status::Dead(Cause::Ground));
        let mut c = started(GlideMode::Classic);
        c.y = CEIL_Y + GLIDER_R + 0.5;
        c.vy = -300.0;
        run(&mut c, 5, idle());
        assert_eq!(c.status, Status::Dead(Cause::Ceiling));
        // Floor stays lethal while the dash i-frames are active.
        let mut d = started(GlideMode::Classic);
        d.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        assert!(d.iframes > 0.0);
        d.y = FLOOR_Y - GLIDER_R - 0.1;
        d.vy = 300.0;
        run(&mut d, 3, idle());
        assert_eq!(d.status, Status::Dead(Cause::Ground));
    }

    #[test]
    fn a_dash_carries_the_glider_through_a_closed_pipe_alive() {
        // The glider sits inside the pipe body's height (gap far above).
        let lip_left_gap = 1.0;
        let x = GLIDER_X + GLIDER_R + LIP_OVERHANG + lip_left_gap;
        let mut g = with_pipe(GlideMode::Classic, spec(150.0, 138.0, None), x);
        g.y = 400.0;
        g.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        for _ in 0..80 {
            g.y = 400.0;
            g.vy = 0.0;
            g.step(&idle());
        }
        assert!(g.is_playing(), "phase shield + scroll boost = safe passage");
        assert!(g.pipes.iter().any(|p| p.touched), "it really went through");
        // Without the dash the very same setup kills.
        let mut n = with_pipe(GlideMode::Classic, spec(150.0, 138.0, None), x);
        n.y = 400.0;
        for _ in 0..80 {
            n.y = 400.0;
            n.vy = 0.0;
            n.step(&idle());
        }
        assert_eq!(n.status, Status::Dead(Cause::Pipe));
    }

    #[test]
    fn the_dash_has_a_2_5_second_cooldown_and_0_22_s_of_i_frames() {
        let mut g = started(GlideMode::Classic);
        g.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        assert!(g.iframes > 0.21 && !g.dash_ready());
        // i-frames last 0.22 s (27 steps incl. the first), then end.
        let mut with_iframes = 1;
        for _ in 0..40 {
            g.y = 300.0;
            g.vy = 0.0;
            g.step(&idle());
            with_iframes += (g.iframes > 0.0) as u32;
        }
        assert!((26..=27).contains(&with_iframes), "{with_iframes}");
        // A second press during the cooldown does nothing.
        g.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        assert_eq!(g.iframes, 0.0);
        // Ready again after 2.5 s in total.
        let used = 1 + 40 + 1;
        let remaining = (2.5 / FIXED_DT as f64) as usize - used + 2;
        for _ in 0..remaining {
            g.y = 300.0;
            g.vy = 0.0;
            g.step(&idle());
        }
        assert!(g.dash_ready(), "cooldown {}", g.dash_cd);
        g.dash_buf = 0.0;
        g.step(&GlideInput {
            dash_pressed: true,
            ..idle()
        });
        assert!(g.iframes > 0.0, "second dash works");
    }

    #[test]
    fn overdrive_widens_far_pipes_and_never_narrows_a_near_one() {
        let mut g = started(GlideMode::Classic);
        g.flow.flow = 95.0;
        assert!(g.overdrive());
        let far = Pipe::new(spec(300.0, 168.0, None), GLIDER_X + 900.0, 0.0);
        let near = Pipe::new(spec(300.0, 168.0, None), GLIDER_X + 200.0, 0.0);
        g.pipes = vec![far, near];
        for _ in 0..120 {
            g.y = 300.0;
            g.vy = 0.0;
            g.flow.flow = 95.0; // stay in Overdrive
            g.step(&idle());
        }
        assert!(
            (g.pipes[0].widen - OVERDRIVE_WIDEN).abs() < 1e-3,
            "far pipe widened"
        );
        // The near pipe was inside the freeze distance: unchanged.
        let near_widen = g.pipes[1].widen;
        assert_eq!(near_widen, 0.0, "near pipes never change");
        // When Overdrive ends, the widened far pipe narrows, but a pipe that
        // is already close keeps whatever gap it has.
        g.pipes.clear();
        g.pipes.push(Pipe::new(
            spec(300.0, 168.0, None),
            GLIDER_X + 250.0,
            OVERDRIVE_WIDEN,
        ));
        g.flow.flow = 0.0;
        for _ in 0..30 {
            g.y = 300.0;
            g.vy = 0.0;
            g.step(&idle());
        }
        assert_eq!(g.pipes[0].widen, OVERDRIVE_WIDEN, "frozen once near");
        assert!(g.pipes[0].gap_h() > 168.0);
    }

    #[test]
    fn speed_rises_monotonically_from_220_to_340() {
        assert!((base_speed(0) - 220.0).abs() < 1e-3);
        let mut prev = 0.0;
        for n in 0..2000 {
            let s = base_speed(n);
            assert!(s >= prev && s <= SPEED_MAX, "{n}");
            prev = s;
        }
        assert!(base_speed(500) > 339.0);
    }

    #[test]
    fn pitch_follows_the_vertical_velocity() {
        assert_eq!(pitch_angle(0.0), 0.0);
        assert!(pitch_angle(FLAP_VY) < 0.0, "nose up after a flap");
        assert!(pitch_angle(MAX_FALL) > 0.5, "nose down when falling");
        assert!(pitch_angle(-5000.0) >= -0.7 && pitch_angle(5000.0) <= 1.0);
        let mut last = -10.0;
        for v in (-800..800).step_by(50) {
            let p = pitch_angle(v as f32);
            assert!(p >= last);
            last = p;
        }
    }

    #[test]
    fn kinetic_pipes_move_deterministically_in_kinetic_only() {
        let mut s = spec(300.0, 168.0, None);
        s.motion = Some(Motion {
            amp: 40.0,
            omega: 1.5,
            phase: 0.5,
        });
        let p = Pipe::new(s, 600.0, 0.0);
        assert_eq!(p.gap_center(1.0, false), 300.0, "static in Classic");
        let a = p.gap_center(1.0, true);
        assert_eq!(a, p.gap_center(1.0, true));
        assert!((a - 300.0).abs() > 1.0 && (a - 300.0).abs() <= 40.0);
    }

    #[test]
    fn daily_course_does_not_depend_on_the_players_inputs() {
        let seed = 20261116;
        let mut a = GlideGame::new(GlideMode::DailyVibe, seed);
        let mut b = GlideGame::new(GlideMode::DailyVibe, seed);
        let layout = |g: &GlideGame| -> Vec<(f32, Option<f32>)> {
            g.pipes
                .iter()
                .map(|p| (p.spec.gap_y, p.spec.ring))
                .collect()
        };
        assert_eq!(layout(&a), layout(&b));
        // Two very different input sequences (b dies quickly, a flaps well).
        a.flow.flow = 0.0;
        for i in 0..4000 {
            a.step(&GlideInput {
                flap_pressed: i % 45 == 0,
                dive_held: false,
                dash_pressed: i % 300 == 0,
            });
            b.step(&idle());
        }
        let mut course_a = Course::new(seed);
        let mut course_b = Course::new(seed);
        for _ in 0..40 {
            assert_eq!(course_a.next_pipe(), course_b.next_pipe());
        }
        // The pipes both games generated agree wherever they overlap.
        for (pa, pb) in a.pipes.iter().zip(&b.pipes) {
            if pa.spec.index == pb.spec.index {
                assert_eq!(pa.spec, pb.spec);
            }
        }
        let other = GlideGame::new(GlideMode::DailyVibe, seed + 1);
        assert_ne!(layout(&a)[..2], layout(&other)[..2]);
    }

    #[test]
    fn zen_never_dies_and_bounce_events_stay_bounded_without_input() {
        let mut g = GlideGame::new(GlideMode::Zen, 99);
        g.step(&flap()); // start
        let mut bounces = 0;
        for _ in 0..(60.0 / FIXED_DT as f64) as usize {
            g.step(&idle());
            assert!(g.is_playing(), "Zen cannot die");
            bounces += g
                .drain_events()
                .iter()
                .filter(|e| matches!(e, GlideEvent::Bounce(_)))
                .count();
        }
        assert!(bounces > 10, "it bounces off the floor");
        assert!(bounces < 400, "no bounce storm: {bounces}");
        assert_eq!(bounces as u32, g.bounces);
    }

    #[test]
    fn zen_slips_out_of_a_pipe_instead_of_bouncing_every_step() {
        let mut g = with_pipe(GlideMode::Zen, spec(150.0, 138.0, None), GLIDER_X + 20.0);
        let mut bounce_events = 0;
        for _ in 0..240 {
            g.y = g.y.clamp(300.0, 420.0);
            g.step(&idle());
            bounce_events += g
                .drain_events()
                .iter()
                .filter(|e| matches!(e, GlideEvent::Bounce(Cause::Pipe)))
                .count();
        }
        assert!(g.is_playing());
        assert_eq!(bounce_events, 1, "one bounce, then the shield lets it pass");
    }

    #[test]
    fn frame_time_is_capped_and_nan_is_ignored() {
        let mut g = started(GlideMode::Zen);
        g.update(10.0, &idle());
        assert!(
            g.steps <= (MAX_FRAME_DT / FIXED_DT) as u64 + 1,
            "{}",
            g.steps
        );
        let s = g.steps;
        g.update(f32::NAN, &idle());
        assert_eq!(g.steps, s);
    }
}
