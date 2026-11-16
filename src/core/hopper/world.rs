//! The endless world: biomes, lanes and every moving thing on them.
//!
//! **Everything here is a pure function of `(seed, row, world time)`.**
//! Rows are generated lazily as the camera advances, so *when* a row is
//! created depends on the player; vehicles, logs, trains and hazard cycles
//! are therefore never simulated forward from their spawn time but computed
//! from a phase: `pos(t) = (phase + dir * speed * t + i * spacing) mod ring`.
//! A fast and a slow player see exactly the same traffic on a Daily seed.

use std::collections::HashMap;

use crate::core::rng::SplitMix64;

use super::geom::Aabb;

/// Playable columns: tile centres `0.5 ..= 14.5` (x runs over `0.0 .. 15.0`).
pub const COLS: usize = 15;
pub const WIDTH: f32 = COLS as f32;
/// Player hit box (forgiving).
pub const HITBOX: f32 = 0.62;
pub const HOP_TIME: f32 = 0.11;
/// Rows from this index on may contain hazard lanes.
pub const HAZARD_FROM: i64 = 35;
pub const START_ROWS: i64 = 4;
pub const MIN_FREE_COLS: usize = 4;

pub const TRAIN_WARNING: f32 = 1.1;
pub const TRAIN_PASS: f32 = 0.35;
pub const TRAIN_LEN: f32 = 10.0;
pub const VEHICLE_H: f32 = 0.7;

pub const LASER_CHARGE: f32 = 1.0;
pub const LASER_ACTIVE: f32 = 0.7;
pub const BRIDGE_SINK_AFTER: f32 = 0.9;
pub const BRIDGE_RESTORE_AFTER: f32 = 2.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Meadow,
    Highway,
    Railway,
    River,
    Hazard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKind {
    /// Nimble hover racer.
    Racer,
    /// Mid-sized cyber truck.
    Truck,
    /// Slow cargo convoy.
    Convoy,
}

impl VehicleKind {
    pub fn len(self) -> f32 {
        match self {
            VehicleKind::Racer => 1.0,
            VehicleKind::Truck => 1.8,
            VehicleKind::Convoy => 3.4,
        }
    }

    pub fn base_speed(self) -> f32 {
        match self {
            VehicleKind::Racer => 5.0,
            VehicleKind::Truck => 3.2,
            VehicleKind::Convoy => 1.9,
        }
    }

