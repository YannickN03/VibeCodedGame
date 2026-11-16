//! `SnakeGame`: the complete, deterministic Snake rules engine.
//!
//! The simulation advances in discrete steps ([`SnakeGame::step`]); the
//! frame-time entry point [`SnakeGame::update`] accumulates `dt` into steps
//! and exposes the sub-step fraction [`SnakeGame::alpha`] for smooth
//! rendering. Nothing here knows about keys, frames or drawing.

use std::collections::{HashSet, VecDeque};

use crate::core::flow::{FlowEngine, FlowTier};
use crate::core::rng::SplitMix64;

use super::grid::{Direction, GridPos};
use super::obstacles;
use super::turn::TurnBuffer;

pub const GRID_W: i32 = 32;
pub const GRID_H: i32 = 18;
pub const START_LEN: usize = 4;
pub const START_POS: GridPos = GridPos::new(16, 9);

/// Slowest and fastest step rate in steps per second.
pub const MIN_SPEED: f32 = 7.5;
pub const MAX_SPEED: f32 = 18.0;
/// Number of apples after which the curve reaches [`MAX_SPEED`].
pub const APPLES_FOR_MAX_SPEED: f32 = 60.0;
/// Speed factor while the boost key is held (if enabled).
pub const BOOST_FACTOR: f32 = 1.6;

pub const APPLE_SCORE: u64 = 100;
pub const PULSE_SCORE: u64 = 250;
pub const APPLE_FLOW: f32 = 8.0;
pub const PULSE_FLOW: f32 = 25.0;
/// Extra flow when two apples are eaten within [`STREAK_WINDOW`] seconds.
pub const STREAK_FLOW: f32 = 4.0;
pub const STREAK_WINDOW: f32 = 2.0;
/// A pulse apple appears after every n-th apple.
pub const PULSE_EVERY: u32 = 5;
pub const PULSE_SECS: f32 = 7.0;
pub const PULSE_GROWTH: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnakeMode {
    /// Deadly walls, speed climbs with every apple.
    Classic,
    /// Walls wrap around.
    PortalWrap,
    /// Symmetric obstacles from the daily seed.
    DailyVibe,
    /// Cannot die: walls wrap, a self-bite trims the tail.
    Zen,
}

