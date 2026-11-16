//! `MazeGame`: the complete, deterministic rules of VibeCoded Maze.
//!
//! Fixed 1/120 s steps (f64 accumulator, 1 microsecond tolerance). The
//! player moves continuously (7.2 tiles/s) with pre-turn corner cutting;
//! ghosts decide at tile centres. Everything is a function of the inputs and
//! the seed: no `macroquad`, no wall-clock time.

use crate::core::flow::{FlowEngine, FlowTier};
use crate::core::rng::SplitMix64;

use super::ghost::{Dir, GhostKind, GhostMode, chase_target, choose_dir, descend, options};
use super::maze::{
    BOX_HOME, CORE_TILE, EXIT_TILE, H, Maze, PLAYER_SPAWN, TUNNEL_ROW, Tile, W, wrap_x,
};

pub const STEP: f32 = 1.0 / 120.0;
pub const MAX_FRAME_DT: f32 = 0.05;
pub const PLAYER_SPEED: f32 = 7.2;
/// A turn is taken up to this far before the crossing's centre.
pub const PRE_TURN: f32 = 0.38;
pub const MEET_DISTANCE: f32 = 0.55;
pub const HIT_STOP_STEPS: u32 = 7;
pub const READY_STEPS: u32 = 120;
pub const LIVES: u32 = 3;
pub const DOT_SCORE: u64 = 10;
pub const ORB_SCORE: u64 = 50;
pub const DOT_FLOW: f32 = 1.5;
pub const ORB_FLOW: f32 = 15.0;
pub const GHOST_FLOW: f32 = 18.0;
pub const CORE_FLOW: f32 = 20.0;
pub const DOT_CHAIN_WINDOW: f32 = 0.65;
pub const OVERDRIVE_SPEED: f32 = 1.2;
pub const CORE_SECS: f32 = 9.0;
pub const BLITZ_SECS: f32 = 120.0;
pub const BLINK_SECS: f32 = 2.0;
pub const ZEN_INVULNERABLE: f32 = 1.5;
pub const ZEN_FLOW_LOSS: f32 = 25.0;
/// Ghosts wait in the box until these stage times (Apex starts outside).
pub const RELEASE_AT: [f32; 4] = [0.0, 1.0, 3.0, 6.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MazeMode {
    Classic,
    BlitzFrenzy,
    DailyVibe,
    Zen,
}

impl MazeMode {
    pub const ALL: [MazeMode; 4] = [
        MazeMode::Classic,
        MazeMode::BlitzFrenzy,
        MazeMode::DailyVibe,
        MazeMode::Zen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            MazeMode::Classic => "classic",
            MazeMode::BlitzFrenzy => "blitz",
            MazeMode::DailyVibe => "daily",
            MazeMode::Zen => "zen",
        }
    }

    pub fn from_id(id: &str) -> MazeMode {
        MazeMode::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(MazeMode::Classic)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    /// No lives left.
    Dead,
    /// The Blitz countdown ran out.
    TimeUp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MazeInput {
    /// A direction pressed this frame; it stays buffered until possible.
    pub dir: Option<Dir>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub dir: Option<Dir>,
    pub desired: Option<Dir>,
}

impl Player {
    pub fn at(tile: (i32, i32)) -> Player {
        Player {
            x: tile.0 as f32 + 0.5,
            y: tile.1 as f32 + 0.5,
            dir: None,
            desired: None,
        }
    }

    pub fn tile(&self) -> (i32, i32) {
        (wrap_x(self.x.floor() as i32), self.y.floor() as i32)
    }

    fn can_enter(maze: &Maze, t: (i32, i32), d: Dir) -> bool {
        let (dx, dy) = d.vec();
        maze.player_walkable(t.0 + dx, t.1 + dy)
    }

    /// Signed distance to the centre of the current tile along `d`
    /// (positive while the centre is still ahead).
    fn ahead(&self, d: Dir) -> f32 {
        let (dx, dy) = d.vec();
        let (a, s) = if d.is_horizontal() {
            (self.x, dx as f32)
        } else {
            (self.y, dy as f32)
        };
        (a.floor() + 0.5 - a) * s
    }

    /// Moves `dist` tiles with buffered turns, corner cutting and wall stops.
    pub fn advance(&mut self, maze: &Maze, dist: f32) {
        let mut rem = dist;
        // A reversal on the same corridor applies at once.
        if let (Some(c), Some(w)) = (self.dir, self.desired)
            && w == c.opposite()
        {
            self.dir = Some(w);
        }
        for _ in 0..8 {
            if rem <= 1e-6 {
                break;
            }
            let t = self.tile();
            let Some(dir) = self.dir else {
                match self.desired {
                    Some(w) if Self::can_enter(maze, t, w) => {
                        self.dir = Some(w);
                        continue;
                    }
                    _ => break,
                }
            };
            // Pre-turn: take an orthogonal turn up to PRE_TURN before the centre.
            if let Some(w) = self.desired
                && w.is_horizontal() != dir.is_horizontal()
                && Self::can_enter(maze, t, w)
            {
                let a = self.ahead(dir);
                if (-1e-4..=PRE_TURN + 1e-4).contains(&a) {
                    self.dir = Some(w);
                    continue;
                }
            }
            let a = self.ahead(dir);
            let free = Self::can_enter(maze, t, dir);
            let m = if a > 1e-4 {
                rem.min(a)
            } else if free {
                rem.min(a + 1.0)
            } else {
                break; // stopped exactly on the tile centre in front of a wall
            };
            let (dx, dy) = dir.vec();
            // Move along the corridor ...
            if dir.is_horizontal() {
                self.x += dx as f32 * m;
            } else {
                self.y += dy as f32 * m;
            }
            // ... and slide the other axis towards the corridor centre at the
            // same rate (corner cutting).
            if dir.is_horizontal() {
                let off = self.y.floor() + 0.5 - self.y;
                self.y += off.clamp(-m, m);
            } else {
                let off = self.x.floor() + 0.5 - self.x;
                self.x += off.clamp(-m, m);
            }
            rem -= m;
            // Snap exactly onto a centre we just reached.
            if (self.ahead(dir)).abs() < 1e-4 {
                if dir.is_horizontal() {
                    self.x = self.x.floor() + 0.5;
                } else {
                    self.y = self.y.floor() + 0.5;
                }
            }
            if self.x < 0.0 {
                self.x += W as f32;
            }
            if self.x >= W as f32 {
                self.x -= W as f32;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum House {
    /// Waits inside the box until the stage time reaches this value.
    Waiting(f32),
    Leaving,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub kind: GhostKind,
    /// Tile the ghost last left (or stands on).
    pub from: (i32, i32),
    pub dir: Option<Dir>,
    /// Progress along `from -> from + dir`, `0..1`.
    pub t: f32,
    pub house: House,
    pub eaten: bool,
}

impl Ghost {
    /// Centre position (not wrapped: may reach `-0.5` or `28.5` in the tunnel).
    pub fn pos(&self) -> (f32, f32) {
        let (dx, dy) = self.dir.map_or((0, 0), |d| d.vec());
        (
            self.from.0 as f32 + 0.5 + dx as f32 * self.t,
            self.from.1 as f32 + 0.5 + dy as f32 * self.t,
        )
    }

    pub fn tile(&self) -> (i32, i32) {
        let (x, y) = self.pos();
        (wrap_x(x.floor() as i32), y.floor() as i32)
    }

    /// Turns around on the spot (a continuous 180 degree turn).
    pub fn reverse(&mut self) {
        let Some(d) = self.dir else { return };
        if self.t <= 1e-6 {
            self.dir = Some(d.opposite());
        } else {
            let (dx, dy) = d.vec();
            self.from = (wrap_x(self.from.0 + dx), self.from.1 + dy);
            self.dir = Some(d.opposite());
            self.t = 1.0 - self.t;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Core {
    pub tile: (i32, i32),
    pub timer: f32,
    pub value: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MazeEvent {
    Dot {
        waka: bool,
    },
    Orb,
    GhostEaten {
        kind: GhostKind,
        points: u64,
        chain: u32,
    },
    LifeLost,
    ZenBump,
    StageClear {
        stage: u32,
    },
    CoreSpawned,
    CoreEaten {
        points: u64,
    },
    FrightenedEnd,
    PhaseChange {
        chase: bool,
    },
    Regenerated {
        right: bool,
    },
    Ready,
    GameOver,
    TimeUp,
}

#[derive(Clone, Debug)]
pub struct MazeGame {
    pub mode: MazeMode,
    pub seed: u64,
    pub status: Status,
    pub base: Maze,
    pub maze: Maze,
    pub player: Player,
    pub ghosts: [Ghost; 4],
    pub lives: u32,
    pub score: u64,
    pub stage: u32,
    pub dots_left: usize,
    pub dots_total: usize,
    pub dots_eaten: u32,
    pub ghosts_eaten: u32,
    pub flow: FlowEngine,
    /// Ghosts eaten in the current Frightened phase.
    pub chain: u32,
    pub fright_t: f32,
    pub chase: bool,
    phase_t: f32,
    pub stage_time: f32,
    pub time: f32,
    pub steps: u64,
    /// Remaining hit-stop and "ready" steps (everything stands still).
    pub freeze: u32,
    pub ready: u32,
    dot_timer: f32,
    waka: bool,
    pub core: Option<Core>,
    pub cores_spawned: u8,
    pub blitz_left: f32,
    pub zen_inv: f32,
    half_left: [usize; 2],
    pub home_dist: Vec<i32>,
    pub exit_dist: Vec<i32>,
    frng: SplitMix64,
    acc: f64,
    events: Vec<MazeEvent>,
}

impl MazeGame {
    pub fn new(mode: MazeMode, seed: u64) -> Self {
        let base = if mode == MazeMode::DailyVibe {
            Maze::daily(seed)
        } else {
            Maze::classic()
        };
        let home_dist = base.ghost_distances(BOX_HOME);
        let exit_dist = base.ghost_distances(EXIT_TILE);
        let mut g = MazeGame {
            mode,
            seed,
            status: Status::Playing,
            maze: base.clone(),
            dots_total: base.dots_total(),
            dots_left: base.dots_total(),
            base,
            player: Player::at(PLAYER_SPAWN),
            ghosts: Self::fresh_ghosts(),
            lives: LIVES,
            score: 0,
            stage: 1,
            dots_eaten: 0,
            ghosts_eaten: 0,
            flow: FlowEngine::new(),
            chain: 0,
            fright_t: 0.0,
            chase: false,
            phase_t: 0.0,
            stage_time: 0.0,
            time: 0.0,
            steps: 0,
            freeze: 0,
            ready: READY_STEPS,
            dot_timer: 0.0,
            waka: false,
            core: None,
            cores_spawned: 0,
            blitz_left: BLITZ_SECS,
            zen_inv: 0.0,
            half_left: [0, 0],
            home_dist,
            exit_dist,
            frng: SplitMix64::new(seed ^ 0xF31A_9E57_0001_0001),
            acc: 0.0,
            events: Vec::new(),
        };
        g.count_halves();
        g.phase_t = g.scatter_secs();
        g
    }

    fn fresh_ghosts() -> [Ghost; 4] {
        let mk = |kind, from: (i32, i32), house, dir| Ghost {
            kind,
            from,
            dir,
            t: 0.0,
            house,
            eaten: false,
        };
        [
            mk(GhostKind::Apex, (14, 8), House::Out, Some(Dir::Left)),
            mk(
                GhostKind::Vektor,
                (13, 11),
                House::Waiting(RELEASE_AT[1]),
                None,
            ),
            mk(
                GhostKind::Flank,
                (12, 11),
                House::Waiting(RELEASE_AT[2]),
                None,
            ),
            mk(
                GhostKind::Drift,
                (15, 11),
                House::Waiting(RELEASE_AT[3]),
                None,
            ),
        ]
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

    pub fn drain_events(&mut self) -> Vec<MazeEvent> {
        std::mem::take(&mut self.events)
    }

    /// The effective mode of a ghost right now.
    pub fn mode_of(&self, g: &Ghost) -> GhostMode {
        if g.eaten {
            GhostMode::Eaten
        } else if self.fright_t > 0.0 {
            GhostMode::Frightened
        } else if self.chase {
            GhostMode::Chase
        } else {
            GhostMode::Scatter
        }
    }

    pub fn frightened_secs(&self) -> f32 {
        (7.0 - 0.7 * (self.stage - 1) as f32).max(2.5)
    }

    fn scatter_secs(&self) -> f32 {
        (7.0 - 0.5 * (self.stage - 1) as f32).max(3.0)
    }

    const CHASE_SECS: f32 = 20.0;

    /// Ghost speed in tiles/s for the current stage: 85 % of the player's
    /// speed, +2 % per stage, never above 95 %.
    pub fn ghost_base_speed(&self) -> f32 {
        (0.85 + 0.02 * (self.stage - 1) as f32).min(0.95) * PLAYER_SPEED
    }

    fn ghost_speed(&self, g: &Ghost) -> f32 {
        let base = self.ghost_base_speed();
        let mut v = match self.mode_of(g) {
            GhostMode::Eaten => base * 2.2,
            GhostMode::Frightened => base * 0.55,
            _ => base,
        };
        // The tunnel slows ghosts down by 40 % (row 11 only).
        if g.from.1 == TUNNEL_ROW as i32 && (g.from.0 <= 2 || g.from.0 >= 25) {
            v *= 0.6;
        }
        v
    }

    pub fn player_speed(&self) -> f32 {
        PLAYER_SPEED
            * if self.overdrive() {
                OVERDRIVE_SPEED
            } else {
                1.0
            }
    }

    fn count_halves(&mut self) {
        self.half_left = [0, 0];
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                if matches!(self.maze.get(x, y), Tile::Dot | Tile::PowerOrb) {
                    self.half_left[(x >= 14) as usize] += 1;
                }
            }
        }
    }

    // ------------------------------------------------------------ main loop

    pub fn update(&mut self, dt: f32, input: &MazeInput) {
        if !self.is_playing() || !dt.is_finite() {
            return;
        }
        if let Some(d) = input.dir {
            self.player.desired = Some(d);
        }
        self.acc += dt.clamp(0.0, MAX_FRAME_DT) as f64;
        while self.acc + 1e-6 >= STEP as f64 && self.is_playing() {
            self.acc -= STEP as f64;
            self.step_inner();
        }
    }

    /// One fixed step with an explicit input (tests, bots).
    pub fn step(&mut self, input: &MazeInput) {
        if let Some(d) = input.dir {
            self.player.desired = Some(d);
        }
        self.step_inner();
    }

    fn step_inner(&mut self) {
        if !self.is_playing() {
            return;
        }
        self.steps += 1;
        if self.freeze > 0 {
            // Hit-stop: everything, including timers, stands still.
            self.freeze -= 1;
            return;
        }
        if self.ready > 0 {
            self.ready -= 1;
            if self.ready == 0 {
                self.events.push(MazeEvent::Ready);
            }
            return;
        }
        let dt = STEP;
        self.time += dt;
        self.stage_time += dt;
        self.dot_timer = (self.dot_timer - dt).max(0.0);
        self.zen_inv = (self.zen_inv - dt).max(0.0);
        self.flow.update(dt);
        if self.mode == MazeMode::BlitzFrenzy {
            self.blitz_left -= dt;
            if self.blitz_left <= 0.0 {
                self.blitz_left = 0.0;
                self.status = Status::TimeUp;
                self.events.push(MazeEvent::TimeUp);
                return;
            }
        }
        self.update_phases(dt);
        if let Some(c) = &mut self.core {
            c.timer -= dt;
            if c.timer <= 0.0 {
                self.core = None;
            }
        }
        let speed = self.player_speed();
        self.player.advance(&self.maze, speed * dt);
        self.eat_here();
        if !self.is_playing() {
            return;
        }
        for i in 0..4 {
            self.release_check(i);
            let v = self.ghost_speed(&self.ghosts[i]);
            self.move_ghost(i, v * dt);
        }
        self.collide();
        if self.is_playing() {
            self.check_stage();
        }
    }

    fn update_phases(&mut self, dt: f32) {
        if self.fright_t > 0.0 {
            // The Scatter/Chase timer pauses while ghosts are frightened.
            self.fright_t -= dt;
            if self.fright_t <= 0.0 {
                self.fright_t = 0.0;
                self.chain = 0;
                self.events.push(MazeEvent::FrightenedEnd);
            }
            return;
        }
        self.phase_t -= dt;
        if self.phase_t <= 0.0 {
            self.chase = !self.chase;
            self.phase_t = if self.chase {
                Self::CHASE_SECS
            } else {
                self.scatter_secs()
            };
            self.events
                .push(MazeEvent::PhaseChange { chase: self.chase });
            self.reverse_active();
        }
    }

    /// Forces a 180 degree turn on every ghost that is out and not eaten.
    fn reverse_active(&mut self) {
        for g in &mut self.ghosts {
            if g.house == House::Out && !g.eaten {
                g.reverse();
            }
        }
    }

    // ------------------------------------------------------------ eating

    fn eat_here(&mut self) {
        let (tx, ty) = self.player.tile();
        match self.maze.get(tx, ty) {
            Tile::Dot => {
                self.maze.set(tx, ty, Tile::Empty);
                self.on_pellet(tx, false);
            }
            Tile::PowerOrb => {
                self.maze.set(tx, ty, Tile::Empty);
                self.on_pellet(tx, true);
            }
            _ => {}
        }
        if let Some(c) = self.core
            && c.tile == (tx, ty)
        {
            self.core = None;
            let pts = self.award(c.value);
            self.flow.register_event(CORE_FLOW);
            self.events.push(MazeEvent::CoreEaten { points: pts });
        }
    }

    fn award(&mut self, base: u64) -> u64 {
        let pts = (base as f32 * self.flow.multiplier()).round() as u64;
        self.score += pts;
        pts
    }

    fn on_pellet(&mut self, x: i32, orb: bool) {
        self.dots_left -= 1;
        self.dots_eaten += 1;
        self.half_left[(x >= 14) as usize] -= 1;
        if orb {
            self.score += ORB_SCORE;
            self.flow.register_event(ORB_FLOW);
            self.events.push(MazeEvent::Orb);
            // A new orb refreshes the timer; the ghost chain continues.
            let was_off = self.fright_t <= 0.0;
            self.fright_t = self.frightened_secs();
            if was_off {
                self.chain = 0;
            }
            for g in &mut self.ghosts {
                if g.house == House::Out && !g.eaten {
                    g.reverse();
                }
            }
        } else {
            self.score += DOT_SCORE;
            // Unbroken dot chains (within 0.65 s) keep the flow growing.
            if self.dot_timer > 0.0 {
                self.flow.register_event(DOT_FLOW);
            }
            self.dot_timer = DOT_CHAIN_WINDOW;
            self.waka = !self.waka;
            self.events.push(MazeEvent::Dot { waka: self.waka });
        }
        self.maybe_spawn_core();
        self.maybe_regenerate(x >= 14);
    }

    fn maybe_spawn_core(&mut self) {
        if self.mode == MazeMode::BlitzFrenzy || self.core.is_some() {
            return;
        }
        let eaten = 1.0 - self.dots_left as f32 / self.dots_total as f32;
        let want = match self.cores_spawned {
            0 => eaten >= 0.30,
            1 => eaten >= 0.70,
            _ => false,
        };
        if want {
            let mut r = SplitMix64::new(
                self.seed
                    ^ (self.stage as u64).wrapping_mul(0x9E37_79B9)
                    ^ (self.cores_spawned as u64 + 1).wrapping_mul(0x85EB_CA6B_C2B2_AE35),
            );
            let value = 300 + 100 * r.next_below(8);
            self.core = Some(Core {
                tile: CORE_TILE,
                timer: CORE_SECS,
                value,
            });
            self.cores_spawned += 1;
            self.events.push(MazeEvent::CoreSpawned);
        }
    }

    /// Blitz: an emptied half of the maze (all dots AND orbs) fills up again.
    fn maybe_regenerate(&mut self, right: bool) {
        if self.mode != MazeMode::BlitzFrenzy || self.half_left[right as usize] != 0 {
            return;
        }
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                if (x >= 14) == right && matches!(self.base.get(x, y), Tile::Dot | Tile::PowerOrb) {
                    self.maze.set(x, y, self.base.get(x, y));
                    self.dots_left += 1;
                    self.half_left[right as usize] += 1;
                }
            }
        }
        // Never refill the tile the player stands on twice in a row.
        let (px, py) = self.player.tile();
        if matches!(self.maze.get(px, py), Tile::Dot | Tile::PowerOrb) {
            self.maze.set(px, py, Tile::Empty);
            self.dots_left -= 1;
            self.half_left[(px >= 14) as usize] -= 1;
        }
        self.events.push(MazeEvent::Regenerated { right });
    }

    fn check_stage(&mut self) {
        if self.mode == MazeMode::BlitzFrenzy || self.dots_left > 0 {
            return;
        }
        self.stage += 1;
        self.maze = self.base.clone();
        self.dots_left = self.maze.dots_total();
        self.dots_total = self.dots_left;
        self.count_halves();
        self.cores_spawned = 0;
        self.core = None;
        self.events
            .push(MazeEvent::StageClear { stage: self.stage });
        self.reset_actors();
    }

    fn reset_actors(&mut self) {
        self.player = Player::at(PLAYER_SPAWN);
        self.ghosts = Self::fresh_ghosts();
        self.fright_t = 0.0;
        self.chain = 0;
        self.chase = false;
        self.phase_t = self.scatter_secs();
        self.stage_time = 0.0;
        self.freeze = 0;
        self.ready = READY_STEPS;
    }

    // ------------------------------------------------------------ ghosts

    fn release_check(&mut self, i: usize) {
        if let House::Waiting(at) = self.ghosts[i].house
            && self.stage_time >= at
        {
            self.ghosts[i].house = House::Leaving;
            self.decide(i);
        }
    }

    fn move_ghost(&mut self, i: usize, dist: f32) {
        let mut rem = dist;
        for _ in 0..8 {
            if rem <= 1e-6 {
                break;
            }
            let g = self.ghosts[i];
            if matches!(g.house, House::Waiting(_)) {
                break;
            }
            let Some(d) = g.dir else {
                self.decide(i);
                if self.ghosts[i].dir.is_none() {
                    break;
                }
                continue;
            };
            let left = 1.0 - g.t;
            if rem < left {
                self.ghosts[i].t += rem;
                break;
            }
            rem -= left;
            let (dx, dy) = d.vec();
            let gm = &mut self.ghosts[i];
            gm.from = (wrap_x(gm.from.0 + dx), gm.from.1 + dy);
            gm.t = 0.0;
            self.decide(i);
        }
    }

    /// The ghost stands on a tile centre: pick the next direction.
    fn decide(&mut self, i: usize) {
        let g = self.ghosts[i];
        let cur = g.dir.unwrap_or(Dir::Left);
        match g.house {
            House::Waiting(_) => {}
            House::Leaving => {
                if g.from.1 <= EXIT_TILE.1 {
                    self.ghosts[i].house = House::Out;
                    self.decide(i);
                } else {
                    self.ghosts[i].dir = descend(&self.maze, &self.exit_dist, g.from);
                }
            }
            House::Out => {
                if g.eaten {
                    if g.from == BOX_HOME {
                        let gm = &mut self.ghosts[i];
                        gm.eaten = false;
                        gm.house = House::Waiting(self.stage_time + 1.0);
                        gm.dir = None;
                        gm.t = 0.0;
                    } else {
                        self.ghosts[i].dir = descend(&self.maze, &self.home_dist, g.from);
                    }
                    return;
                }
                let dir = match self.mode_of(&g) {
                    GhostMode::Frightened => {
                        let opts = options(&self.maze, g.from, cur, false);
                        if opts.is_empty() {
                            None
                        } else {
                            Some(opts[self.frng.next_below(opts.len() as u64) as usize])
                        }
                    }
                    mode => {
                        let target = self.target_of(&g, mode);
                        choose_dir(&self.maze, g.from, cur, target, false)
                    }
                };
                self.ghosts[i].dir = dir.or(Some(cur.opposite()));
            }
        }
        // Eaten ghosts enter through the gate; everybody else is done.
        if self.ghosts[i].eaten && self.ghosts[i].dir.is_none() {
            self.ghosts[i].dir = Some(Dir::Down);
        }
    }

    fn target_of(&self, g: &Ghost, mode: GhostMode) -> (i32, i32) {
        if mode == GhostMode::Scatter {
            return g.kind.corner();
        }
        let apex = self.ghosts[0].tile();
        let pdir = self.player.dir.or(self.player.desired).unwrap_or(Dir::Left);
        chase_target(g.kind, g.from, self.player.tile(), pdir, apex)
    }

    // ------------------------------------------------------------ collisions

    fn collide(&mut self) {
        let (px, py) = (self.player.x, self.player.y);
        for i in 0..4 {
            let g = self.ghosts[i];
            if g.eaten || g.house != House::Out {
                continue;
            }
            let (gx, gy) = g.pos();
            let dx = (gx - px).abs();
            let dx = dx.min(W as f32 - dx);
            let dist = (dx * dx + (gy - py) * (gy - py)).sqrt();
            if g.tile() != self.player.tile() && dist >= MEET_DISTANCE {
                continue;
            }
            if self.fright_t > 0.0 {
                self.eat_ghost(i);
                return;
            }
            if self.mode == MazeMode::Zen {
                if self.zen_inv <= 0.0 {
                    // Zen: a touch only costs flow; the ghost is sent home.
                    self.flow.flow = (self.flow.flow - ZEN_FLOW_LOSS).max(0.0);
                    self.ghosts[i].eaten = true;
                    self.zen_inv = ZEN_INVULNERABLE;
                    self.events.push(MazeEvent::ZenBump);
                }
                continue;
            }
            self.lose_life();
            return;
        }
    }

    fn eat_ghost(&mut self, i: usize) {
        let value = 200u64 << self.chain.min(3);
        let pts = self.award(value);
        self.chain += 1;
        self.ghosts_eaten += 1;
        self.ghosts[i].eaten = true;
        self.flow.register_event(GHOST_FLOW);
        self.freeze = HIT_STOP_STEPS;
        self.events.push(MazeEvent::GhostEaten {
            kind: self.ghosts[i].kind,
            points: pts,
            chain: self.chain,
        });
    }

    fn lose_life(&mut self) {
        self.lives = self.lives.saturating_sub(1);
        self.events.push(MazeEvent::LifeLost);
        if self.lives == 0 {
            self.status = Status::Dead;
            self.events.push(MazeEvent::GameOver);
        } else {
            // Positions reset, the eaten dots stay eaten.
            self.reset_actors();
        }
    }
}

/// Test and tooling helpers.
impl MazeGame {
    /// Starts a Frightened phase right now (developer aid).
    pub fn frighten(&mut self, secs: f32) {
        self.fright_t = secs;
        self.chain = 0;
        self.reverse_active();
    }

    /// Skips the "ready" countdown.
    pub fn skip_ready(&mut self) {
        self.ready = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::maze::bot;

    fn input(d: Dir) -> MazeInput {
        MazeInput { dir: Some(d) }
    }

    fn idle() -> MazeInput {
        MazeInput::default()
    }

    fn ready(mode: MazeMode, seed: u64) -> MazeGame {
        let mut g = MazeGame::new(mode, seed);
        g.skip_ready();
        g
    }

    fn player(x: f32, y: f32, dir: Dir, desired: Option<Dir>) -> Player {
        Player {
            x,
            y,
            dir: Some(dir),
            desired,
        }
    }

    const D: f32 = PLAYER_SPEED * STEP; // 0.06 tiles per step

    // ------------------------------------------------------------ player

    #[test]
    fn a_turn_is_taken_up_to_0_38_tiles_before_the_crossing_centre() {
        let m = Maze::classic();
        // (5,5) is a crossing: the centre is x = 5.5. Moving Right, want Down.
        let mut p = player(5.5 - 0.37, 5.5, Dir::Right, Some(Dir::Down));
        p.advance(&m, 0.001);
        assert_eq!(p.dir, Some(Dir::Down), "0.37 before the centre: taken");
        let mut p = player(5.5 - 0.39, 5.5, Dir::Right, Some(Dir::Down));
        p.advance(&m, 0.001);
        assert_eq!(p.dir, Some(Dir::Right), "0.39 before: still too early");
        assert_eq!(p.desired, Some(Dir::Down), "the wish stays buffered");
        // Moving on, the buffered turn is taken as soon as it is within reach.
        let mut p = player(5.5 - 0.50, 5.5, Dir::Right, Some(Dir::Down));
        let mut turned_at = None;
        for _ in 0..30 {
            p.advance(&m, D);
            if p.dir == Some(Dir::Down) {
                turned_at = Some(5.5 - p.x);
                break;
            }
        }
        let d = turned_at.expect("turned");
        assert!(
            (0.38 - D..=0.38 + 1e-3).contains(&d),
            "turned {d} before the centre"
        );
    }

    #[test]
    fn corner_cutting_slides_onto_the_new_corridor_without_touching_a_wall() {
        let m = Maze::classic();
        let mut p = player(5.5 - 0.5, 5.5, Dir::Right, Some(Dir::Down));
        let mut aligned_after = None;
        for step in 0..40 {
            p.advance(&m, D);
            let t = p.tile();
            assert!(
                m.player_walkable(t.0, t.1),
                "step {step}: inside a wall at {t:?}"
            );
            if aligned_after.is_none() && (p.x - 5.5).abs() < 1e-4 {
                aligned_after = Some(step);
            }
        }
        // The x offset of at most 0.38 is slid away at 0.06 per step.
        let n = aligned_after.expect("aligned");
        assert!(n <= 12, "aligned after {n} steps");
        assert_eq!(p.dir, Some(Dir::Down));
        assert!(p.y > 5.5 + 1.0, "moved down the corridor");
    }

    #[test]
    fn a_reversal_on_the_same_corridor_applies_at_once() {
        let m = Maze::classic();
        let mut p = player(3.2, 5.5, Dir::Right, Some(Dir::Left));
        let x0 = p.x;
        p.advance(&m, D);
        assert_eq!(p.dir, Some(Dir::Left));
        assert!(
            (x0 - p.x - D).abs() < 1e-5,
            "already moving back in the very first step"
        );
    }

    #[test]
    fn an_impossible_turn_stays_buffered_until_a_gap_appears() {
        let m = Maze::classic();
        // Row 5 between the crossings: Up is a wall at x = 2 and 3.
        let mut p = player(2.5, 5.5, Dir::Right, Some(Dir::Up));
        for _ in 0..20 {
            p.advance(&m, D);
        }
        assert_eq!(p.desired, Some(Dir::Up));
        assert_eq!(p.dir, Some(Dir::Right));
        // It happens at the crossing x = 5 (Up is open there).
        for _ in 0..60 {
            p.advance(&m, D);
            if p.dir == Some(Dir::Up) {
                break;
            }
        }
        assert_eq!(p.dir, Some(Dir::Up));
        assert!((4.9..=5.6).contains(&p.x));
    }

    #[test]
    fn the_player_stops_exactly_on_the_tile_centre_in_front_of_a_wall() {
        let m = Maze::classic();
        let mut p = player(24.5, 1.5, Dir::Right, None);
        for _ in 0..200 {
            p.advance(&m, D);
        }
        assert!((p.x - 26.5).abs() < 1e-5, "{}", p.x);
        assert!((p.y - 1.5).abs() < 1e-5);
        assert_eq!(p.dir, Some(Dir::Right), "still facing the wall");
        // A wish into a wall changes nothing and does not jitter.
        p.desired = Some(Dir::Up);
        p.advance(&m, D);
        assert_eq!((p.x, p.y), (26.5, 1.5));
    }

    #[test]
    fn the_tunnel_wraps_in_both_directions() {
        let m = Maze::classic();
        let mut left = player(1.5, 11.5, Dir::Left, None);
        left.advance(&m, 3.0);
        assert!((left.x - 26.5).abs() < 1e-4, "left tunnel: {}", left.x);
        assert!(m.player_walkable(left.tile().0, left.tile().1));
        let mut right = player(26.5, 11.5, Dir::Right, None);
        right.advance(&m, 3.0);
        assert!((right.x - 1.5).abs() < 1e-4, "right tunnel: {}", right.x);
        // Step by step the tile is always valid across the seam.
        let mut p = player(2.0, 11.5, Dir::Left, None);
        for _ in 0..200 {
            p.advance(&m, D);
            let t = p.tile();
            assert!(m.player_walkable(t.0, t.1));
            assert!((0.0..W as f32).contains(&p.x));
        }
    }

    #[test]
    fn standing_still_the_player_starts_on_the_first_possible_wish() {
        let m = Maze::classic();
        let mut p = Player::at((3, 17));
        p.desired = Some(Dir::Up); // (3,16) is a wall
        p.advance(&m, 0.5);
        assert_eq!(p.dir, None);
        p.desired = Some(Dir::Left);
        p.advance(&m, 0.5);
        assert!(p.x < 3.5 && p.dir == Some(Dir::Left));
    }

    #[test]
    fn turning_still_works_at_the_overdrive_speed() {
        let m = Maze::classic();
        for speed in [PLAYER_SPEED, PLAYER_SPEED * OVERDRIVE_SPEED] {
            let mut p = player(5.5 - 1.0, 5.5, Dir::Right, Some(Dir::Down));
            for step in 0..80 {
                p.advance(&m, speed * STEP);
                let t = p.tile();
                assert!(m.player_walkable(t.0, t.1), "speed {speed} step {step}");
            }
            assert_eq!(p.dir, Some(Dir::Down), "speed {speed}");
            assert!(
                (p.x - 5.5).abs() < 1e-3,
                "aligned at speed {speed}: {}",
                p.x
            );
        }
        let mut g = ready(MazeMode::Classic, 1);
        g.flow.flow = 95.0;
        assert!(g.overdrive());
        assert!((g.player_speed() - PLAYER_SPEED * 1.2).abs() < 1e-4);
    }

    // ------------------------------------------------------------ ghosts

    #[test]
    fn every_scatter_chase_switch_forces_a_180_degree_turn() {
        let mut g = ready(MazeMode::Classic, 1);
        for _ in 0..5 {
            g.step(&idle());
        }
        let before = g.ghosts[0].dir.expect("apex moves");
        g.phase_t = 0.001;
        g.step(&idle());
        assert!(g.chase, "switched to Chase");
        assert_eq!(g.ghosts[0].dir, Some(before.opposite()));
        // And back to Scatter after 20 s (time-jump the timer).
        let before = g.ghosts[0].dir.unwrap();
        g.phase_t = 0.001;
        g.step(&idle());
        assert!(!g.chase);
        assert_eq!(g.ghosts[0].dir, Some(before.opposite()));
    }

    #[test]
    fn scatter_lasts_7_seconds_and_chase_20() {
        let mut g = ready(MazeMode::Zen, 1);
        let mut chase_at = None;
        let mut scatter_again = None;
        for i in 0..(40 * 120) {
            g.step(&idle());
            if g.chase && chase_at.is_none() {
                chase_at = Some(i);
            }
            if chase_at.is_some() && !g.chase && scatter_again.is_none() {
                scatter_again = Some(i);
            }
            if !g.is_playing() {
                break;
            }
        }
        let c = chase_at.expect("chase starts") as f32 / 120.0;
        assert!((6.9..7.2).contains(&c), "scatter 7 s: {c}");
        if let Some(s) = scatter_again {
            assert!(((s as f32 / 120.0) - c - 20.0).abs() < 0.2, "chase 20 s");
        }
    }

    #[test]
    fn the_scatter_chase_timer_pauses_while_ghosts_are_frightened() {
        let mut g = ready(MazeMode::Classic, 1);
        g.phase_t = 5.0;
        g.frighten(2.0);
        for _ in 0..(2.0 * 120.0) as usize + 2 {
            g.step(&idle());
        }
        assert_eq!(g.fright_t, 0.0);
        assert!(
            (g.phase_t - 5.0).abs() < 0.05,
            "timer stood still: {}",
            g.phase_t
        );
    }

    #[test]
    fn frightened_ghosts_turn_around_slow_down_and_pick_turns_deterministically() {
        let mut a = ready(MazeMode::Classic, 5);
        let mut b = ready(MazeMode::Classic, 5);
        for _ in 0..50 {
            a.step(&idle());
            b.step(&idle());
        }
        let before = a.ghosts[0].dir.unwrap();
        a.frighten(6.0);
        b.frighten(6.0);
        assert_eq!(a.ghosts[0].dir, Some(before.opposite()), "turns around");
        let g0 = a.ghosts[0];
        assert!((a.ghost_speed(&g0) - a.ghost_base_speed() * 0.55).abs() < 1e-4);
        for _ in 0..300 {
            a.step(&idle());
            b.step(&idle());
        }
        assert_eq!(a.ghosts[0], b.ghosts[0], "same seed, same random turns");
        assert_eq!(a.ghosts[0].dir, b.ghosts[0].dir);
    }

    #[test]
    fn eaten_ghosts_run_home_at_2_2_times_speed_and_regenerate() {
        for seed in 0..8u64 {
            let mut g = ready(MazeMode::DailyVibe, 20260100 + seed);
            let apex = &mut g.ghosts[0];
            apex.from = (1, 1);
            apex.dir = Some(Dir::Down);
            apex.t = 0.0;
            apex.eaten = true;
            let g0 = g.ghosts[0];
            assert!((g.ghost_speed(&g0) - g.ghost_base_speed() * 2.2).abs() < 1e-4);
            let mut home = None;
            for i in 0..(20 * 120) {
                // Keep the player away so nothing interferes.
                g.player = Player::at((13, 17));
                g.step(&idle());
                if matches!(g.ghosts[0].house, House::Waiting(_)) && !g.ghosts[0].eaten {
                    home = Some(i);
                    break;
                }
            }
            let i = home.unwrap_or_else(|| panic!("seed {seed}: the eyes never got home"));
            assert!(i < 15 * 120, "seed {seed}: took {} s", i / 120);
        }
    }

    #[test]
    fn ghosts_wait_in_the_box_and_leave_at_their_release_times() {
        let mut g = ready(MazeMode::Classic, 3);
        assert!(matches!(g.ghosts[1].house, House::Waiting(_)));
        let mut out_at = [None; 4];
        for i in 0..(10 * 120) {
            g.player = Player::at((13, 17));
            g.step(&idle());
            for (slot, gh) in out_at.iter_mut().zip(&g.ghosts) {
                if slot.is_none() && gh.house == House::Out {
                    *slot = Some(i as f32 / 120.0);
                }
            }
        }
        assert_eq!(out_at[0], Some(0.0), "Apex starts outside");
        let (v, f, d) = (out_at[1].unwrap(), out_at[2].unwrap(), out_at[3].unwrap());
        assert!(v > 1.0 && v < f && f < d, "release order: {v} {f} {d}");
    }

    #[test]
    fn the_tunnel_slows_ghosts_by_40_percent_on_row_11_only() {
        let g = ready(MazeMode::Classic, 1);
        let mut gh = g.ghosts[0];
        gh.eaten = false;
        let normal = g.ghost_speed(&gh);
        gh.from = (1, 11);
        assert!((g.ghost_speed(&gh) - normal * 0.6).abs() < 1e-4, "x <= 2");
        gh.from = (25, 11);
        assert!((g.ghost_speed(&gh) - normal * 0.6).abs() < 1e-4, "x >= 25");
        gh.from = (3, 11);
        assert!(
            (g.ghost_speed(&gh) - normal).abs() < 1e-4,
            "inside the maze"
        );
        gh.from = (1, 9);
        assert!(
            (g.ghost_speed(&gh) - normal).abs() < 1e-4,
            "other rows are not slowed"
        );
    }

    #[test]
    fn ghosts_get_faster_but_stay_below_the_player() {
        let mut g = ready(MazeMode::Classic, 1);
        let mut last = 0.0;
        for stage in 1..30 {
            g.stage = stage;
            let v = g.ghost_base_speed();
            assert!(v >= last && v < PLAYER_SPEED, "stage {stage}: {v}");
            last = v;
        }
        g.stage = 1;
        let f1 = g.frightened_secs();
        g.stage = 4;
        assert!(g.frightened_secs() < f1, "shorter Frightened phases");
        g.stage = 99;
        assert!(g.frightened_secs() >= 2.0, "still long enough to blink");
    }

    // ------------------------------------------------------------ collisions

    fn put_ghost_on_player(g: &mut MazeGame, i: usize) {
        let t = g.player.tile();
        g.ghosts[i].from = t;
        g.ghosts[i].t = 0.0;
        g.ghosts[i].dir = Some(Dir::Left);
        g.ghosts[i].house = House::Out;
        g.ghosts[i].eaten = false;
    }

    #[test]
    fn a_frightened_ghost_is_eaten_with_the_doubling_combo_200_400_800_1600() {
        let mut g = ready(MazeMode::Classic, 1);
        g.flow.flow = 0.0;
        g.frighten(7.0);
        let mut points = Vec::new();
        for i in 0..4 {
            put_ghost_on_player(&mut g, i);
            g.freeze = 0;
            g.flow.flow = 0.0;
            g.step(&idle());
            for e in g.drain_events() {
                if let MazeEvent::GhostEaten {
                    points: p, chain, ..
                } = e
                {
                    points.push((p, chain));
                }
            }
        }
        assert_eq!(points, vec![(200, 1), (400, 2), (800, 3), (1600, 4)]);
        assert_eq!(g.ghosts_eaten, 4);
        assert!(g.ghosts.iter().all(|x| x.eaten));
    }

    #[test]
    fn the_ghost_combo_is_capped_at_1600_and_scaled_by_the_flow_multiplier() {
        let mut g = ready(MazeMode::BlitzFrenzy, 1);
        g.frighten(7.0);
        g.flow.flow = 70.0; // Sync: 1.5x
        let mut points = Vec::new();
        for i in 0..6 {
            let k = i % 4;
            put_ghost_on_player(&mut g, k);
            g.freeze = 0;
            g.flow.flow = 70.0;
            g.step(&idle());
            for e in g.drain_events() {
                if let MazeEvent::GhostEaten { points: p, .. } = e {
                    points.push(p);
                }
            }
        }
        assert_eq!(
            points,
            vec![300, 600, 1200, 2400, 2400, 2400],
            "1600 cap x 1.5"
        );
    }

    #[test]
    fn eating_a_ghost_freezes_everything_for_7_steps_and_gives_flow() {
        let mut g = ready(MazeMode::Classic, 1);
        g.frighten(7.0);
        let f0 = g.flow.flow;
        put_ghost_on_player(&mut g, 0);
        g.step(&idle());
        assert_eq!(g.freeze, HIT_STOP_STEPS);
        assert!(g.flow.flow >= f0 + GHOST_FLOW - 0.5);
        let (time, fright, ghosts, p) = (g.time, g.fright_t, g.ghosts, g.player);
        for _ in 0..HIT_STOP_STEPS {
            g.step(&idle());
            assert_eq!(
                (g.time, g.fright_t, g.ghosts, g.player),
                (time, fright, ghosts, p)
            );
        }
        g.step(&idle());
        assert!(g.time > time, "time runs again");
    }

    #[test]
    fn a_new_orb_refreshes_the_timer_and_keeps_the_chain_while_frightened() {
        let mut g = ready(MazeMode::Classic, 1);
        g.frighten(7.0);
        g.fright_t = 3.0;
        g.chain = 2;
        g.on_pellet(1, true);
        assert!((g.fright_t - g.frightened_secs()).abs() < 1e-4, "refreshed");
        assert_eq!(g.chain, 2, "the chain continues during the phase");
        // After the phase ended, a new orb starts a fresh chain.
        let mut h = ready(MazeMode::Classic, 1);
        h.chain = 3;
        h.fright_t = 0.0;
        h.on_pellet(1, true);
        assert_eq!(h.chain, 0);
    }

    #[test]
    fn an_active_ghost_costs_a_life_resets_positions_and_keeps_the_eaten_dots() {
        let mut g = ready(MazeMode::Classic, 1);
        for _ in 0..600 {
            let d = bot::choose(&g);
            g.step(&MazeInput { dir: d });
            if g.dots_eaten >= 10 {
                break;
            }
        }
        assert!(g.dots_eaten >= 10);
        let left = g.dots_left;
        let score = g.score;
        g.fright_t = 0.0;
        put_ghost_on_player(&mut g, 0);
        g.step(&idle());
        assert_eq!(g.lives, LIVES - 1);
        assert_eq!(g.dots_left, left, "dots stay eaten");
        assert_eq!(g.score, score);
        assert_eq!(g.player.tile(), PLAYER_SPAWN, "positions reset");
        assert_eq!(g.ghosts[0].from, (14, 8));
        assert!(g.ready > 0 && g.is_playing());
        // Three hits end the game.
        for _ in 0..2 {
            g.skip_ready();
            put_ghost_on_player(&mut g, 0);
            g.step(&idle());
        }
        assert_eq!(g.status, Status::Dead);
        assert_eq!(g.lives, 0);
    }

    #[test]
    fn a_head_on_approach_never_passes_through_even_at_full_relative_speed() {
        let mut g = ready(MazeMode::Classic, 1);
        g.chase = true;
        // Row 1 is a long straight corridor.
        g.player = player(8.5, 1.5, Dir::Right, None);
        g.ghosts[0].from = (20, 1);
        g.ghosts[0].dir = Some(Dir::Left);
        g.ghosts[0].t = 0.0;
        g.ghosts[0].house = House::Out;
        let mut hit = false;
        for _ in 0..600 {
            g.step(&idle());
            let (gx, _) = g.ghosts[0].pos();
            if g.lives < LIVES {
                hit = true;
                break;
            }
            assert!(g.player.x <= gx, "the player passed through the ghost");
        }
        assert!(hit, "they met head-on");
    }

    #[test]
    fn collisions_work_across_the_tunnel_seam() {
        let mut g = ready(MazeMode::Classic, 1);
        g.fright_t = 0.0;
        g.player = player(27.8, 11.5, Dir::Right, None);
        g.ghosts[0].from = (0, 11);
        g.ghosts[0].dir = Some(Dir::Left);
        g.ghosts[0].t = 0.3; // pos x = 0.2: wrap distance 0.4
        g.ghosts[0].house = House::Out;
        g.collide();
        assert_eq!(g.lives, LIVES - 1, "wrap-aware distance < 0.55");
        // Just out of reach on the far side of the seam is no hit.
        let mut h = ready(MazeMode::Classic, 1);
        h.player = player(27.0, 11.5, Dir::Right, None);
        h.ghosts[0].from = (1, 11);
        h.ghosts[0].dir = Some(Dir::Right);
        h.ghosts[0].t = 0.0;
        h.ghosts[0].house = House::Out;
        h.collide();
        assert_eq!(h.lives, LIVES);
    }

    #[test]
    fn zen_costs_flow_not_lives_and_does_not_drain_every_step() {
        let mut g = ready(MazeMode::Zen, 1);
        g.flow.flow = 80.0;
        let mut bumps = 0;
        for _ in 0..(60 * 120) {
            g.step(&idle()); // idle player in the open: ghosts keep hunting
            bumps += g
                .drain_events()
                .iter()
                .filter(|e| matches!(e, MazeEvent::ZenBump))
                .count();
        }
        assert!(g.is_playing());
        assert_eq!(g.lives, LIVES, "never loses a life");
        assert!(bumps >= 1, "the ghosts did touch the player");
        assert!(bumps <= 60, "bounded: invulnerable between bumps ({bumps})");
        assert!(g.flow.flow >= 0.0);
        // One touch costs flow once, and sends the ghost home.
        let mut h = ready(MazeMode::Zen, 1);
        h.flow.flow = 80.0;
        put_ghost_on_player(&mut h, 0);
        h.step(&idle());
        assert!((h.flow.flow - (80.0 - ZEN_FLOW_LOSS)).abs() < 1.0);
        assert!(h.ghosts[0].eaten);
        put_ghost_on_player(&mut h, 1);
        h.step(&idle());
        assert!(h.flow.flow > 50.0, "invulnerable right after a bump");
    }

    // ------------------------------------------------------------ scoring and modes

    #[test]
    fn dots_pay_10_orbs_50_and_the_flow_chain_needs_a_dot_within_0_65_seconds() {
        let mut g = ready(MazeMode::Classic, 1);
        g.flow.flow = 0.0;
        g.on_pellet(3, false);
        assert_eq!(g.score, 10);
        assert_eq!(g.flow.flow, 0.0, "the first dot of a chain earns no flow");
        g.on_pellet(3, false);
        assert!(
            (g.flow.flow - DOT_FLOW).abs() < 1e-4,
            "second dot within the window: +1.5"
        );
        g.dot_timer = 0.0; // the window ran out
        g.on_pellet(3, false);
        assert!(
            (g.flow.flow - DOT_FLOW).abs() < 1e-4,
            "a pause breaks the chain"
        );
        let f = g.flow.flow;
        g.on_pellet(1, true);
        assert_eq!(g.score, 10 * 3 + 50);
        assert!((g.flow.flow - f - ORB_FLOW).abs() < 1e-4);
        assert!(g.fright_t > 6.0);
    }

    #[test]
    fn dots_alternate_the_waka_flag() {
        let mut g = ready(MazeMode::Classic, 1);
        g.on_pellet(2, false);
        g.on_pellet(2, false);
        let wakas: Vec<bool> = g
            .drain_events()
            .into_iter()
            .filter_map(|e| match e {
                MazeEvent::Dot { waka } => Some(waka),
                _ => None,
            })
            .collect();
        assert_eq!(wakas, vec![true, false]);
    }

    fn eat_fraction(g: &mut MazeGame, frac: f32) {
        let target = (g.dots_total as f32 * frac) as usize;
        'outer: for y in 0..H as i32 {
            for x in 0..W as i32 {
                if g.dots_total - g.dots_left >= target {
                    break 'outer;
                }
                if matches!(g.maze.get(x, y), Tile::Dot) {
                    g.maze.set(x, y, Tile::Empty);
                    g.on_pellet(x, false);
                }
            }
        }
    }

    #[test]
    fn two_vibe_cores_appear_per_stage_at_30_and_70_percent_for_9_seconds() {
        let mut g = ready(MazeMode::Classic, 7);
        eat_fraction(&mut g, 0.2);
        assert!(g.core.is_none() && g.cores_spawned == 0);
        eat_fraction(&mut g, 0.31);
        let c = g.core.expect("first core");
        assert_eq!(c.tile, CORE_TILE);
        assert!((300..=1000).contains(&c.value) && c.value.is_multiple_of(100));
        assert_eq!(c.timer, CORE_SECS);
        // It vanishes after 9 s.
        g.skip_ready();
        for _ in 0..(9 * 120 + 5) {
            g.player = Player::at((1, 1));
            g.step(&idle());
        }
        assert!(g.core.is_none());
        eat_fraction(&mut g, 0.71);
        assert_eq!(g.cores_spawned, 2);
        assert!(g.core.is_some());
        eat_fraction(&mut g, 0.9);
        assert_eq!(g.cores_spawned, 2, "never more than two per stage");
        // Eating it pays value x flow and +20 flow.
        let mut h = ready(MazeMode::Classic, 7);
        eat_fraction(&mut h, 0.31);
        let value = h.core.unwrap().value;
        h.flow.flow = 0.0;
        h.maze.set(CORE_TILE.0, CORE_TILE.1, Tile::Empty);
        h.player = Player::at(CORE_TILE);
        let s = h.score;
        h.step(&idle());
        assert_eq!(h.score - s, value);
        assert!((h.flow.flow - CORE_FLOW).abs() < 1.0);
        // Values come from a stage-keyed stream: the same everywhere.
        let mut k = ready(MazeMode::Classic, 7);
        eat_fraction(&mut k, 0.31);
        assert_eq!(k.core.unwrap().value, value);
    }

    #[test]
    fn clearing_all_dots_starts_the_next_stage_faster_with_shorter_frightened() {
        let mut g = ready(MazeMode::Classic, 1);
        let (v1, f1) = (g.ghost_base_speed(), g.frightened_secs());
        eat_fraction(&mut g, 1.0);
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                if g.maze.get(x, y) == Tile::PowerOrb {
                    g.maze.set(x, y, Tile::Empty);
                    g.on_pellet(x, true);
                }
            }
        }
        g.step(&idle());
        assert_eq!(g.stage, 2);
        assert_eq!(g.dots_left, g.dots_total, "dots refilled");
        assert!(g.ghost_base_speed() > v1 && g.frightened_secs() < f1);
        assert_eq!(g.player.tile(), PLAYER_SPAWN);
        assert!(g.ready > 0);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, MazeEvent::StageClear { stage: 2 }))
        );
    }

    #[test]
    fn blitz_runs_120_seconds_regenerates_emptied_halves_and_ends_with_time_up() {
        let mut g = ready(MazeMode::BlitzFrenzy, 1);
        assert_eq!(g.blitz_left, BLITZ_SECS);
        // Empty the left half: it fills up again, including its orbs.
        let left_before = g.half_left[0];
        for y in 0..H as i32 {
            for x in 0..14 {
                if matches!(g.maze.get(x, y), Tile::Dot | Tile::PowerOrb) {
                    let orb = g.maze.get(x, y) == Tile::PowerOrb;
                    g.maze.set(x, y, Tile::Empty);
                    g.on_pellet(x, orb);
                }
            }
        }
        assert!(
            g.half_left[0] >= left_before - 1,
            "regenerated: {}",
            g.half_left[0]
        );
        assert_eq!(g.maze.get(1, 1), Tile::PowerOrb);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, MazeEvent::Regenerated { right: false }))
        );
        assert_eq!(g.stage, 1, "Blitz has no stage clear");
        assert!(g.core.is_none(), "no vibe cores in Blitz");
        // The countdown ends the session.
        g.blitz_left = 0.05;
        for _ in 0..20 {
            g.step(&idle());
        }
        assert_eq!(g.status, Status::TimeUp);
        assert!(g.drain_events().contains(&MazeEvent::TimeUp));
    }

    #[test]
    fn the_daily_maze_is_the_same_for_everyone_whatever_they_do() {
        let a = MazeGame::new(MazeMode::DailyVibe, 20261116);
        let mut b = MazeGame::new(MazeMode::DailyVibe, 20261116);
        assert_eq!(a.maze, b.maze);
        for _ in 0..600 {
            b.step(&input(Dir::Left));
        }
        let c = MazeGame::new(MazeMode::DailyVibe, 20261116);
        assert_eq!(a.base, c.base);
        assert_eq!(b.base, a.base, "playing never changes the course");
        assert_ne!(a.base, MazeGame::new(MazeMode::DailyVibe, 20261117).base);
        // The classic modes always use the hand-made maze.
        assert_eq!(MazeGame::new(MazeMode::Classic, 5).base, Maze::classic());
    }

    #[test]
    fn identical_inputs_give_identical_games_at_60_and_144_hz() {
        fn drive(hz: u32) -> Vec<(u32, u32, u64, u64)> {
            let mut g = ready(MazeMode::Zen, 4);
            let mut out = Vec::new();
            let per = hz / 12;
            let dt = 1.0 / hz as f32;
            for block in 0..120 {
                for f in 0..per {
                    let dir =
                        (f == 0).then(|| [Dir::Left, Dir::Up, Dir::Right, Dir::Down][block % 4]);
                    g.update(dt, &MazeInput { dir });
                }
                out.push((g.player.x.to_bits(), g.player.y.to_bits(), g.steps, g.score));
            }
            out
        }
        let (a, b) = (drive(60), drive(144));
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x, y, "block {i}");
        }
    }

    #[test]
    fn the_bot_plays_a_minute_of_every_mode_with_bounded_losses() {
        for (mode, seed, max_lost) in [
            (MazeMode::Classic, 1u64, 2u32),
            (MazeMode::Classic, 20261116, 2),
            (MazeMode::DailyVibe, 20261116, 2),
            (MazeMode::Zen, 7, 0),
            (MazeMode::BlitzFrenzy, 3, 2),
        ] {
            let mut g = ready(mode, seed);
            for _ in 0..(60 * 120) {
                let d = bot::choose(&g);
                g.step(&MazeInput { dir: d });
                if !g.is_playing() {
                    break;
                }
            }
            assert!(
                LIVES - g.lives <= max_lost,
                "{mode:?}/{seed}: lost {} lives",
                LIVES - g.lives
            );
            assert!(
                g.dots_eaten > 100,
                "{mode:?}/{seed}: only {} dots",
                g.dots_eaten
            );
            assert!(g.score > 1000);
        }
    }
}