    pub fn is_empty(self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Aegis,
    Boots,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaneKind {
    Grass {
        trees: [bool; COLS],
        corridor: usize,
    },
    Road {
        dir: f32,
        kind: VehicleKind,
        speed: f32,
        spacing: f32,
        phase: f32,
        ring: f32,
    },
    Rail {
        dir: f32,
        period: f32,
        phase: f32,
    },
    River {
        dir: f32,
        speed: f32,
        width: f32,
        spacing: f32,
        phase: f32,
        ring: f32,
    },
    Laser {
        period: f32,
        phase: f32,
        active: [bool; COLS],
        corridor: usize,
    },
    Bridge {
        solid: [bool; COLS],
        corridor: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowSpec {
    pub row: i64,
    pub biome: Biome,
    pub lane: LaneKind,
    /// Index of the lane inside its segment (0 = first).
    pub lane_index: usize,
    pub segment: usize,
    /// Vibe shard on this tile column, if any.
    pub shard: Option<usize>,
    /// Power-up item on this tile column, if any.
    pub item: Option<(ItemKind, usize)>,
}

impl RowSpec {
    /// Grass, start rows and anything the player can stand on safely.
    pub fn is_safe_ground(&self) -> bool {
        matches!(self.lane, LaneKind::Grass { .. })
    }

    pub fn is_tree(&self, col: usize) -> bool {
        matches!(&self.lane, LaneKind::Grass { trees, .. } if trees[col])
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub start: i64,
    pub len: i64,
    pub biome: Biome,
    pub dir0: f32,
    pub corridor: usize,
}

/// Per-mode spawn rates of collectibles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub shard: f32,
    pub item: f32,
}

#[derive(Clone, Debug)]
pub struct World {
    seed: u64,
    rates: Rates,
    seg_rng: SplitMix64,
    pub segments: Vec<Segment>,
    rows: HashMap<i64, RowSpec>,
}

const SEG_SALT: u64 = 0x40_9AE5_0001_5EED;
const ROW_SALT: u64 = 0x40_9AE5_0002_5EED;

fn row_rng(seed: u64, row: i64) -> SplitMix64 {
    let mut r = SplitMix64::new(seed ^ ROW_SALT ^ (row as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    r.next_u64(); // decorrelate neighbouring rows
    r
}

/// Positions of the vehicles / logs of a ring lane at world time `t`.
pub fn ring_positions(
    phase: f32,
    dir: f32,
    speed: f32,
    spacing: f32,
    ring: f32,
    margin: f32,
    t: f32,
) -> Vec<f32> {
    let n = (ring / spacing).round() as usize;
    (0..n)
        .map(|i| (phase + dir * speed * t + i as f32 * spacing).rem_euclid(ring) - margin)
        .collect()
}

/// Where a train is at time `t`: `Idle`, `Warning { left }` or `Pass`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrainPhase {
    Idle,
    Warning { left: f32 },
    Pass { progress: f32 },
}

pub fn train_phase(period: f32, phase: f32, t: f32) -> TrainPhase {
    let c = (t + phase).rem_euclid(period);
    let pass_start = period - TRAIN_PASS;
    let warn_start = pass_start - TRAIN_WARNING;
    if c >= pass_start {
        TrainPhase::Pass {
            progress: (c - pass_start) / TRAIN_PASS,
        }
    } else if c >= warn_start {
        TrainPhase::Warning {
            left: pass_start - c,
        }
    } else {
        TrainPhase::Idle
    }
}

/// `(left, right)` x-extent of the train while it passes.
pub fn train_span(dir: f32, progress: f32) -> (f32, f32) {
    let travel = WIDTH + 2.0 * TRAIN_LEN;
    let front = if dir > 0.0 {
        -TRAIN_LEN + travel * progress
    } else {
        WIDTH + TRAIN_LEN - travel * progress
    };
    if dir > 0.0 {
        (front - TRAIN_LEN, front)
    } else {
        (front, front + TRAIN_LEN)
    }
}

/// Laser cycle: `Off`, `Charging` (visible warning) or `Active`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaserPhase {
    Off,
    Charging,
    Active,
}

/// Seconds until the laser fires, while it is charging.
pub fn laser_charge_left(period: f32, phase: f32, t: f32) -> Option<f32> {
    let c = (t + phase).rem_euclid(period);
    let active_start = period - LASER_ACTIVE;
    (c >= active_start - LASER_CHARGE && c < active_start).then_some(active_start - c)
}

pub fn laser_phase(period: f32, phase: f32, t: f32) -> LaserPhase {
    let c = (t + phase).rem_euclid(period);
    let active_start = period - LASER_ACTIVE;
    if c >= active_start {
        LaserPhase::Active
    } else if c >= active_start - LASER_CHARGE {
        LaserPhase::Charging
    } else {
        LaserPhase::Off
    }
}

impl RowSpec {
    /// Centres of the vehicles of a road lane.
    pub fn vehicles(&self, t: f32) -> Vec<f32> {
        match &self.lane {
            LaneKind::Road {
                dir,
                kind,
                speed,
                spacing,
                phase,
                ring,
            } => ring_positions(*phase, *dir, *speed, *spacing, *ring, kind.len() + 2.0, t),
            _ => Vec::new(),
        }
    }

    /// Centres of the platforms of a river lane.
    pub fn platforms(&self, t: f32) -> Vec<f32> {
        match &self.lane {
            LaneKind::River {
                dir,
                speed,
                width,
                spacing,
                phase,
                ring,
            } => ring_positions(*phase, *dir, *speed, *spacing, *ring, width + 2.0, t),
            _ => Vec::new(),
        }
    }

    /// Drift velocity (tiles/s) of the lane's logs.
    pub fn log_velocity(&self) -> f32 {
        match &self.lane {
            LaneKind::River { dir, speed, .. } => dir * speed,
            _ => 0.0,
        }
    }

    /// Does a vehicle or train overlap a player box centred at `x` on this
    /// row at time `t`? `y` is the player's centre (row units).
    pub fn lethal_at(&self, x: f32, y: f32, t: f32) -> bool {
        let player = Aabb::from_center(x, y, HITBOX, HITBOX);
        let r = self.row as f32;
        match &self.lane {
            LaneKind::Road { kind, .. } => self
                .vehicles(t)
                .iter()
                .any(|&c| Aabb::from_center(c, r + 0.5, kind.len(), VEHICLE_H).overlaps(&player)),
            LaneKind::Rail { dir, period, phase } => match train_phase(*period, *phase, t) {
                TrainPhase::Pass { progress } => {
                    let (l, rr) = train_span(*dir, progress);
                    Aabb {
                        x: l,
                        y: r + 0.1,
                        w: rr - l,
                        h: 0.8,
                    }
                    .overlaps(&player)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// The log under `x` at time `t` (with the +-`tolerance` leniency of the
    /// platform edges), returning its drift velocity.
    pub fn platform_under(&self, x: f32, t: f32, tolerance: f32) -> Option<f32> {
        let LaneKind::River { width, .. } = &self.lane else {
            return None;
        };
        self.platforms(t)
            .iter()
            .any(|&c| x >= c - width * 0.5 - tolerance && x <= c + width * 0.5 + tolerance)
            .then(|| self.log_velocity())
    }

    /// Is the laser field over tile `col` deadly at `t`?
    pub fn laser_active(&self, col: usize, t: f32) -> bool {
        match &self.lane {
            LaneKind::Laser {
                period,
                phase,
                active,
                ..
            } => active[col] && laser_phase(*period, *phase, t) == LaserPhase::Active,
            _ => false,
        }
    }
}

impl World {
    pub fn new(seed: u64, rates: Rates) -> Self {
        let mut w = World {
            seed,
            rates,
            seg_rng: SplitMix64::new(seed ^ SEG_SALT),
            segments: Vec::new(),
            rows: HashMap::new(),
        };
        // Four safe grass rows without obstacles.
        w.segments.push(Segment {
            start: 0,
            len: START_ROWS,
            biome: Biome::Meadow,
            dir0: 1.0,
            corridor: COLS / 2,
        });
        w
    }

    pub fn cached_rows(&self) -> usize {
        self.rows.len()
    }

    fn next_segment(&mut self) {
        let last = self.segments.last().expect("start segment");
        let start = last.start + last.len;
        let prev = last.biome;
        let rng = &mut self.seg_rng;
        let roll = rng.next_below(100);
        let hazard_ok = start >= HAZARD_FROM;
        let biome = match prev {
            // After a river always comes solid ground or a road.
            Biome::River => {
                if roll < 55 {
                    Biome::Meadow
                } else {
                    Biome::Highway
                }
            }
            Biome::Meadow => match roll {
                0..=29 => Biome::Highway,
                30..=44 => Biome::Railway,
                45..=74 => Biome::River,
                75..=89 if hazard_ok => Biome::Hazard,
                _ => Biome::Highway,
            },
            _ => match roll {
                0..=44 => Biome::Meadow,
                45..=64 => Biome::River,
                65..=79 if hazard_ok => Biome::Hazard,
                65..=79 => Biome::Meadow,
                80..=89 => Biome::Highway,
                _ => Biome::Meadow,
            },
        };
        let len = match biome {
            Biome::Meadow => 1 + rng.next_below(3) as i64,
            Biome::Highway => 1 + rng.next_below(4) as i64,
            Biome::Railway => 1 + rng.next_below(2) as i64,
            Biome::River => 1 + rng.next_below(3) as i64,
            Biome::Hazard => 2 + rng.next_below(3) as i64,
        };
        let dir0 = if rng.next_below(2) == 0 { 1.0 } else { -1.0 };
        let corridor = rng.next_below(COLS as u64) as usize;
        self.segments.push(Segment {
            start,
            len,
            biome,
            dir0,
            corridor,
        });
    }

    fn segment_of(&mut self, row: i64) -> usize {
        loop {
            let last = self.segments.last().expect("start segment");
            if row < last.start + last.len {
                break;
            }
            self.next_segment();
        }
        self.segments
            .iter()
            .position(|s| row >= s.start && row < s.start + s.len)
            .expect("row inside a segment")
    }

    /// The spec of `row` (generated on demand and cached).
    pub fn row(&mut self, row: i64) -> &RowSpec {
        if !self.rows.contains_key(&row) {
            let spec = self.build_row(row.max(0));
            self.rows.insert(row, spec);
        }
        &self.rows[&row]
    }

    /// A row that is already generated (read-only, for renderers).
    pub fn peek(&self, row: i64) -> Option<&RowSpec> {
        self.rows.get(&row)
    }

    /// Drops cached rows below `min_row`.
    pub fn forget_below(&mut self, min_row: i64) {
        self.rows.retain(|r, _| *r >= min_row);
    }

    fn build_row(&mut self, row: i64) -> RowSpec {
        let si = self.segment_of(row);
        let seg = self.segments[si].clone();
        let lane_index = (row - seg.start) as usize;
        let mut rng = row_rng(self.seed, row);
        let difficulty = 1.0 + (row as f32 / 150.0).min(0.7);
        let lane = match seg.biome {
            Biome::Meadow => {
                let mut trees = [false; COLS];
                // Always draw, so the stream stays aligned.
                let rolls: Vec<f32> = (0..COLS).map(|_| rng.next_f32()).collect();
                if row >= START_ROWS {
                    for c in 0..COLS {
                        trees[c] = rolls[c] < 0.22 && c != seg.corridor;
                    }
                    // At least MIN_FREE_COLS free columns.
                    let mut c = 0;
                    while trees.iter().filter(|t| !**t).count() < MIN_FREE_COLS {
                        trees[c] = false;
                        c += 1;
                    }
                }
                LaneKind::Grass {
                    trees,
                    corridor: seg.corridor,
                }
            }
            Biome::Highway => {
                let dir = if lane_index.is_multiple_of(2) {
                    seg.dir0
                } else {
                    -seg.dir0
                };
                let roll = rng.next_f32();
                let kind = if roll < 0.5 {
                    VehicleKind::Racer
                } else if roll < 0.85 {
                    VehicleKind::Truck
                } else {
                    VehicleKind::Convoy
                };
                let speed = kind.base_speed() * difficulty;
                // Gap between vehicles: enough to hop across (hop time at
                // lane speed plus the hit box plus a margin).
                let gap = (speed * 0.45 + HITBOX + 0.9).max(2.2) + 1.5 * rng.next_f32();
                let spacing = kind.len() + gap;
                let margin = kind.len() + 2.0;
                let ring = spacing * ((WIDTH + 2.0 * margin) / spacing).ceil();
                let phase = rng.next_f32() * ring;
                LaneKind::Road {
                    dir,
                    kind,
                    speed,
                    spacing,
                    phase,
                    ring,
                }
            }
            Biome::Railway => {
                let dir = if rng.next_below(2) == 0 { 1.0 } else { -1.0 };
                let period = 6.0 + 4.0 * rng.next_f32();
                let phase = rng.next_f32() * period;
                LaneKind::Rail { dir, period, phase }
            }
            Biome::River => {
                // Neighbouring lanes flow in opposite directions.
                let dir = if lane_index.is_multiple_of(2) {
                    seg.dir0
                } else {
                    -seg.dir0
                };
                let speed =
                    ((0.9 + 1.0 * rng.next_f32()) * (1.0 + (difficulty - 1.0) * 0.5)).min(2.4);
                let width = 2.5 + 1.5 * rng.next_f32();
                let gap = 1.2 + 1.2 * rng.next_f32();
                let spacing = width + gap;
                let margin = width + 2.0;
                let ring = spacing * ((WIDTH + 2.0 * margin) / spacing).ceil();
                let phase = rng.next_f32() * ring;
                LaneKind::River {
                    dir,
                    speed,
                    width,
                    spacing,
                    phase,
                    ring,
                }
            }
            Biome::Hazard => {
                if lane_index.is_multiple_of(2) {
                    let period = 4.0 + rng.next_f32();
                    let phase = rng.next_f32() * period;
                    let mut active = [false; COLS];
                    for (c, a) in active.iter_mut().enumerate() {
                        *a = rng.next_f32() < 0.55 && c != seg.corridor;
                    }
                    let mut c = 0;
                    while active.iter().filter(|a| !**a).count() < MIN_FREE_COLS {
                        active[c] = false;
                        c += 1;
                    }
                    LaneKind::Laser {
                        period,
                        phase,
                        active,
                        corridor: seg.corridor,
                    }
                } else {
                    let mut solid = [true; COLS];
                    for (c, s) in solid.iter_mut().enumerate() {
                        *s = rng.next_f32() > 0.2 || c == seg.corridor;
                    }
                    LaneKind::Bridge {
                        solid,
                        corridor: seg.corridor,
                    }
                }
            }
        };
        // Collectibles (rolls are always drawn, so rows stay independent).
        let shard_roll = rng.next_f32();
        let shard_col = rng.next_below(COLS as u64) as usize;
        let item_roll = rng.next_f32();
        let item_col = rng.next_below(COLS as u64) as usize;
        let item_kind = if rng.next_below(2) == 0 {
            ItemKind::Aegis
        } else {
            ItemKind::Boots
        };
        let standable = |c: usize| match &lane {
            LaneKind::Grass { trees, .. } => !trees[c] && row >= START_ROWS,
            LaneKind::Road { .. } | LaneKind::Rail { .. } => true,
            LaneKind::Bridge { solid, .. } => solid[c],
            _ => false,
        };
        let shard = (shard_roll < self.rates.shard && standable(shard_col)).then_some(shard_col);
        let item = (item_roll < self.rates.item && standable(item_col) && Some(item_col) != shard)
            .then_some((item_kind, item_col));
        RowSpec {
            row,
            biome: seg.biome,
            lane,
            lane_index,
            segment: si,
            shard,
            item,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: Rates = Rates {
        shard: 0.16,
        item: 0.05,
    };

    fn world(seed: u64) -> World {
        World::new(seed, RATES)
    }

    #[test]
    fn the_first_four_rows_are_empty_safe_grass() {
        let mut w = world(1);
        for r in 0..START_ROWS {
            let s = w.row(r).clone();
            assert!(s.is_safe_ground() && s.biome == Biome::Meadow);
            assert!(!(0..COLS).any(|c| s.is_tree(c)), "no trees on start rows");
            assert!(s.shard.is_none() && s.item.is_none());
        }
    }

    #[test]
    fn every_grass_row_has_at_least_four_free_columns_and_a_free_corridor() {
        for seed in 0..40u64 {
            let mut w = world(seed);
            for r in 0..400 {
                let s = w.row(r).clone();
                if let LaneKind::Grass { trees, corridor } = s.lane {
                    let free = trees.iter().filter(|t| !**t).count();
                    assert!(free >= MIN_FREE_COLS, "seed {seed} row {r}: {free} free");
                    assert!(!trees[corridor], "corridor is free");
                }
            }
        }
    }

    #[test]
    fn a_free_path_runs_through_every_grass_segment() {
        // BFS over free tiles from the corridor of the first row to the last
        // row of every grass segment: transitions are never fully blocked.
        for seed in 0..30u64 {
            let mut w = world(seed);
            for r in 0..300 {
                w.row(r);
            }
            for si in 0..w.segments.len() {
                let seg = w.segments[si].clone();
                if seg.biome != Biome::Meadow || seg.start >= 300 {
                    continue;
                }
                let mut free = Vec::new();
                for r in seg.start..seg.start + seg.len {
                    let s = w.row(r).clone();
                    let mut row = [true; COLS];
                    for (c, f) in row.iter_mut().enumerate() {
                        *f = !s.is_tree(c);
                    }
                    free.push(row);
                }
                let mut seen = vec![[false; COLS]; free.len()];
                let mut stack = vec![(0usize, seg.corridor)];
                seen[0][seg.corridor] = true;
                while let Some((r, c)) = stack.pop() {
                    let mut push = |nr: usize, nc: usize| {
                        if free[nr][nc] && !seen[nr][nc] {
                            seen[nr][nc] = true;
                            stack.push((nr, nc));
                        }
                    };
                    if r + 1 < free.len() {
                        push(r + 1, c);
                    }
                    if r > 0 {
                        push(r - 1, c);
                    }
                    if c > 0 {
                        push(r, c - 1);
                    }
                    if c + 1 < COLS {
                        push(r, c + 1);
                    }
                }
                assert!(
                    seen[free.len() - 1].iter().any(|s| *s),
                    "seed {seed}: segment at {} is blocked",
                    seg.start
                );
            }
        }
    }

    #[test]
    fn rivers_obey_the_fairness_rules() {
        for seed in 0..60u64 {
            let mut w = world(seed);
            for r in 0..600 {
                w.row(r);
            }
            let mut run = 0;
            let mut last_dir: Option<f32> = None;
            for r in 0..600 {
                let s = w.row(r).clone();
                if let LaneKind::River {
                    dir, width, speed, ..
                } = s.lane
                {
                    run += 1;
                    assert!(
                        run <= 3,
                        "seed {seed}: more than 3 river lanes in a row at {r}"
                    );
                    assert!((2.5..=4.0).contains(&width), "platform width {width}");
                    assert!(speed > 0.5 && speed <= 2.4);
                    if let Some(d) = last_dir {
                        assert_eq!(d, -dir, "adjacent lanes alternate direction (row {r})");
                    }
                    last_dir = Some(dir);
                } else {
                    run = 0;
                    last_dir = None;
                    // A river lane is always followed by grass or road.
                    if r > 0 && matches!(w.row(r - 1).lane, LaneKind::River { .. }) {
                        assert!(
                            matches!(s.lane, LaneKind::Grass { .. } | LaneKind::Road { .. }),
                            "seed {seed}: row {r} after a river must be solid"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn vehicle_gaps_leave_time_to_cross() {
        for seed in 0..40u64 {
            let mut w = world(seed);
            for r in 0..500 {
                let s = w.row(r).clone();
                if let LaneKind::Road {
                    kind,
                    speed,
                    spacing,
                    ring,
                    ..
                } = s.lane
                {
                    let gap = spacing - kind.len();
                    assert!(
                        gap >= speed * HOP_TIME + HITBOX + 0.5,
                        "seed {seed} row {r}: gap {gap} too small for speed {speed}"
                    );
                    assert!(ring >= WIDTH + 2.0 * (kind.len() + 2.0) - 1e-3);
                    // The actual positions keep at least that spacing.
                    for t in [0.0f32, 1.3, 7.7] {
                        let mut v = s.vehicles(t);
                        v.sort_by(|a, b| a.total_cmp(b));
                        for p in v.windows(2) {
                            assert!(p[1] - p[0] - kind.len() >= gap - 1e-3);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hazards_only_appear_from_row_35() {
        let mut seen = false;
        for seed in 0..40u64 {
            let mut w = world(seed);
            for r in 0..400 {
                let s = w.row(r).clone();
                if s.biome == Biome::Hazard {
                    seen = true;
                    assert!(r >= HAZARD_FROM, "hazard at row {r}");
                }
            }
        }
        assert!(seen, "hazard lanes do exist");
    }

    #[test]
    fn all_five_biomes_occur() {
        let mut w = world(3);
        let mut biomes = std::collections::HashSet::new();
        for r in 0..600 {
            biomes.insert(w.row(r).biome);
        }
        assert_eq!(biomes.len(), 5);
    }

    #[test]
    fn rows_do_not_depend_on_the_order_they_are_generated_in() {
        let mut a = world(20261116);
        let mut b = world(20261116);
        for r in 0..120 {
            a.row(r);
        }
        for r in (0..120).rev() {
            b.row(r); // far rows first
        }
        for r in 0..120 {
            assert_eq!(a.row(r), b.row(r), "row {r}");
        }
        let mut c = world(20261117);
        assert!((0..60).any(|r| a.row(r) != c.row(r)));
    }

    #[test]
    fn traffic_is_a_pure_function_of_time() {
        // Whoever generated the row first, vehicles and logs are at the very
        // same place at the same world time.
        let mut a = world(77);
        let mut b = world(77);
        for r in 0..150 {
            a.row(r);
        }
        for t in [0.0f32, 3.3, 12.5, 100.0] {
            for r in (0..150).step_by(7) {
                let (sa, sb) = (a.row(r).clone(), b.row(r).clone());
                assert_eq!(sa.vehicles(t), sb.vehicles(t));
                assert_eq!(sa.platforms(t), sb.platforms(t));
            }
        }
    }

    #[test]
    fn train_cycle_warns_for_1_1_seconds_then_passes_in_0_35() {
        let (period, phase) = (8.0, 0.0);
        assert_eq!(train_phase(period, phase, 1.0), TrainPhase::Idle);
        // Warning starts 1.45 s before the cycle end.
        let warn_start = period - TRAIN_PASS - TRAIN_WARNING;
        assert_eq!(
            train_phase(period, phase, warn_start - 0.01),
            TrainPhase::Idle
        );
        match train_phase(period, phase, warn_start + 0.01) {
            TrainPhase::Warning { left } => assert!((left - TRAIN_WARNING).abs() < 0.02),
            p => panic!("{p:?}"),
        }
        assert!(matches!(
            train_phase(period, phase, period - TRAIN_PASS + 0.01),
            TrainPhase::Pass { .. }
        ));
        // The span crosses the whole row.
        let (l0, r0) = train_span(1.0, 0.0);
        let (l1, r1) = train_span(1.0, 1.0);
        assert!(
            r0 <= 0.0 + 1e-4 && l1 >= WIDTH - 1e-4,
            "{r0} {l1} {l0} {r1}"
        );
        let (l, r) = train_span(1.0, 0.5);
        assert!(
            ((r - l) - TRAIN_LEN).abs() < 1e-4,
            "the train is 10 tiles long"
        );
        assert!(l < WIDTH * 0.5 && r > 0.0, "mid-pass it is on screen");
    }

    #[test]
    fn laser_cycle_charges_for_one_second_before_firing() {
        let (period, phase) = (4.5, 0.0);
        let active_start = period - LASER_ACTIVE;
        assert_eq!(laser_phase(period, phase, 0.1), LaserPhase::Off);
        assert_eq!(
            laser_phase(period, phase, active_start - 0.5),
            LaserPhase::Charging
        );
        assert_eq!(
            laser_phase(period, phase, active_start + 0.1),
            LaserPhase::Active
        );
        assert_eq!(
            laser_phase(period, phase, active_start - LASER_CHARGE - 0.05),
            LaserPhase::Off
        );
    }

    #[test]
    fn vehicle_collision_uses_the_forgiving_hit_box() {
        let mut w = world(5);
        // Find a road lane and a time where a vehicle is at a known place.
        let mut spec = None;
        for r in 0..300 {
            let s = w.row(r).clone();
            if matches!(s.lane, LaneKind::Road { .. }) {
                spec = Some(s);
                break;
            }
        }
        let s = spec.expect("a road");
        let LaneKind::Road { kind, .. } = s.lane else {
            unreachable!()
        };
        let c = s.vehicles(0.0)[0].clamp(2.0, 12.0);
        let t = 0.0;
        let v = s.vehicles(t);
        let c = *v.iter().find(|&&x| (1.0..14.0).contains(&x)).unwrap_or(&c);
        let y = s.row as f32 + 0.5;
        assert!(s.lethal_at(c, y, t), "standing in the vehicle");
        let edge = c + kind.len() * 0.5 + HITBOX * 0.5;
        assert!(!s.lethal_at(edge + 0.02, y, t), "just clear of the vehicle");
        assert!(s.lethal_at(edge - 0.05, y, t), "just touching deeper");
        // A different row's lane never hurts.
        assert!(!s.lethal_at(c, y + 1.0, t));
    }

    #[test]
    fn platform_tolerance_is_0_42_tiles_beyond_the_edges() {
        let mut w = world(9);
        let mut spec = None;
        for r in 0..400 {
            let s = w.row(r).clone();
            if matches!(s.lane, LaneKind::River { .. }) {
                spec = Some(s);
                break;
            }
        }
        let s = spec.expect("a river");
        let LaneKind::River { width, .. } = s.lane else {
            unreachable!()
        };
        let t = 2.0;
        let c = *s
            .platforms(t)
            .iter()
            .find(|&&x| (3.0..12.0).contains(&x))
            .expect("log");
        let left = c - width * 0.5;
        assert!(
            s.platform_under(left - 0.41, t, 0.42).is_some(),
            "just inside the tolerance"
        );
        // Nothing else within reach on that side (gaps are >= 1.2).
        assert!(
            s.platform_under(left - 0.43, t, 0.42).is_none(),
            "just outside"
        );
        assert!(s.platform_under(c + width * 0.5 + 0.41, t, 0.42).is_some());
        assert!(s.platform_under(c, t, 0.42).is_some());
    }

    #[test]
    fn old_rows_can_be_dropped_and_come_back_identical() {
        let mut w = world(4);
        let before = w.row(10).clone();
        for r in 0..200 {
            w.row(r);
        }
        w.forget_below(150);
        assert!(w.cached_rows() <= 50);
        assert_eq!(*w.row(10), before, "regenerated rows are identical");
    }

    #[test]
    fn collectible_rates_follow_the_mode() {
        let count = |rates: Rates| {
            let mut w = World::new(11, rates);
            (0..1000)
                .map(|r| {
                    let s = w.row(r);
                    s.shard.is_some() as usize + s.item.is_some() as usize
                })
                .sum::<usize>()
        };
        let calm = count(RATES);
        let rush = count(Rates {
            shard: 0.30,
            item: 0.12,
        });
        assert!(rush > calm * 3 / 2, "blitz spawns more: {rush} vs {calm}");
    }
}
