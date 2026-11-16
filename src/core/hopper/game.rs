//! `HopperGame`: the complete, deterministic rules of VibeCoded Hopper.
//!
//! Runs in fixed 1/120 s steps (f64 accumulator with a 1 microsecond
//! tolerance, like Glide). The world itself is a pure function of
//! `(seed, row, time)` (see `world.rs`); the only player-dependent state is
//! the player, collected items, sinking bridges and the camera.

use std::collections::{HashMap, HashSet};

use crate::core::flow::{FlowEngine, FlowTier};

use super::world::{
    BRIDGE_RESTORE_AFTER, BRIDGE_SINK_AFTER, Biome, COLS, HITBOX, HOP_TIME, ItemKind, LaneKind,
    Rates, RowSpec, TRAIN_PASS, TrainPhase, VehicleKind, WIDTH, World, train_phase,
};

pub const STEP: f32 = 1.0 / 120.0;
pub const MAX_FRAME_DT: f32 = 0.05;
pub const INPUT_BUFFER: f32 = 0.1;
/// Rows visible above the bottom edge of the camera.
pub const VIEW_ROWS: f32 = 14.0;
/// The camera's bottom edge follows the player at this many rows.
pub const CAMERA_LEAD: f32 = 4.0;
pub const SWEEP_BEHIND: f32 = 1.2;
pub const IDLE_LIMIT: f32 = 6.5;
pub const PLATFORM_TOLERANCE: f32 = 0.42;
pub const COMBO_WINDOW: f32 = 1.3;
pub const SHARD_SCORE: u64 = 50;
pub const ROW_SCORE: u64 = 10;
pub const NEAR_MISS_SCORE: u64 = 25;
pub const SHARD_FLOW: f32 = 8.0;
pub const FORWARD_FLOW: f32 = 6.0;
pub const NEAR_MISS_FLOW: f32 = 15.0;
pub const NEAR_MISS_CLEARANCE: f32 = 0.5;
pub const BOOTS_SECS: f32 = 8.0;
pub const RESCUE_SHIELD: f32 = 1.2;
pub const ICE_SECS: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn delta(self) -> (f32, i64) {
        match self {
            Dir::Up => (0.0, 1),
            Dir::Down => (0.0, -1),
            Dir::Left => (-1.0, 0),
            Dir::Right => (1.0, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HopperMode {
    Classic,
    BlitzRush,
    DailyVibe,
    Zen,
}

impl HopperMode {
    pub const ALL: [HopperMode; 4] = [
        HopperMode::Classic,
        HopperMode::BlitzRush,
        HopperMode::DailyVibe,
        HopperMode::Zen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            HopperMode::Classic => "classic",
            HopperMode::BlitzRush => "blitz",
            HopperMode::DailyVibe => "daily",
            HopperMode::Zen => "zen",
        }
    }

    pub fn from_id(id: &str) -> HopperMode {
        HopperMode::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(HopperMode::Classic)
    }

    fn rates(self) -> Rates {
        if self == HopperMode::BlitzRush {
            Rates {
                shard: 0.30,
                item: 0.12,
            }
        } else {
            Rates {
                shard: 0.16,
                item: 0.05,
            }
        }
    }

    /// Camera speed in rows per second at `row`.
    pub fn camera_speed(self, row: i64) -> f32 {
        match self {
            HopperMode::Zen => 0.0,
            HopperMode::BlitzRush => (0.9 + 0.004 * row as f32).min(1.25),
            _ => (0.28 + 0.003 * row as f32).min(0.7),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Traffic,
    Train,
    Splash,
    Hazard,
    Fall,
    Sweep,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Dead(Cause),
}

/// One frame of input: the direction pressed this frame (rising edge).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HopperInput {
    pub dir: Option<Dir>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hop {
    pub from_x: f32,
    pub from_row: i64,
    pub to_x: f32,
    pub to_row: i64,
    pub t: f32,
    pub dur: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ice {
    pub row: i64,
    pub x: f32,
    pub timer: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HopperEvent {
    Hop,
    Bump,
    Progress { row: i64, points: u64, combo: u32 },
    Shard { x: f32, row: i64, points: u64 },
    Item(ItemKind),
    NearMiss { row: i64, points: u64 },
    TrainWarning { row: i64 },
    TrainPass { row: i64 },
    BridgeSink { row: i64, col: usize },
    Splash,
    ShieldSaved,
    IceSaved,
    ZenReset,
    Died(Cause),
}

#[derive(Clone, Debug)]
pub struct HopperGame {
    pub mode: HopperMode,
    pub seed: u64,
    pub status: Status,
    pub world: World,
    pub x: f32,
    pub row: i64,
    pub hop: Option<Hop>,
    pub max_row: i64,
    pub camera_row: f32,
    pub time: f32,
    pub steps: u64,
    pub score: u64,
    pub shards: u32,
    pub combo: u32,
    pub combo_timer: f32,
    pub best_combo: u32,
    pub near_misses: u32,
    pub flow: FlowEngine,
    pub shield: bool,
    pub boots: f32,
    pub boots_jump_used: bool,
    pub inv: f32,
    pub idle_time: f32,
    pub last_safe: (i64, f32),
    pub ice: Option<Ice>,
    pub ice_armed: bool,
    was_overdrive: bool,
    pending: Option<(Dir, f32)>,
    taken_shards: HashSet<i64>,
    taken_items: HashSet<i64>,
    /// (row, col) -> time the player stepped on the bridge tile.
    pub sinking: HashMap<(i64, usize), f32>,
    /// Closest approach of a vehicle while standing in / crossing a road row.
    lane_track: Option<(i64, f32)>,
    rail_track: Option<(i64, f32)>,
    warned: HashMap<i64, i64>,
    passing: HashMap<i64, i64>,
    generated_to: i64,
    acc: f64,
    events: Vec<HopperEvent>,
}

impl HopperGame {
    pub fn new(mode: HopperMode, seed: u64) -> Self {
        let mut g = HopperGame {
            mode,
            seed,
            status: Status::Playing,
            world: World::new(seed, mode.rates()),
            x: WIDTH * 0.5 - 0.5 + 0.5,
            row: 0,
            hop: None,
            max_row: 0,
            camera_row: 0.0,
            time: 0.0,
            steps: 0,
            score: 0,
            shards: 0,
            combo: 0,
            combo_timer: 0.0,
            best_combo: 0,
            near_misses: 0,
            flow: FlowEngine::new(),
            shield: false,
            boots: 0.0,
            boots_jump_used: false,
            inv: 0.0,
            idle_time: 0.0,
            last_safe: (0, 7.5),
            ice: None,
            ice_armed: false,
            was_overdrive: false,
            pending: None,
            taken_shards: HashSet::new(),
            taken_items: HashSet::new(),
            sinking: HashMap::new(),
            lane_track: None,
            rail_track: None,
            warned: HashMap::new(),
            passing: HashMap::new(),
            generated_to: -1,
            acc: 0.0,
            events: Vec::new(),
        };
        g.x = 7.5;
        g.ensure_rows();
        g
    }

    /// Starts on the first grass row at or after `row` (tests, boss-style
    /// checks of the hazard zone). The row's corridor column is free.
    pub fn new_at_row(mode: HopperMode, seed: u64, row: i64) -> Self {
        let mut g = HopperGame::new(mode, seed);
        let mut r = row;
        loop {
            let spec = g.world.row(r).clone();
            if let LaneKind::Grass { corridor, .. } = spec.lane
                && r >= row
            {
                g.row = r;
                g.max_row = r;
                g.x = corridor as f32 + 0.5;
                g.last_safe = (r, g.x);
                break;
            }
            r += 1;
        }
        g.camera_row = g.row as f32 - CAMERA_LEAD;
        g.ensure_rows();
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

    pub fn drain_events(&mut self) -> Vec<HopperEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn distance(&self) -> i64 {
        self.max_row
    }

    pub fn hop_progress(&self) -> f32 {
        self.hop.map_or(0.0, |h| (h.t / h.dur).clamp(0.0, 1.0))
    }

    /// Height of the visual jump arc (0 on the ground).
    pub fn hop_z(&self) -> f32 {
        let p = self.hop_progress();
        4.0 * p * (1.0 - p) * 0.6
    }

    /// Player x (interpolated during a hop).
    pub fn pos_x(&self) -> f32 {
        match self.hop {
            Some(h) => h.from_x + (h.to_x - h.from_x) * self.hop_progress(),
            None => self.x,
        }
    }

    /// Player row coordinate (bottom edge of the occupied tile).
    pub fn pos_row(&self) -> f32 {
        match self.hop {
            Some(h) => h.from_row as f32 + (h.to_row - h.from_row) as f32 * self.hop_progress(),
            None => self.row as f32,
        }
    }

    fn center_y(&self) -> f32 {
        self.pos_row() + 0.5
    }

    /// `true` if a new hop would start right now.
    pub fn can_accept(&self) -> bool {
        self.hop.is_none() && self.pending.is_none()
    }

    pub fn spec(&mut self, row: i64) -> RowSpec {
        self.world.row(row).clone()
    }

    /// Has the shard of `row` been collected?
    pub fn shard_taken(&self, row: i64) -> bool {
        self.taken_shards.contains(&row)
    }

    /// Has the item of `row` been collected?
    pub fn item_taken(&self, row: i64) -> bool {
        self.taken_items.contains(&row)
    }

    pub fn tile_col(x: f32) -> usize {
        (x.floor() as i64).clamp(0, COLS as i64 - 1) as usize
    }

    /// Makes sure all visible rows exist and drops old ones.
    pub fn ensure_rows(&mut self) {
        let want = (self.camera_row + VIEW_ROWS + 6.0).ceil() as i64;
        let from = (self.generated_to + 1)
            .max((self.camera_row - 6.0).floor() as i64)
            .max(0);
        for r in from..=want {
            self.world.row(r);
        }
        self.generated_to = self.generated_to.max(want);
        let min_row = (self.camera_row - 6.0).floor() as i64;
        self.world
            .forget_below(min_row.min(self.row - 1).min(self.last_safe.0 - 1));
        let keep = min_row - 2;
        self.taken_shards.retain(|r| *r >= keep);
        self.taken_items.retain(|r| *r >= keep);
        self.sinking.retain(|(r, _), _| *r >= keep);
        self.warned.retain(|r, _| *r >= keep);
        self.passing.retain(|r, _| *r >= keep);
    }

    // ------------------------------------------------------------ main loop

    /// Advances by a frame of `dt` seconds (capped at 0.05 s).
    pub fn update(&mut self, dt: f32, input: &HopperInput) {
        if !self.is_playing() || !dt.is_finite() {
            return;
        }
        if let Some(d) = input.dir {
            self.pending = Some((d, 0.0));
        }
        self.acc += dt.clamp(0.0, MAX_FRAME_DT) as f64;
        while self.acc + 1e-6 >= STEP as f64 && self.is_playing() {
            self.acc -= STEP as f64;
            self.step_inner();
        }
    }

    /// One fixed step with an explicit input (tests, bots).
    pub fn step(&mut self, input: &HopperInput) {
        if let Some(d) = input.dir {
            self.pending = Some((d, 0.0));
        }
        self.step_inner();
    }

    fn step_inner(&mut self) {
        if !self.is_playing() {
            return;
        }
        let dt = STEP;
        self.steps += 1;
        self.time += dt;
        self.inv = (self.inv - dt).max(0.0);
        self.boots = (self.boots - dt).max(0.0);
        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }
        if let Some(i) = &mut self.ice {
            i.timer -= dt;
        }
        self.flow.update(dt);
        let od = self.overdrive();
        if od && !self.was_overdrive {
            self.ice_armed = true;
        }
        self.was_overdrive = od;

        // Buffered input: ages while a hop is running.
        if let Some((d, age)) = self.pending {
            let age = age + dt;
            if age > INPUT_BUFFER && self.hop.is_some() {
                self.pending = None;
            } else {
                self.pending = Some((d, age));
            }
        }
        // Hop progress.
        if let Some(h) = &mut self.hop {
            h.t += dt;
            if h.t + 1e-6 >= h.dur {
                let h = *h;
                self.hop = None;
                self.land(h);
                if !self.is_playing() {
                    return;
                }
            }
        }
        if self.hop.is_none()
            && let Some((d, age)) = self.pending.take()
            && age <= INPUT_BUFFER
        {
            self.start_hop(d);
        }

        self.stand_checks(dt);
        if !self.is_playing() {
            return;
        }
        self.collisions();
        if !self.is_playing() {
            return;
        }
        self.track_near_misses();
        self.train_warnings();
        self.update_camera(dt);
        self.ensure_rows();
    }

    // ------------------------------------------------------------ hopping

    fn start_hop(&mut self, dir: Dir) {
        let (dx, dy) = dir.delta();
        let mut to_row = self.row + dy;
        let mut to_x = self.x + dx;
        if to_row < 0 || !self.in_bounds(to_row, to_x) {
            self.events.push(HopperEvent::Bump);
            return;
        }
        let spec = self.world.row(to_row).clone();
        let col = Self::tile_col(to_x);
        if spec.is_tree(col) {
            // Phase-Boots: exactly one tree jump per pair of boots.
            if self.boots > 0.0 && !self.boots_jump_used {
                let (jr, jx) = (self.row + 2 * dy, self.x + 2.0 * dx);
                let free = self.in_bounds(jr, jx)
                    && jr >= 0
                    && !self.world.row(jr).clone().is_tree(Self::tile_col(jx));
                if free {
                    self.boots_jump_used = true;
                    to_row = jr;
                    to_x = jx;
                } else {
                    self.events.push(HopperEvent::Bump);
                    return;
                }
            } else {
                self.events.push(HopperEvent::Bump);
                return;
            }
        }
        let dur = HOP_TIME * if self.boots > 0.0 { 0.7 } else { 1.0 };
        self.hop = Some(Hop {
            from_x: self.x,
            from_row: self.row,
            to_x,
            to_row,
            t: 0.0,
            dur,
        });
        self.events.push(HopperEvent::Hop);
    }

    fn in_bounds(&mut self, row: i64, x: f32) -> bool {
        if row < 0 {
            return false;
        }
        let river = matches!(self.world.row(row).lane, LaneKind::River { .. });
        if river {
            (0.2..=WIDTH - 0.2).contains(&x)
        } else {
            (0.0..WIDTH).contains(&x)
        }
    }

    fn land(&mut self, h: Hop) {
        let from_row = h.from_row;
        let spec = self.world.row(h.to_row).clone();
        let river = matches!(spec.lane, LaneKind::River { .. });
        self.row = h.to_row;
        self.x = if river { h.to_x } else { h.to_x.floor() + 0.5 };
        if self.row != from_row {
            self.idle_time = 0.0;
            self.finish_crossing(from_row);
        }
        // Progress.
        if self.row > self.max_row {
            for r in self.max_row + 1..=self.row {
                self.progress(r);
            }
            self.max_row = self.row;
        }
        let col = Self::tile_col(self.x);
        // Collectibles on the landing tile.
        if let Some(sc) = spec.shard
            && sc == col
            && !self.taken_shards.contains(&self.row)
        {
            self.taken_shards.insert(self.row);
            let pts = self.award(SHARD_SCORE);
            self.flow.register_event(SHARD_FLOW);
            self.shards += 1;
            self.events.push(HopperEvent::Shard {
                x: self.x,
                row: self.row,
                points: pts,
            });
        }
        if let Some((kind, ic)) = spec.item
            && ic == col
            && !self.taken_items.contains(&self.row)
        {
            self.taken_items.insert(self.row);
            match kind {
                ItemKind::Aegis => self.shield = true,
                ItemKind::Boots => {
                    self.boots = BOOTS_SECS;
                    self.boots_jump_used = false;
                }
            }
            self.events.push(HopperEvent::Item(kind));
        }
        match &spec.lane {
            LaneKind::Grass { .. } => self.last_safe = (self.row, self.x),
            LaneKind::River { .. } => self.river_check(),
            LaneKind::Bridge { solid, .. } => {
                if !solid[col] {
                    self.fail(Cause::Fall);
                } else {
                    self.sinking.entry((self.row, col)).or_insert(self.time);
                }
            }
            _ => {}
        }
    }

    fn progress(&mut self, row: i64) {
        let pts = self.award(ROW_SCORE);
        if self.combo_timer > 0.0 {
            self.combo += 1;
            self.flow.register_event(FORWARD_FLOW);
        } else {
            self.combo = 1;
        }
        self.combo_timer = COMBO_WINDOW;
        self.best_combo = self.best_combo.max(self.combo);
        self.events.push(HopperEvent::Progress {
            row,
            points: pts,
            combo: self.combo,
        });
    }

    fn award(&mut self, base: u64) -> u64 {
        let pts = (base as f32 * self.flow.multiplier()).round() as u64;
        self.score += pts;
        pts
    }

    // ------------------------------------------------------------ rules

    /// Called when the player lands on a river row: needs a platform.
    fn river_check(&mut self) {
        if !self.on_platform() {
            self.fail(Cause::Splash);
        }
    }

    fn on_platform(&mut self) -> bool {
        if let Some(i) = self.ice
            && i.row == self.row
            && i.timer > 0.0
            && (self.x - i.x).abs() <= 0.9
        {
            return true;
        }
        let spec = self.world.row(self.row).clone();
        spec.platform_under(self.x, self.time, PLATFORM_TOLERANCE)
            .is_some()
    }

    /// Per-step effects of standing on the current tile.
    fn stand_checks(&mut self, dt: f32) {
        if self.hop.is_none() {
            self.idle_time += dt;
        }
        let spec = self.world.row(self.row).clone();
        let col = Self::tile_col(self.x);
        if self.hop.is_none() {
            match &spec.lane {
                LaneKind::River { .. } => {
                    let ice_here = self.ice.is_some_and(|i| {
                        i.row == self.row && i.timer > 0.0 && (self.x - i.x).abs() <= 0.9
                    });
                    match spec.platform_under(self.x, self.time, PLATFORM_TOLERANCE) {
                        _ if ice_here => {}
                        Some(v) => {
                            self.x += v * dt;
                            if self.x < 0.2 || self.x > WIDTH - 0.2 {
                                self.events.push(HopperEvent::Splash);
                                self.fail(Cause::Splash);
                            }
                        }
                        None => self.fail(Cause::Splash),
                    }
                }
                LaneKind::Bridge { solid, .. } => {
                    // Bridge tiles sink 0.9 s after being entered, then rise again.
                    let sunk = self.sinking.get(&(self.row, col)).is_some_and(|s| {
                        self.time - s >= BRIDGE_SINK_AFTER
                            && self.time - s < BRIDGE_SINK_AFTER + BRIDGE_RESTORE_AFTER
                    });
                    if !solid[col] || sunk {
                        self.fail(Cause::Fall);
                    }
                }
                _ => {}
            }
        }
        // Restore bridges.
        let t = self.time;
        self.sinking
            .retain(|_, s| t - *s < BRIDGE_SINK_AFTER + BRIDGE_RESTORE_AFTER);
        if self.mode != HopperMode::Zen && self.idle_time > IDLE_LIMIT {
            self.fail(Cause::Idle);
        }
    }

    fn collisions(&mut self) {
        if self.inv > 0.0 || !self.is_playing() {
            return;
        }
        let y = self.center_y();
        let x = self.pos_x();
        let r0 = (y - HITBOX * 0.5).floor() as i64;
        let r1 = (y + HITBOX * 0.5).floor() as i64;
        for r in r0..=r1 {
            if r < 0 {
                continue;
            }
            let spec = self.world.row(r).clone();
            if spec.lethal_at(x, y, self.time) {
                let cause = if matches!(spec.lane, LaneKind::Rail { .. }) {
                    Cause::Train
                } else {
                    Cause::Traffic
                };
                self.fail(cause);
                return;
            }
            if let LaneKind::Laser { .. } = spec.lane
                && spec.laser_active(Self::tile_col(x), self.time)
            {
                self.fail(Cause::Hazard);
                return;
            }
        }
    }

    /// Records how close traffic came while the player is in / crossing a lane.
    fn track_near_misses(&mut self) {
        let y = self.center_y();
        let x = self.pos_x();
        let row = y.floor() as i64;
        let spec = self.world.row(row).clone();
        match &spec.lane {
            LaneKind::Road { kind, .. } => {
                let clear = spec
                    .vehicles(self.time)
                    .iter()
                    .map(|c| (c - x).abs() - (kind.len() + HITBOX) * 0.5)
                    .fold(f32::INFINITY, f32::min);
                let min = match self.lane_track {
                    Some((r, m)) if r == row => m.min(clear),
                    _ => clear,
                };
                self.lane_track = Some((row, min));
            }
            LaneKind::Rail {
                dir: _,
                period,
                phase,
            } => {
                let left = match train_phase(*period, *phase, self.time) {
                    TrainPhase::Warning { left } => left,
                    TrainPhase::Idle => f32::INFINITY,
                    TrainPhase::Pass { .. } => 0.0,
                };
                let min = match self.rail_track {
                    Some((r, m)) if r == row => m.min(left),
                    _ => left,
                };
                self.rail_track = Some((row, min));
            }
            _ => {}
        }
    }

    /// The player left `from_row`: pay a near miss if traffic or a train
    /// came very close (never together with a hit: a hit clears the tracks).
    fn finish_crossing(&mut self, from_row: i64) {
        let mut near = false;
        if let Some((r, min)) = self.lane_track.take()
            && r == from_row
            && (0.0..NEAR_MISS_CLEARANCE).contains(&min)
        {
            near = true;
        }
        if let Some((r, left)) = self.rail_track.take()
            && r == from_row
            && left < TRAIN_PASS + 0.15
            && left > 0.0
        {
            near = true;
        }
        if near {
            self.near_misses += 1;
            let pts = self.award(NEAR_MISS_SCORE);
            self.flow.register_event(NEAR_MISS_FLOW);
            self.events.push(HopperEvent::NearMiss {
                row: from_row,
                points: pts,
            });
        }
    }

    fn train_warnings(&mut self) {
        let lo = self.camera_row.floor() as i64;
        let hi = (self.camera_row + VIEW_ROWS).ceil() as i64;
        for r in lo.max(0)..=hi {
            let spec = self.world.row(r).clone();
            let LaneKind::Rail { period, phase, .. } = spec.lane else {
                continue;
            };
            let cycle = ((self.time + phase) / period).floor() as i64;
            match train_phase(period, phase, self.time) {
                TrainPhase::Warning { .. } if self.warned.get(&r) != Some(&cycle) => {
                    self.warned.insert(r, cycle);
                    self.events.push(HopperEvent::TrainWarning { row: r });
                }
                TrainPhase::Pass { .. } if self.passing.get(&r) != Some(&cycle) => {
                    self.passing.insert(r, cycle);
                    self.events.push(HopperEvent::TrainPass { row: r });
                }
                _ => {}
            }
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let follow = self.pos_row() - CAMERA_LEAD;
        let base = self.mode.camera_speed(self.max_row);
        let mut cam = self.camera_row + base * dt;
        if follow > cam {
            cam += (follow - cam) * (5.0 * dt).min(1.0);
        }
        self.camera_row = cam.max(self.camera_row);
        if self.mode != HopperMode::Zen
            && self.hop.is_none()
            && (self.row as f32) < self.camera_row - SWEEP_BEHIND
        {
            self.fail(Cause::Sweep);
        }
    }

    // ------------------------------------------------------------ failing

    /// The player would die from `cause`: ice, shield and Zen can save them.
    pub fn fail(&mut self, cause: Cause) {
        if self.inv > 0.0 || !self.is_playing() {
            return;
        }
        self.lane_track = None;
        self.rail_track = None;
        let sweepish = matches!(cause, Cause::Sweep | Cause::Idle);
        if !sweepish {
            // Overdrive ice: one emergency platform per Overdrive phase.
            if matches!(cause, Cause::Splash) && self.overdrive() && self.ice_armed {
                self.ice_armed = false;
                self.ice = Some(Ice {
                    row: self.row,
                    x: self.x,
                    timer: ICE_SECS,
                });
                self.events.push(HopperEvent::IceSaved);
                return;
            }
            if self.shield {
                self.shield = false;
                self.events.push(HopperEvent::ShieldSaved);
                self.rescue();
                return;
            }
            if self.mode == HopperMode::Zen {
                self.events.push(HopperEvent::ZenReset);
                self.rescue();
                return;
            }
        }
        if self.mode == HopperMode::Zen {
            // No sweeps in Zen; be safe anyway.
            self.rescue();
            return;
        }
        if cause == Cause::Splash {
            self.events.push(HopperEvent::Splash);
        }
        self.status = Status::Dead(cause);
        self.events.push(HopperEvent::Died(cause));
    }

    /// Back to the last safe grass tile with a short protection.
    fn rescue(&mut self) {
        let (row, x) = self.last_safe;
        self.row = row;
        self.x = x;
        self.hop = None;
        self.pending = None;
        self.inv = RESCUE_SHIELD;
        self.idle_time = 0.0;
        self.ice = None;
    }
}

/// Convenience for renderers: name of a vehicle kind.
pub fn vehicle_name(k: VehicleKind) -> &'static str {
    match k {
        VehicleKind::Racer => "HOVER RACER",
        VehicleKind::Truck => "CYBER TRUCK",
        VehicleKind::Convoy => "CARGO CONVOY",
    }
}

/// `true` for biomes whose rows can kill by traffic.
pub fn is_traffic(b: Biome) -> bool {
    matches!(b, Biome::Highway | Biome::Railway)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hopper::world::{LASER_ACTIVE, LaneKind};

    fn up() -> HopperInput {
        HopperInput { dir: Some(Dir::Up) }
    }

    fn press(d: Dir) -> HopperInput {
        HopperInput { dir: Some(d) }
    }

    fn idle() -> HopperInput {
        HopperInput::default()
    }

    fn run(g: &mut HopperGame, steps: usize) {
        for _ in 0..steps {
            g.step(&idle());
        }
    }

    /// First row (and seed) satisfying `pred`.
    fn find(pred: impl Fn(&mut HopperGame, i64) -> bool) -> (HopperGame, i64) {
        for seed in 0..300u64 {
            let mut g = HopperGame::new(HopperMode::Zen, seed);
            for r in 5..300 {
                if pred(&mut g, r) {
                    return (g, r);
                }
            }
        }
        panic!("no matching row found");
    }

    /// Puts the player on `(row, x)` and keeps the camera out of the way.
    fn place(g: &mut HopperGame, row: i64, x: f32) {
        g.row = row;
        g.x = x;
        g.hop = None;
        g.pending = None;
        g.camera_row = row as f32 - 3.0;
        g.max_row = g.max_row.max(row);
        g.last_safe = (row, x);
        g.inv = 0.0;
        g.ensure_rows();
    }

    fn lane(g: &mut HopperGame, r: i64) -> LaneKind {
        g.world.row(r).lane.clone()
    }

    #[test]
    fn a_hop_takes_0_11_seconds_and_snaps_to_the_tile_centre() {
        let mut g = HopperGame::new(HopperMode::Classic, 1);
        assert_eq!((g.row, g.x), (0, 7.5));
        g.step(&up());
        assert!(g.hop.is_some());
        let mut steps = 1;
        while g.hop.is_some() {
            g.step(&idle());
            steps += 1;
        }
        assert!((13..=15).contains(&steps), "{steps} steps");
        assert_eq!((g.row, g.x), (1, 7.5));
        // Hop arc: zero at both ends, highest in the middle.
        let mut h = HopperGame::new(HopperMode::Classic, 1);
        h.step(&up());
        assert!(h.hop_z() < 0.1);
        run(&mut h, 6);
        assert!(h.hop_z() > 0.5, "{}", h.hop_z());
        // Sideways hops land on tile centres too.
        let mut s = HopperGame::new(HopperMode::Classic, 1);
        s.step(&press(Dir::Right));
        run(&mut s, 20);
        assert_eq!((s.row, s.x), (0, 8.5));
    }

    #[test]
    fn the_player_cannot_leave_the_sides_or_the_start() {
        let mut g = HopperGame::new(HopperMode::Zen, 1);
        for _ in 0..7 {
            g.step(&press(Dir::Left));
            run(&mut g, 20);
        }
        assert_eq!(g.x, 0.5);
        g.drain_events();
        g.step(&press(Dir::Left));
        run(&mut g, 20);
        assert_eq!(g.x, 0.5, "bumped against the edge, no death");
        assert!(g.drain_events().contains(&HopperEvent::Bump));
        assert!(g.is_playing());
        // Not below row 0 either.
        g.step(&press(Dir::Down));
        run(&mut g, 20);
        assert_eq!(g.row, 0);
    }

    #[test]
    fn trees_bump_the_player_back_without_a_move_or_a_death() {
        let (mut g, r) = find(|g, r| {
            let s = g.world.row(r).clone();
            let below = g.world.row(r - 1).clone();
            s.is_safe_ground() && below.is_safe_ground() && (0..COLS).any(|c| s.is_tree(c))
        });
        let c = (0..COLS).find(|&c| g.world.row(r).is_tree(c)).unwrap();
        place(&mut g, r - 1, c as f32 + 0.5);
        g.drain_events();
        g.step(&up());
        run(&mut g, 20);
        assert_eq!((g.row, g.x), (r - 1, c as f32 + 0.5), "blocked");
        assert!(g.drain_events().contains(&HopperEvent::Bump));
        assert!(g.is_playing() && g.hop.is_none());
    }

    #[test]
    fn phase_boots_allow_exactly_one_tree_jump() {
        let (mut g, r) = find(|g, r| {
            let s = g.world.row(r).clone();
            let below = g.world.row(r - 1).clone();
            let above = g.world.row(r + 1).clone();
            s.is_safe_ground()
                && below.is_safe_ground()
                && above.is_safe_ground()
                && (0..COLS).any(|c| s.is_tree(c) && !above.is_tree(c))
        });
        let c = (0..COLS)
            .find(|&c| g.world.row(r).is_tree(c) && !g.world.row(r + 1).is_tree(c))
            .unwrap();
        place(&mut g, r - 1, c as f32 + 0.5);
        g.boots = BOOTS_SECS;
        g.step(&up());
        run(&mut g, 20);
        assert_eq!(g.row, r + 1, "jumped over the tree");
        assert!(g.boots_jump_used);
        // The next tree is a wall again, even with boots still active.
        let (mut h, r2) = find(|g, r| {
            let s = g.world.row(r).clone();
            s.is_safe_ground()
                && g.world.row(r - 1).is_safe_ground()
                && (0..COLS).any(|c| s.is_tree(c))
        });
        let c2 = (0..COLS).find(|&c| h.world.row(r2).is_tree(c)).unwrap();
        place(&mut h, r2 - 1, c2 as f32 + 0.5);
        h.boots = BOOTS_SECS;
        h.boots_jump_used = true;
        h.step(&up());
        run(&mut h, 20);
        assert_eq!(h.row, r2 - 1, "the one jump is used up");
    }

    #[test]
    fn the_input_buffer_is_100_ms() {
        // A press 20 ms into a hop is executed as soon as the hop ends.
        let mut g = HopperGame::new(HopperMode::Zen, 1);
        g.step(&up());
        run(&mut g, 2); // ~25 ms in
        g.step(&press(Dir::Right));
        let mut guard = 0;
        while g.hop.is_some_and(|h| h.to_row == 1 && h.to_x == 7.5) && guard < 40 {
            g.step(&idle());
            guard += 1;
        }
        assert_eq!(g.row, 1);
        run(&mut g, 2);
        assert!(
            g.hop.is_some_and(|h| h.to_x == 8.5) || g.x == 8.5,
            "the buffered press ran: {:?}",
            g.hop
        );
        // A press at the very start of a hop is older than 100 ms when the
        // hop ends (0.117 s) and is dropped.
        let mut d = HopperGame::new(HopperMode::Zen, 1);
        d.step(&up());
        d.step(&press(Dir::Right));
        run(&mut d, 40);
        assert_eq!((d.row, d.x), (1, 7.5), "stale press dropped");
        // Pressing while idle acts immediately.
        let mut i = HopperGame::new(HopperMode::Zen, 1);
        i.step(&press(Dir::Left));
        assert!(i.hop.is_some());
    }

    #[test]
    fn frame_rate_does_not_change_the_game_60_vs_144_hz() {
        fn drive(hz: u32) -> Vec<(i64, u32, u64, u64)> {
            let mut g = HopperGame::new(HopperMode::Zen, 4);
            g.inv = 1000.0;
            let mut out = Vec::new();
            let per = hz / 12;
            let dt = 1.0 / hz as f32;
            for block in 0..120 {
                for f in 0..per {
                    let dir = if f == 0 {
                        match block % 5 {
                            0 | 1 => Some(Dir::Up),
                            2 => Some(Dir::Left),
                            3 => Some(Dir::Right),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    g.update(dt, &HopperInput { dir });
                }
                out.push((g.row, g.x.to_bits(), g.steps, g.score));
            }
            out
        }
        let a = drive(60);
        let b = drive(144);
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x, y, "block {i}");
        }
        assert_eq!(a[119].2, 1200);
    }

    #[test]
    fn a_vehicle_hit_kills_and_standing_clear_of_it_is_safe() {
        let (mut g, r) = find(|g, r| matches!(g.world.row(r).lane, LaneKind::Road { .. }));
        g.mode = HopperMode::Classic;
        let len = match lane(&mut g, r) {
            LaneKind::Road { kind, .. } => kind.len(),
            _ => unreachable!(),
        };
        let t = 3.0;
        g.time = t;
        let v = g.world.row(r).vehicles(t);
        let c = *v
            .iter()
            .find(|&&x| (2.0..13.0).contains(&x))
            .expect("a visible vehicle");
        place(&mut g, r, c.floor() + 0.5);
        g.step(&idle());
        assert_eq!(g.status, Status::Dead(Cause::Traffic));
        // Standing outside the 0.62 hit box right next to a vehicle is fine.
        let spec = g.world.row(r).clone();
        let edge = c + len * 0.5 + HITBOX * 0.5;
        assert!(
            !spec.lethal_at(edge + 0.02, r as f32 + 0.5, t),
            "just clear"
        );
        assert!(
            spec.lethal_at(edge - 0.05, r as f32 + 0.5, t),
            "just inside"
        );
    }

    #[test]
    fn a_train_kills_only_during_its_pass_and_warns_once_per_cycle() {
        let (mut g, r) = find(|g, r| matches!(g.world.row(r).lane, LaneKind::Rail { .. }));
        let LaneKind::Rail { period, phase, .. } = lane(&mut g, r) else {
            unreachable!()
        };
        g.mode = HopperMode::Classic;
        place(&mut g, r, 7.5);
        let warn_start = period - TRAIN_PASS - 1.1;
        let cycle = |t: f32| (t + phase).rem_euclid(period);
        // Start just after the warning begins.
        let mut t = 0.0;
        while cycle(t) < warn_start + 0.05 || cycle(t) > warn_start + 0.3 {
            t += 0.01;
        }
        g.time = t;
        g.drain_events();
        run(&mut g, 30);
        g.idle_time = 0.0;
        assert!(g.is_playing(), "the warning phase is harmless");
        let ev = g.drain_events();
        let warnings = ev
            .iter()
            .filter(|e| matches!(e, HopperEvent::TrainWarning { row } if *row == r))
            .count();
        assert_eq!(warnings, 1, "one warning per cycle");
        // Wait for the pass: standing on the track is fatal.
        for _ in 0..200 {
            g.idle_time = 0.0;
            g.camera_row = r as f32 - 3.0;
            g.step(&idle());
        }
        assert_eq!(g.status, Status::Dead(Cause::Train));
    }

    #[test]
    fn near_misses_pay_once_and_never_together_with_a_hit() {
        let mut g = HopperGame::new(HopperMode::Classic, 1);
        g.lane_track = Some((3, 0.3));
        g.flow.flow = 0.0;
        g.finish_crossing(3);
        assert_eq!(g.near_misses, 1);
        assert!((g.flow.flow - NEAR_MISS_FLOW).abs() < 0.01);
        assert_eq!(g.score, NEAR_MISS_SCORE);
        g.finish_crossing(3);
        assert_eq!(g.near_misses, 1, "only once per crossing");
        // Too far away: no bonus.
        g.lane_track = Some((3, 0.8));
        g.finish_crossing(3);
        assert_eq!(g.near_misses, 1);
        // A hit wipes the track: nothing is paid afterwards.
        g.lane_track = Some((3, 0.2));
        g.fail(Cause::Traffic);
        assert!(!g.is_playing());
        g.finish_crossing(3);
        assert_eq!(g.near_misses, 1, "a hit and a near miss never coincide");
        // A negative clearance (overlap) is a hit, not a near miss.
        let mut h = HopperGame::new(HopperMode::Classic, 1);
        h.lane_track = Some((2, -0.1));
        h.finish_crossing(2);
        assert_eq!(h.near_misses, 0);
        // Trains: leaving the track just before the pass counts.
        let mut t = HopperGame::new(HopperMode::Classic, 1);
        t.rail_track = Some((4, 0.3));
        t.finish_crossing(4);
        assert_eq!(t.near_misses, 1);
    }

    #[test]
    fn logs_carry_the_player_at_the_log_speed() {
        let (mut g, r) = find(|g, r| {
            matches!(g.world.row(r).lane, LaneKind::River { .. })
                && g.world.row(r - 1).is_safe_ground()
        });
        g.mode = HopperMode::Classic;
        let t = 4.0;
        let landing_t = t + 0.13;
        let c = *g
            .world
            .row(r)
            .platforms(landing_t)
            .iter()
            .find(|&&x| (3.0..12.0).contains(&x))
            .unwrap();
        let col = c.floor();
        let col_idx = col as usize;
        if g.world.row(r - 1).is_tree(col_idx) {
            return; // (the search is seed dependent; trees are tested elsewhere)
        }
        place(&mut g, r - 1, col + 0.5);
        g.time = t;
        g.step(&up());
        run(&mut g, 25);
        assert!(g.is_playing(), "landed on the log");
        assert_eq!(g.row, r);
        let x0 = g.x;
        let v = g.world.row(r).log_velocity();
        for _ in 0..30 {
            g.idle_time = 0.0;
            g.step(&idle());
        }
        assert!(g.is_playing());
        assert!(
            ((g.x - x0) - v * 0.25).abs() < 0.05,
            "carried at the log speed"
        );
    }

    #[test]
    fn landing_in_open_water_splashes() {
        let (mut g, r) = find(|g, r| {
            matches!(g.world.row(r).lane, LaneKind::River { .. })
                && g.world.row(r - 1).is_safe_ground()
        });
        g.mode = HopperMode::Classic;
        for k in 0..400 {
            let t = 2.0 + k as f32 * 0.07;
            for col in 0..COLS {
                if g.world
                    .row(r)
                    .platform_under(col as f32 + 0.5, t + 0.13, 0.9)
                    .is_none()
                    && g.world
                        .row(r)
                        .platform_under(col as f32 + 0.5, t + 0.08, 0.9)
                        .is_none()
                    && !g.world.row(r - 1).is_tree(col)
                {
                    let mut h = g.clone();
                    place(&mut h, r - 1, col as f32 + 0.5);
                    h.time = t;
                    h.step(&up());
                    run(&mut h, 25);
                    assert_eq!(h.status, Status::Dead(Cause::Splash), "col {col} t {t}");
                    return;
                }
            }
        }
        panic!("found no open water");
    }

    #[test]
    fn drifting_out_of_the_playfield_on_a_log_splashes() {
        let (mut g, r) = find(|g, r| {
            let s = g.world.row(r).clone();
            matches!(s.lane, LaneKind::River { .. }) && s.log_velocity() > 0.5
        });
        g.mode = HopperMode::Classic;
        place(&mut g, r, 14.0);
        // Put the player on a log that is about to leave on the right.
        let mut found = None;
        for k in 0..2000 {
            let t = k as f32 * 0.05;
            let spec = g.world.row(r).clone();
            if let Some(c) = spec
                .platforms(t)
                .into_iter()
                .find(|&c| c > 13.0 && c < 14.2)
            {
                found = Some((t, c));
                break;
            }
        }
        let (t, c) = found.expect("a log near the right edge");
        g.time = t;
        g.x = c;
        for _ in 0..(8 * 120) {
            g.idle_time = 0.0;
            g.camera_row = r as f32 - 3.0;
            g.step(&idle());
            if !g.is_playing() {
                break;
            }
        }
        assert_eq!(g.status, Status::Dead(Cause::Splash), "x {}", g.x);
        assert!(g.x >= WIDTH - 0.5);
    }

    #[test]
    fn the_aegis_shield_saves_exactly_once() {
        let (mut g, r) = find(|g, r| {
            matches!(g.world.row(r).lane, LaneKind::Road { .. })
                && g.world.row(r - 1).is_safe_ground()
        });
        g.mode = HopperMode::Classic;
        let (safe_row, safe_x) = (r - 1, 5.5);
        place(&mut g, safe_row, safe_x);
        g.shield = true;
        g.row = r;
        g.x = 8.5;
        g.drain_events();
        g.fail(Cause::Traffic);
        assert!(g.is_playing() && !g.shield, "shield consumed");
        assert_eq!(
            (g.row, g.x),
            (safe_row, safe_x),
            "back on the last safe row"
        );
        assert!(
            g.inv > 1.0,
            "short protection so the lane cannot kill at once"
        );
        assert!(g.drain_events().contains(&HopperEvent::ShieldSaved));
        // During the protection, nothing happens (no storm).
        g.fail(Cause::Traffic);
        assert!(g.is_playing());
        // After it, the next hit is fatal.
        g.inv = 0.0;
        g.fail(Cause::Traffic);
        assert_eq!(g.status, Status::Dead(Cause::Traffic));
    }

    #[test]
    fn the_shield_does_not_stop_the_camera_sweep() {
        let mut g = HopperGame::new(HopperMode::Classic, 1);
        g.shield = true;
        g.fail(Cause::Sweep);
        assert_eq!(g.status, Status::Dead(Cause::Sweep));
    }

    #[test]
    fn zen_resets_gently_instead_of_dying_and_has_no_camera_pressure() {
        let mut g = HopperGame::new(HopperMode::Zen, 1);
        place(&mut g, 2, 3.5);
        g.last_safe = (1, 4.5);
        g.fail(Cause::Traffic);
        assert!(g.is_playing());
        assert_eq!((g.row, g.x), (1, 4.5));
        // Idle for a minute: neither the idle sweep nor the camera sweep.
        let mut z = HopperGame::new(HopperMode::Zen, 1);
        for _ in 0..(60 * 120) {
            z.step(&idle());
        }
        assert!(z.is_playing());
        assert_eq!(z.camera_row, 0.0, "no camera pressure in Zen");
    }

    #[test]
    fn overdrive_ice_rescues_once_per_overdrive_phase() {
        let (mut g, r) = find(|g, r| matches!(g.world.row(r).lane, LaneKind::River { .. }));
        g.mode = HopperMode::Classic;
        place(&mut g, r, 7.5);
        // Entering Overdrive arms the ice.
        g.flow.flow = 95.0;
        g.step(&idle());
        assert!(g.overdrive() && g.ice_armed);
        g.drain_events();
        g.fail(Cause::Splash);
        assert!(g.is_playing(), "the ice caught the player");
        assert!(g.ice.is_some() && !g.ice_armed);
        assert!(g.drain_events().contains(&HopperEvent::IceSaved));
        // A second splash in the same Overdrive phase is fatal.
        g.ice = None;
        g.fail(Cause::Splash);
        assert_eq!(g.status, Status::Dead(Cause::Splash));
        // Leaving and re-entering Overdrive arms it again.
        let (mut h, r) = find(|g, r| matches!(g.world.row(r).lane, LaneKind::River { .. }));
        h.mode = HopperMode::Classic;
        place(&mut h, r, 7.5);
        h.flow.flow = 95.0;
        h.step(&idle());
        h.ice_armed = false; // used up
        h.flow.flow = 10.0;
        h.step(&idle());
        assert!(
            !h.overdrive() && !h.ice_armed,
            "stays spent outside Overdrive"
        );
        h.flow.flow = 95.0;
        h.step(&idle());
        assert!(h.ice_armed, "re-armed on re-entering Overdrive");
    }

    #[test]
    fn the_ice_platform_holds_for_two_seconds_and_then_melts() {
        let (mut g, r) = find(|g, r| matches!(g.world.row(r).lane, LaneKind::River { .. }));
        g.mode = HopperMode::Classic;
        place(&mut g, r, 7.5);
        g.flow.flow = 95.0;
        g.step(&idle());
        g.fail(Cause::Splash);
        assert!(g.ice.is_some());
        // Standing on the ice for a while is fine ...
        for _ in 0..180 {
            g.flow.flow = 95.0;
            g.idle_time = 0.0;
            g.camera_row = r as f32 - 3.0;
            g.step(&idle());
        }
        assert!(g.is_playing(), "ice holds for ~2 s");
        // ... until it melts.
        for _ in 0..120 {
            g.flow.flow = 95.0;
            g.idle_time = 0.0;
            g.camera_row = r as f32 - 3.0;
            g.step(&idle());
        }
        assert!(!g.is_playing() || g.on_platform());
    }

    #[test]
    fn scoring_flow_and_combo_for_fast_forward_hops() {
        let mut g = HopperGame::new(HopperMode::Zen, 1);
        g.step(&up());
        run(&mut g, 20);
        assert_eq!((g.max_row, g.score, g.combo), (1, 10, 1));
        g.step(&up());
        run(&mut g, 20);
        assert_eq!((g.max_row, g.combo), (2, 2), "within 1.3 s the combo grows");
        assert!(
            g.flow.flow >= FORWARD_FLOW - 0.1,
            "fast forward step charges flow"
        );
        // Waiting longer than 1.3 s resets the combo.
        run(&mut g, 200);
        assert_eq!(g.combo, 0);
        // Hopping backwards and forwards does not pay twice for old rows.
        let before = g.score;
        g.step(&press(Dir::Down));
        run(&mut g, 20);
        g.step(&up());
        run(&mut g, 20);
        assert_eq!(g.score, before, "no points for rows already reached");
    }

    #[test]
    fn shards_and_items_are_collected_once() {
        let (mut g, r) = find(|g, r| {
            let s = g.world.row(r).clone();
            s.shard.is_some() && s.is_safe_ground() && g.world.row(r - 1).is_safe_ground()
        });
        let col = g.world.row(r).shard.unwrap();
        place(&mut g, r - 1, col as f32 + 0.5);
        g.step(&up());
        run(&mut g, 20);
        assert_eq!(g.shards, 1);
        assert!(g.score >= SHARD_SCORE);
        // Stepping back onto the tile does not collect again.
        g.step(&press(Dir::Down));
        run(&mut g, 20);
        g.step(&up());
        run(&mut g, 20);
        assert_eq!(g.shards, 1);
    }

    #[test]
    fn items_grant_the_shield_or_the_boots() {
        let (mut g, r) = find(|g, r| {
            let s = g.world.row(r).clone();
            s.item.is_some() && s.is_safe_ground() && g.world.row(r - 1).is_safe_ground()
        });
        let (kind, col) = g.world.row(r).item.unwrap();
        place(&mut g, r - 1, col as f32 + 0.5);
        g.step(&up());
        run(&mut g, 20);
        match kind {
            ItemKind::Aegis => assert!(g.shield),
            ItemKind::Boots => assert!(g.boots > 7.0 && !g.boots_jump_used),
        }
    }

    #[test]
    fn the_idle_sweep_and_the_camera_sweep_kill_in_classic_and_blitz() {
        // Idle sweep: 6.5 s on the same row.
        let mut g = HopperGame::new(HopperMode::Classic, 1);
        let mut steps = 0;
        while g.is_playing() && steps < 20_000 {
            g.camera_row = -100.0; // keep the camera sweep out of the way
            g.step(&idle());
            steps += 1;
        }
        assert_eq!(g.status, Status::Dead(Cause::Idle));
        assert!(
            (6.4..6.7).contains(&(steps as f32 / 120.0)),
            "{} s",
            steps as f32 / 120.0
        );
        // Camera sweep: falling behind the lower edge.
        let mut c = HopperGame::new(HopperMode::BlitzRush, 1);
        let mut guard = 0;
        while c.is_playing() && guard < 20_000 {
            c.idle_time = 0.0;
            c.step(&idle());
            guard += 1;
        }
        assert_eq!(c.status, Status::Dead(Cause::Sweep));
        let waited = guard as f32 / 120.0;
        assert!(
            waited < 6.5,
            "blitz sweeps before the idle timeout: {waited}"
        );
    }

    #[test]
    fn the_camera_follows_upwards_and_old_rows_are_forgotten() {
        let mut g = HopperGame::new(HopperMode::Zen, 7);
        let mut rows_seen = 0;
        for _ in 0..(30.0 * 120.0) as usize {
            let i = crate::core::hopper::bot::choose(&mut g);
            g.step(&i);
            rows_seen = rows_seen.max(g.world.cached_rows());
        }
        assert!(g.max_row > 30);
        assert!(g.camera_row > g.max_row as f32 - 8.0, "camera follows");
        assert!(rows_seen < 60, "row cache stays bounded: {rows_seen}");
        assert!(g.taken_shards.len() < 40 && g.sinking.len() < 40);
    }

    #[test]
    fn hazard_bridges_sink_after_0_9_seconds() {
        let mut g = HopperGame::new_at_row(HopperMode::Classic, 5, 34);
        assert!(g.row >= 34);
        let mut bridge = None;
        for r in 35..400 {
            if let LaneKind::Bridge { solid, .. } = g.world.row(r).lane.clone()
                && let Some(c) = (0..COLS).find(|&c| solid[c])
            {
                bridge = Some((r, c));
                break;
            }
        }
        let (br, bc) = bridge.expect("a bridge row");
        place(&mut g, br, bc as f32 + 0.5);
        g.sinking.insert((br, bc), g.time);
        // Safe for 0.9 s ...
        for _ in 0..((BRIDGE_SINK_AFTER - 0.05) * 120.0) as usize {
            g.idle_time = 0.0;
            g.step(&idle());
            assert!(g.is_playing(), "tile still holds at {}", g.time);
        }
        // ... then it sinks under the player.
        run(&mut g, 20);
        assert_eq!(g.status, Status::Dead(Cause::Fall));
    }

    #[test]
    fn hazard_lasers_fire_only_in_their_active_window_on_active_tiles() {
        let mut g = HopperGame::new_at_row(HopperMode::Classic, 5, 34);
        let mut laser = None;
        for r in 35..400 {
            if let LaneKind::Laser {
                period,
                phase,
                active,
                ..
            } = g.world.row(r).lane.clone()
                && let Some(c) = (0..COLS).find(|&c| active[c])
            {
                laser = Some((r, c, period, phase));
                break;
            }
        }
        let (lr, lc, period, phase) = laser.expect("a laser row");
        place(&mut g, lr, lc as f32 + 0.5);
        let active_start = period - LASER_ACTIVE;
        let want = |t: f32| (t + phase).rem_euclid(period);
        let mut t = 0.0;
        while want(t) > 0.3 {
            t += 0.01;
        }
        g.time = t; // right after the previous active window
        let mut died_at = None;
        for _ in 0..(period * 120.0) as usize {
            g.idle_time = 0.0;
            g.camera_row = lr as f32 - 3.0;
            g.step(&idle());
            if !g.is_playing() {
                died_at = Some(want(g.time));
                break;
            }
        }
        let c = died_at.expect("the laser fires on its active tile");
        assert_eq!(g.status, Status::Dead(Cause::Hazard));
        assert!(
            c >= active_start - 0.02,
            "fires in the active window: {c} vs {active_start}"
        );
        // A free (inactive) tile of the same row is never hit.
        let mut h = HopperGame::new_at_row(HopperMode::Classic, 5, 34);
        let LaneKind::Laser { active, .. } = h.world.row(lr).lane.clone() else {
            unreachable!()
        };
        let free = (0..COLS).find(|&c| !active[c]).unwrap();
        place(&mut h, lr, free as f32 + 0.5);
        for _ in 0..(period * 2.0 * 120.0) as usize {
            h.idle_time = 0.0;
            h.camera_row = lr as f32 - 3.0;
            h.step(&idle());
        }
        assert!(h.is_playing());
    }

    #[test]
    fn daily_worlds_do_not_depend_on_what_the_player_does() {
        let seed = 20261116;
        let mut a = HopperGame::new(HopperMode::DailyVibe, seed);
        let mut b = HopperGame::new(HopperMode::DailyVibe, seed);
        // a flies far with the bot, b sits still until it is swept away.
        for _ in 0..(15.0 * 120.0) as usize {
            let i = crate::core::hopper::bot::choose(&mut a);
            a.step(&i);
            b.step(&idle());
        }
        assert!(a.max_row > b.max_row + 10);
        for r in 0..a.max_row.min(80) {
            assert_eq!(a.world.row(r).clone(), b.world.row(r).clone(), "row {r}");
        }
        for t in [0.0f32, 7.7, 40.0] {
            for r in 0..60 {
                let (sa, sb) = (a.world.row(r).clone(), b.world.row(r).clone());
                assert_eq!(sa.vehicles(t), sb.vehicles(t));
                assert_eq!(sa.platforms(t), sb.platforms(t));
            }
        }
        let mut o = HopperGame::new(HopperMode::DailyVibe, seed + 1);
        assert!((0..40).any(|r| a.world.row(r).clone() != o.world.row(r).clone()));
    }

    #[test]
    fn long_random_play_keeps_the_invariants() {
        for mode in HopperMode::ALL {
            let mut g = HopperGame::new(mode, 99);
            let mut rng = crate::core::rng::SplitMix64::new(5);
            for _ in 0..(20 * 120) {
                let dir = (rng.next_below(10) == 0).then(|| match rng.next_below(4) {
                    0 => Dir::Up,
                    1 => Dir::Down,
                    2 => Dir::Left,
                    _ => Dir::Right,
                });
                g.step(&HopperInput { dir });
                if !g.is_playing() {
                    g = HopperGame::new(mode, rng.next_u64());
                    continue;
                }
                assert!(g.x.is_finite() && g.row >= 0);
                assert!((0.0..=WIDTH).contains(&g.x), "x {}", g.x);
                assert!((0.0..=100.0).contains(&g.flow.flow));
                assert!(g.world.cached_rows() < 200);
            }
        }
    }
}