impl SnakeMode {
    pub const ALL: [SnakeMode; 4] = [
        SnakeMode::Classic,
        SnakeMode::PortalWrap,
        SnakeMode::DailyVibe,
        SnakeMode::Zen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            SnakeMode::Classic => "classic",
            SnakeMode::PortalWrap => "portal",
            SnakeMode::DailyVibe => "daily",
            SnakeMode::Zen => "zen",
        }
    }

    pub fn from_id(id: &str) -> SnakeMode {
        SnakeMode::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(SnakeMode::Classic)
    }

    /// Walls wrap around instead of killing.
    pub fn wraps(self) -> bool {
        matches!(self, SnakeMode::PortalWrap | SnakeMode::Zen)
    }

    pub fn immortal(self) -> bool {
        self == SnakeMode::Zen
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collision {
    WallCollision,
    ObstacleCollision,
    SelfCollision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Dead(Collision),
    /// Every cell is filled: the board is complete.
    Victory,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Apple {
    pub pos: GridPos,
    pub pulse: bool,
    /// Remaining seconds of a pulse apple (unused for standard apples).
    pub time_left: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SnakeEvent {
    Turned(Direction),
    Wrapped(GridPos),
    Ate {
        pos: GridPos,
        pulse: bool,
        points: u64,
    },
    PulseSpawned(GridPos),
    PulseExpired(GridPos),
    /// Zen: the tail beyond a self-bite was cut off.
    Trimmed {
        removed: Vec<GridPos>,
    },
    Died {
        cause: Collision,
        at: GridPos,
    },
    Victory,
}

/// Step rate for a number of eaten apples: logarithmic from
/// [`MIN_SPEED`] to [`MAX_SPEED`], monotonic and bounded.
pub fn steps_per_second(apples: u32) -> f32 {
    let k = 6.0f32;
    let t = (1.0 + apples as f32 / k).ln() / (1.0 + APPLES_FOR_MAX_SPEED / k).ln();
    (MIN_SPEED + (MAX_SPEED - MIN_SPEED) * t).clamp(MIN_SPEED, MAX_SPEED)
}

#[derive(Clone, Debug)]
pub struct SnakeGame {
    pub mode: SnakeMode,
    pub seed: u64,
    pub width: i32,
    pub height: i32,
    /// `segments[0]` is the head.
    pub segments: VecDeque<GridPos>,
    /// Segments before the last step (for sub-step interpolation).
    pub prev_segments: VecDeque<GridPos>,
    /// Current heading.
    pub dir: Direction,
    pub apple: Option<Apple>,
    pub pulse: Option<Apple>,
    pub obstacles: Vec<GridPos>,
    obstacle_set: HashSet<GridPos>,
    pub score: u64,
    pub apples_eaten: u32,
    pub status: Status,
    pub flow: FlowEngine,
    pub elapsed: f64,
    turns: TurnBuffer,
    pending_growth: u32,
    /// Total growth granted by apples (for invariants).
    pub total_growth: u32,
    /// Segments lost to Zen trims (for invariants).
    pub trimmed_total: u32,
    start_len: u32,
    since_eat: f32,
    acc: f32,
    boost: bool,
    rng: SplitMix64,
    events: Vec<SnakeEvent>,
}

impl SnakeGame {
    pub fn new(mode: SnakeMode, seed: u64) -> Self {
        Self::with_layout(mode, seed, GRID_W, GRID_H, START_LEN, START_POS)
    }

    /// Custom field size / start (used by tests; the game uses [`Self::new`]).
    /// The snake starts at `start` heading right, its body trailing left.
    pub fn with_layout(
        mode: SnakeMode,
        seed: u64,
        width: i32,
        height: i32,
        start_len: usize,
        start: GridPos,
    ) -> Self {
        let segments: VecDeque<GridPos> = (0..start_len as i32)
            .map(|i| GridPos::new(start.x - i, start.y))
            .collect();
        let obstacles = if mode == SnakeMode::DailyVibe {
            obstacles::generate(seed, width, height)
        } else {
            Vec::new()
        };
        let mut g = SnakeGame {
            mode,
            seed,
            width,
            height,
            prev_segments: segments.clone(),
            segments,
            dir: Direction::Right,
            apple: None,
            pulse: None,
            obstacle_set: obstacles.iter().copied().collect(),
            obstacles,
            score: 0,
            apples_eaten: 0,
            status: Status::Playing,
            flow: FlowEngine::new(),
            elapsed: 0.0,
            turns: TurnBuffer::new(),
            pending_growth: 0,
            total_growth: 0,
            trimmed_total: 0,
            start_len: start_len as u32,
            since_eat: 99.0,
            acc: 0.0,
            boost: false,
            rng: SplitMix64::new(seed ^ 0x5A4B_E5A4_B35E_ED00),
            events: Vec::new(),
        };
        if !g.spawn_standard() {
            g.status = Status::Victory;
        }
        g
    }

    pub fn is_playing(&self) -> bool {
        self.status == Status::Playing
    }

    pub fn head(&self) -> GridPos {
        self.segments[0]
    }

    pub fn start_len(&self) -> u32 {
        self.start_len
    }

    pub fn pending_growth(&self) -> u32 {
        self.pending_growth
    }

    pub fn tier(&self) -> FlowTier {
        self.flow.tier()
    }

    pub fn drain_events(&mut self) -> Vec<SnakeEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn in_bounds(&self, p: GridPos) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    pub fn is_obstacle(&self, p: GridPos) -> bool {
        self.obstacle_set.contains(&p)
    }

    /// The cell reached from `from` in `dir`: wrapped in wrapping modes,
    /// `None` when it would leave a walled field.
    pub fn next_cell(&self, from: GridPos, dir: Direction) -> Option<GridPos> {
        let n = from.step(dir);
        if self.in_bounds(n) {
            Some(n)
        } else if self.mode.wraps() {
            Some(GridPos::new(
                n.x.rem_euclid(self.width),
                n.y.rem_euclid(self.height),
            ))
        } else {
            None
        }
    }

    /// Plans a turn (see [`TurnBuffer`]). Returns `true` if it was queued.
    pub fn queue_turn(&mut self, dir: Direction) -> bool {
        self.is_playing() && self.turns.push(self.dir, dir)
    }

    pub fn planned_turns(&self) -> usize {
        self.turns.len()
    }

    /// Current step rate in steps per second.
    pub fn speed(&self) -> f32 {
        let base = steps_per_second(self.apples_eaten);
        if self.boost {
            base * BOOST_FACTOR
        } else {
            base
        }
    }

    /// Fraction `0..1` of the current step that has elapsed.
    pub fn alpha(&self) -> f32 {
        if !self.is_playing() {
            return 1.0;
        }
        (self.acc * self.speed()).clamp(0.0, 1.0)
    }

    /// Advances by `dt` seconds; `boost` speeds the snake up. Returns the
    /// number of steps taken.
    pub fn update(&mut self, dt: f32, boost: bool) -> u32 {
        if !self.is_playing() || !dt.is_finite() {
            return 0;
        }
        let dt = dt.clamp(0.0, 0.1);
        self.boost = boost;
        self.elapsed += dt as f64;
        self.since_eat += dt;
        self.flow.update(dt);
        if let Some(p) = &mut self.pulse {
            p.time_left -= dt;
            if p.time_left <= 0.0 {
                let pos = p.pos;
                self.pulse = None;
                self.events.push(SnakeEvent::PulseExpired(pos));
            }
        }
        self.acc += dt;
        let mut steps = 0;
        while self.is_playing() && self.acc >= 1.0 / self.speed() {
            self.acc -= 1.0 / self.speed();
            self.step();
            steps += 1;
        }
        steps
    }

    /// Cells not covered by the snake, an obstacle or an apple.
    fn free_cells(&self) -> Vec<GridPos> {
        let body: HashSet<GridPos> = self.segments.iter().copied().collect();
        (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| GridPos::new(x, y)))
            .filter(|p| {
                !body.contains(p)
                    && !self.obstacle_set.contains(p)
                    && self.apple.is_none_or(|a| a.pos != *p)
                    && self.pulse.is_none_or(|a| a.pos != *p)
            })
            .collect()
    }

    /// Places the standard apple on a random free cell. `false` = no free
    /// cell is left (the board is full).
    fn spawn_standard(&mut self) -> bool {
        let free = self.free_cells();
        if free.is_empty() {
            self.apple = None;
            return false;
        }
        let pos = free[self.rng.next_below(free.len() as u64) as usize];
        self.apple = Some(Apple {
            pos,
            pulse: false,
            time_left: 0.0,
        });
        true
    }

    fn spawn_pulse(&mut self) {
        let free = self.free_cells();
        if free.is_empty() {
            return;
        }
        let pos = free[self.rng.next_below(free.len() as u64) as usize];
        self.pulse = Some(Apple {
            pos,
            pulse: true,
            time_left: PULSE_SECS,
        });
        self.events.push(SnakeEvent::PulseSpawned(pos));
    }

    fn die(&mut self, cause: Collision, at: GridPos) {
        self.prev_segments = self.segments.clone();
        self.status = Status::Dead(cause);
        self.events.push(SnakeEvent::Died { cause, at });
    }

    /// One logical step. Collision order: 1. wall, 2. obstacle, 3. food,
    /// 4. self (the tail tip is excluded when it vacates its cell this step),
    /// 5. segment update.
    pub fn step(&mut self) {
        if !self.is_playing() {
            return;
        }
        if let Some(d) = self.turns.pop() {
            self.dir = d;
            self.events.push(SnakeEvent::Turned(d));
        }
        let head = self.head();

        // 1. Wall.
        let natural = head.step(self.dir);
        let Some(next) = self.next_cell(head, self.dir) else {
            self.die(Collision::WallCollision, natural);
            return;
        };
        if next != natural {
            self.events.push(SnakeEvent::Wrapped(next));
        }
        // 2. Obstacle.
        if self.is_obstacle(next) {
            self.die(Collision::ObstacleCollision, next);
            return;
        }
        // 3. Food. The snake grows over several steps if growth is pending.
        let eat_pulse = self.pulse.is_some_and(|a| a.pos == next);
        let eat_std = self.apple.is_some_and(|a| a.pos == next);
        let growth = if eat_pulse {
            PULSE_GROWTH
        } else if eat_std {
            1
        } else {
            0
        };
        let total_growth = self.pending_growth + growth;
        let will_grow = total_growth > 0;
        // 4. Self collision. A non-growing snake's tail tip leaves its cell
        // at the same moment, so the head may enter it: only the first
        // `len - 1` segments count. A growing snake keeps its tail.
        let len = self.segments.len();
        let body = if will_grow { len } else { len - 1 };
        let hit = self.segments.iter().take(body).position(|&s| s == next);
        let mut trimmed = false;
        if let Some(i) = hit {
            if !self.mode.immortal() {
                self.die(Collision::SelfCollision, next);
                return;
            }
            // Zen: bite = the tail from the bitten segment on is cut off.
            let removed: Vec<GridPos> = self.segments.iter().skip(i).copied().collect();
            self.prev_segments = self.segments.clone();
            self.segments.truncate(i);
            self.segments.push_front(next);
            // Net loss relative to an ordinary step (which keeps the length).
            self.trimmed_total += removed.len() as u32 - 1;
            self.events.push(SnakeEvent::Trimmed { removed });
            trimmed = true;
        }
        // 5. Segment update.
        if !trimmed {
            self.prev_segments = self.segments.clone();
            self.segments.push_front(next);
            if will_grow {
                self.pending_growth = total_growth - 1;
            } else {
                self.segments.pop_back();
                self.pending_growth = 0;
            }
        }
        self.total_growth += growth;

        // Eating (after the move, so the new snake blocks the next spawn).
        if eat_pulse || eat_std {
            self.eat(next, eat_pulse);
        }
    }

    fn eat(&mut self, pos: GridPos, pulse: bool) {
        let mult = self.flow.multiplier();
        let base = if pulse { PULSE_SCORE } else { APPLE_SCORE };
        let points = (base as f32 * mult).round() as u64;
        self.score += points;
        self.apples_eaten += 1;
        let mut flow = if pulse { PULSE_FLOW } else { APPLE_FLOW };
        if self.since_eat < STREAK_WINDOW {
            flow += STREAK_FLOW;
        }
        self.since_eat = 0.0;
        self.flow.register_event(flow);
        self.events.push(SnakeEvent::Ate { pos, pulse, points });
        if pulse {
            self.pulse = None;
            return;
        }
        if !self.spawn_standard() {
            self.status = Status::Victory;
            self.events.push(SnakeEvent::Victory);
            return;
        }
        if self.apples_eaten.is_multiple_of(PULSE_EVERY) && self.pulse.is_none() {
            self.spawn_pulse();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::*;

    fn game(mode: SnakeMode) -> SnakeGame {
        SnakeGame::new(mode, 42)
    }

    /// Replaces the snake with `cells` (head first) heading `dir`.
    fn put(g: &mut SnakeGame, cells: &[(i32, i32)], dir: Direction) {
        g.segments = cells.iter().map(|&(x, y)| GridPos::new(x, y)).collect();
        g.prev_segments = g.segments.clone();
        g.dir = dir;
        g.apple = None;
        g.pulse = None;
        g.turns.clear();
        g.pending_growth = 0;
    }

    fn apple_at(g: &mut SnakeGame, x: i32, y: i32) {
        g.apple = Some(Apple {
            pos: GridPos::new(x, y),
            pulse: false,
            time_left: 0.0,
        });
    }

    #[test]
    fn starts_with_length_4_at_16_9_heading_right() {
        let g = game(SnakeMode::Classic);
        assert_eq!((g.width, g.height), (32, 18));
        assert_eq!(g.segments.len(), 4);
        assert_eq!(g.head(), GridPos::new(16, 9));
        assert_eq!(g.dir, Right);
        assert_eq!(g.segments[3], GridPos::new(13, 9));
        assert!(g.apple.is_some() && g.pulse.is_none());
        assert!(g.is_playing());
    }

    #[test]
    fn moves_one_cell_per_step_keeping_length() {
        let mut g = game(SnakeMode::Classic);
        g.apple = None;
        g.step();
        assert_eq!(g.head(), GridPos::new(17, 9));
        assert_eq!(g.segments.len(), 4);
        assert_eq!(g.prev_segments[0], GridPos::new(16, 9));
    }

    #[test]
    fn turn_buffer_blocks_reversal_in_the_game() {
        let mut g = game(SnakeMode::Classic);
        assert!(!g.queue_turn(Left), "cannot reverse into the neck");
        assert!(g.queue_turn(Up));
        assert!(!g.queue_turn(Down), "double input cannot reverse");
        assert!(g.queue_turn(Left), "legal two-step U-turn");
        g.apple = None;
        g.step();
        assert_eq!(g.head(), GridPos::new(16, 8));
        g.step();
        assert_eq!(g.head(), GridPos::new(15, 8));
        assert!(g.is_playing(), "no self collision on a fast U-turn");
    }

    #[test]
    fn classic_wall_kills() {
        let mut g = game(SnakeMode::Classic);
        put(&mut g, &[(31, 5), (30, 5), (29, 5)], Right);
        g.step();
        assert_eq!(g.status, Status::Dead(Collision::WallCollision));
        let ev = g.drain_events();
        assert!(ev.iter().any(|e| matches!(
            e,
            SnakeEvent::Died {
                cause: Collision::WallCollision,
                ..
            }
        )));
        // A dead snake does not move any more.
        let segs = g.segments.clone();
        g.step();
        assert_eq!(g.segments, segs);
    }

    #[test]
    fn portal_wrap_wraps_on_all_four_edges() {
        let mut g = game(SnakeMode::PortalWrap);
        put(&mut g, &[(31, 5), (30, 5), (29, 5)], Right);
        g.step();
        assert_eq!(g.head(), GridPos::new(0, 5));
        put(&mut g, &[(0, 5), (1, 5), (2, 5)], Left);
        g.step();
        assert_eq!(g.head(), GridPos::new(31, 5));
        put(&mut g, &[(5, 0), (5, 1), (5, 2)], Up);
        g.step();
        assert_eq!(g.head(), GridPos::new(5, 17));
        put(&mut g, &[(5, 17), (5, 16), (5, 15)], Down);
        g.step();
        assert_eq!(g.head(), GridPos::new(5, 0));
        assert!(g.is_playing());
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, SnakeEvent::Wrapped(_)))
        );
    }

    #[test]
    fn obstacle_collision_in_daily_mode() {
        let mut g = SnakeGame::new(SnakeMode::DailyVibe, 20261116);
        assert!(!g.obstacles.is_empty());
        let wall = g.obstacles[0];
        put(
            &mut g,
            &[
                (wall.x - 1, wall.y),
                (wall.x - 2, wall.y),
                (wall.x - 3, wall.y),
            ],
            Right,
        );
        g.step();
        assert_eq!(g.status, Status::Dead(Collision::ObstacleCollision));
    }

    #[test]
    fn daily_seed_gives_identical_obstacles_and_apples() {
        let mut a = SnakeGame::new(SnakeMode::DailyVibe, 20261116);
        let mut b = SnakeGame::new(SnakeMode::DailyVibe, 20261116);
        assert_eq!(a.obstacles, b.obstacles);
        assert_eq!(a.apple, b.apple);
        for _ in 0..20 {
            a.queue_turn(Up);
            b.queue_turn(Up);
            a.step();
            b.step();
        }
        assert_eq!(a.segments, b.segments);
        assert_eq!(a.apple, b.apple);
        let c = SnakeGame::new(SnakeMode::DailyVibe, 20261117);
        assert_ne!(a.obstacles, c.obstacles);
        // Other modes have no obstacles.
        assert!(game(SnakeMode::Classic).obstacles.is_empty());
    }

    #[test]
    fn tail_tip_vacates_so_following_it_is_safe() {
        // 2x2 loop: the head enters the cell the tail leaves this very step.
        let mut g = SnakeGame::with_layout(SnakeMode::Classic, 1, 2, 2, 2, GridPos::new(1, 0));
        put(&mut g, &[(1, 0), (0, 0), (0, 1), (1, 1)], Down);
        g.step();
        assert!(g.is_playing(), "head may enter the vacating tail cell");
        let cells: Vec<_> = g.segments.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(cells, vec![(1, 1), (1, 0), (0, 0), (0, 1)]);
    }

    #[test]
    fn growing_snake_cannot_enter_its_tail() {
        let mut g = SnakeGame::with_layout(SnakeMode::Classic, 1, 2, 2, 2, GridPos::new(1, 0));
        put(&mut g, &[(1, 0), (0, 0), (0, 1), (1, 1)], Down);
        g.pending_growth = 1; // tail stays this step
        g.step();
        assert_eq!(g.status, Status::Dead(Collision::SelfCollision));
    }

    #[test]
    fn eating_with_a_free_tail_cell_in_a_loop() {
        // The head eats an apple while a body cell next to it is NOT the
        // target: ordinary growth by one.
        let mut g = game(SnakeMode::Classic);
        put(&mut g, &[(5, 5), (4, 5), (3, 5), (2, 5)], Right);
        apple_at(&mut g, 6, 5);
        g.step();
        assert_eq!(g.segments.len(), 5);
        assert_eq!(g.apples_eaten, 1);
        assert_eq!(g.score, 100);
        assert!(
            g.apple.is_some_and(|a| a.pos != GridPos::new(6, 5)),
            "new apple"
        );
    }

    #[test]
    fn hitting_own_body_kills_in_classic() {
        let mut g = game(SnakeMode::Classic);
        // A hook shape: heading Up the head runs into its own body.
        put(&mut g, &[(5, 5), (6, 5), (6, 4), (5, 4), (4, 4)], Up);
        g.step();
        assert_eq!(g.status, Status::Dead(Collision::SelfCollision));
    }

    #[test]
    fn victory_when_the_board_is_full() {
        // 3x2 board: five cells are the snake, the last one holds the apple.
        let mut g = SnakeGame::with_layout(SnakeMode::Classic, 1, 3, 2, 2, GridPos::new(1, 0));
        put(&mut g, &[(1, 1), (0, 1), (0, 0), (1, 0), (2, 0)], Right);
        apple_at(&mut g, 2, 1);
        g.step();
        assert_eq!(g.segments.len(), 6);
        assert_eq!(g.status, Status::Victory);
        assert!(g.drain_events().contains(&SnakeEvent::Victory));
        assert!(g.apple.is_none());
        g.step(); // no more movement after victory
        assert_eq!(g.segments.len(), 6);
    }

    #[test]
    fn apple_never_spawns_on_snake_or_obstacle() {
        for seed in 0..50 {
            let g = SnakeGame::new(SnakeMode::DailyVibe, 20260101 + seed);
            let a = g.apple.expect("apple").pos;
            assert!(!g.segments.contains(&a));
            assert!(!g.is_obstacle(a));
        }
    }

    #[test]
    fn pulse_apple_appears_after_five_apples_and_expires() {
        let mut g = game(SnakeMode::PortalWrap);
        // Eat five standard apples in a straight line (wrap-free area).
        for i in 0..5 {
            put(&mut g, &[(2 + i, 3), (1 + i, 3), (i, 3)], Right);
            apple_at(&mut g, 3 + i, 3);
            g.step();
        }
        assert_eq!(g.apples_eaten, 5);
        let p = g.pulse.expect("pulse apple after 5 apples");
        assert!(p.pulse && (p.time_left - PULSE_SECS).abs() < 1e-6);
        // It expires after 7 seconds.
        g.drain_events();
        g.update(1.0, false);
        g.update(1.0, false);
        for _ in 0..70 {
            if g.is_playing() {
                g.update(0.1, false);
            }
        }
        assert!(g.is_playing(), "portal mode: straight line never dies");
        assert!(g.pulse.is_none());
        assert!(g.drain_events().iter().any(|e| matches!(
            e,
            SnakeEvent::PulseExpired(_) | SnakeEvent::Ate { pulse: true, .. }
        )));
    }

    #[test]
    fn pulse_apple_scores_250_grows_two_and_adds_25_flow() {
        let mut g = game(SnakeMode::Classic);
        put(&mut g, &[(5, 5), (4, 5), (3, 5), (2, 5)], Right);
        g.pulse = Some(Apple {
            pos: GridPos::new(6, 5),
            pulse: true,
            time_left: 5.0,
        });
        g.apple = None;
        g.apple = Some(Apple {
            pos: GridPos::new(20, 12),
            pulse: false,
            time_left: 0.0,
        });
        g.step();
        assert_eq!(g.score, 250, "x1.0 flow multiplier in Chill");
        assert!((g.flow.flow - 25.0).abs() < 1e-3);
        assert_eq!(g.segments.len(), 5, "first of two growth steps");
        assert_eq!(g.pending_growth(), 1);
        g.step();
        assert_eq!(g.segments.len(), 6, "second growth step");
        g.step();
        assert_eq!(g.segments.len(), 6, "growth finished");
        assert!(g.pulse.is_none());
    }

    #[test]
    fn growing_into_tail_tip_while_growth_is_pending_is_a_collision() {
        // After a pulse apple the tail does not move for two steps.
        let mut g = SnakeGame::with_layout(SnakeMode::Classic, 1, 4, 4, 2, GridPos::new(1, 1));
        put(&mut g, &[(1, 1), (1, 0), (0, 0), (0, 1)], Left);
        g.pending_growth = 1;
        g.step(); // head -> (0,1), the tail tip stays: collision
        assert_eq!(g.status, Status::Dead(Collision::SelfCollision));
    }

    #[test]
    fn score_uses_the_flow_multiplier() {
        let mut g = game(SnakeMode::Classic);
        g.flow.flow = 70.0; // Sync tier = 1.5x
        put(&mut g, &[(5, 5), (4, 5), (3, 5), (2, 5)], Right);
        apple_at(&mut g, 6, 5);
        g.step();
        assert_eq!(g.score, 150);
    }

    #[test]
    fn zen_cannot_die_walls_wrap_and_bites_trim_the_tail() {
        let mut g = game(SnakeMode::Zen);
        put(&mut g, &[(31, 5), (30, 5), (29, 5)], Right);
        g.step();
        assert_eq!(g.head(), GridPos::new(0, 5), "walls wrap in Zen");
        // A bite: the loop closes onto the 4th segment.
        put(&mut g, &[(5, 5), (6, 5), (6, 4), (5, 4), (4, 4)], Up);
        g.drain_events();
        g.step();
        assert!(g.is_playing(), "Zen is immortal");
        // Head moved into (5,4), which was segment index 3: tail cut there.
        let cells: Vec<_> = g.segments.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(cells, vec![(5, 4), (5, 5), (6, 5), (6, 4)]);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, SnakeEvent::Trimmed { removed } if removed.len() == 2))
        );
        assert_eq!(g.trimmed_total, 1);
    }

    #[test]
    fn speed_curve_is_monotonic_and_bounded() {
        assert!((steps_per_second(0) - MIN_SPEED).abs() < 1e-4);
        let mut prev = 0.0;
        for a in 0..400 {
            let s = steps_per_second(a);
            assert!(s >= prev, "monotonic at {a}");
            assert!((MIN_SPEED..=MAX_SPEED).contains(&s));
            prev = s;
        }
        assert!((steps_per_second(60) - MAX_SPEED).abs() < 1e-3);
        assert_eq!(steps_per_second(10_000), MAX_SPEED);
        // Logarithmic: the first apples speed up more than later ones.
        assert!(
            steps_per_second(10) - steps_per_second(0)
                > steps_per_second(60) - steps_per_second(50)
        );
    }

    #[test]
    fn update_steps_with_dt_and_exposes_alpha() {
        let mut g = game(SnakeMode::Classic);
        g.apple = None;
        let dt = 1.0 / 60.0;
        let mut steps = 0;
        for _ in 0..60 {
            steps += g.update(dt, false);
        }
        // 7.5 steps per second for one second.
        assert!((7..=8).contains(&steps), "{steps}");
        let a = g.alpha();
        assert!((0.0..=1.0).contains(&a));
        // Boost is faster.
        let mut b = game(SnakeMode::Classic);
        b.apple = None;
        let mut boosted = 0;
        for _ in 0..30 {
            boosted += b.update(dt, true);
        }
        assert!(boosted >= 5, "boost: {boosted}");
        assert!(b.speed() > steps_per_second(0));
    }

    #[test]
    fn invariants_hold_over_a_long_random_walk() {
        // Random legal turns in every mode: structural invariants only.
        for mode in SnakeMode::ALL {
            let mut g = SnakeGame::new(mode, 777);
            let mut rng = SplitMix64::new(9);
            for _ in 0..3000 {
                if rng.next_below(3) == 0 {
                    g.queue_turn(Direction::ALL[rng.next_below(4) as usize]);
                }
                g.step();
                if !g.is_playing() {
                    g = SnakeGame::new(mode, rng.next_u64());
                    continue;
                }
                let set: HashSet<_> = g.segments.iter().collect();
                assert_eq!(
                    set.len(),
                    g.segments.len(),
                    "{mode:?}: overlapping segments"
                );
                for s in &g.segments {
                    assert!(g.in_bounds(*s) && !g.is_obstacle(*s));
                }
                assert_eq!(
                    g.segments.len() as u32 + g.pending_growth() + g.trimmed_total,
                    g.start_len() + g.total_growth,
                    "{mode:?}: length bookkeeping"
                );
                if let Some(a) = g.apple {
                    assert!(!g.segments.contains(&a.pos) && !g.is_obstacle(a.pos));
                }
            }
        }
    }
}
