//! `ArmadaGame`: the complete, deterministic rules of VibeCoded Armada.
//!
//! The simulation runs in fixed 1/120 s steps (a 760 u/s bolt moves under
//! 6.4 units per step, less than one 8-unit bunker cell, so nothing tunnels).
//! Frame time only feeds an accumulator; fire and dash presses are buffered
//! so short taps are never lost. No rendering, no `macroquad`.

use crate::core::flow::{FlowEngine, FlowTier};
use crate::core::rng::SplitMix64;

use super::bunker::{self, Bunker};
use super::geom::Aabb;
use super::waves::{
    COLS, Enemy, EnemyKind, PowerKind, SLOT_X, SLOT_Y, UfoFlight, UfoPlan, WaveSpec, is_boss_wave,
    wave_spec,
};

pub const ARENA_W: f32 = 900.0;
pub const ARENA_H: f32 = 640.0;
pub const DEFENSE_Y: f32 = 560.0;
pub const SHIP_Y: f32 = 595.0;
pub const BUNKER_Y: f32 = 505.0;
pub const UFO_Y: f32 = 36.0;
pub const STEP: f32 = 1.0 / 120.0;
pub const MAX_FRAME_DT: f32 = 0.05;

pub const SHIP_W: f32 = 30.0;
pub const SHIP_H: f32 = 18.0;
pub const SHIP_SPEED: f32 = 420.0;
pub const SHIP_MIN_X: f32 = 30.0;
pub const SHIP_MAX_X: f32 = 870.0;
pub const FIRE_COOLDOWN: f32 = 0.22;
pub const MAX_STD_BOLTS: usize = 3;
pub const BOLT_SPEED: f32 = 720.0;
pub const DASH_DURATION: f32 = 0.16;
pub const DASH_SPEED: f32 = 950.0;
pub const DASH_IFRAMES: f32 = 0.22;
pub const DASH_COOLDOWN: f32 = 1.8;
pub const DASH_COOLDOWN_SWARM: f32 = 1.1;
pub const HULL_START: u32 = 3;
pub const HULL_MAX: u32 = 5;
pub const SPAWN_SHIELD: f32 = 1.5;
pub const COMBO_WINDOW: f32 = 2.5;
pub const GRAZE_RADIUS: f32 = 22.0;
pub const STEP_Y: f32 = 18.0;
pub const EDGE_LEFT: f32 = 24.0;
pub const EDGE_RIGHT: f32 = 876.0;
pub const INTERMISSION: f32 = 1.6;
const INPUT_BUFFER: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArmadaMode {
    Classic,
    BulletSwarm,
    DailyVibe,
    Zen,
}

impl ArmadaMode {
    pub const ALL: [ArmadaMode; 4] = [
        ArmadaMode::Classic,
        ArmadaMode::BulletSwarm,
        ArmadaMode::DailyVibe,
        ArmadaMode::Zen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ArmadaMode::Classic => "classic",
            ArmadaMode::BulletSwarm => "swarm",
            ArmadaMode::DailyVibe => "daily",
            ArmadaMode::Zen => "zen",
        }
    }

    pub fn from_id(id: &str) -> ArmadaMode {
        ArmadaMode::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(ArmadaMode::Classic)
    }

    pub fn dash_cooldown(self) -> f32 {
        if self == ArmadaMode::BulletSwarm {
            DASH_COOLDOWN_SWARM
        } else {
            DASH_COOLDOWN
        }
    }

    fn graze(self) -> (f32, u64) {
        if self == ArmadaMode::BulletSwarm {
            (6.0, 80)
        } else {
            (4.0, 50)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    HullDestroyed,
    DefenseBreach,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Dead(Cause),
}

/// One frame of player input (already mapped from the keyboard).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArmadaInput {
    /// `-1.0..=1.0`, negative = left.
    pub move_x: f32,
    pub fire_held: bool,
    pub fire_pressed: bool,
    pub dash_pressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bolt {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub w: f32,
    pub h: f32,
    /// Enemy bolts: has been inside the graze radius.
    pub near: bool,
    /// Graze already scored (or the bolt hit and can never score one).
    pub grazed: bool,
}

impl Bolt {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center(self.x, self.y, self.w, self.h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub x: f32,
    pub y: f32,
    pub kind: PowerKind,
}

impl Capsule {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center(self.x, self.y, 26.0, 20.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ufo {
    pub x: f32,
    pub dir: f32,
    pub flight: UfoFlight,
}

impl Ufo {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center(self.x, UFO_Y, 44.0, 18.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turret {
    /// Offset from the boss centre.
    pub dx: f32,
    pub hp: u32,
    pub alive: bool,
    pub cooldown: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Laser {
    Idle(f32),
    /// Warning line at `x`; the beam fires when `t` reaches 0.
    Telegraph {
        t: f32,
        x: f32,
    },
    /// The beam sweeps from `x` in direction `dir`.
    Fire {
        t: f32,
        x: f32,
        dir: f32,
    },
}

pub const TELEGRAPH_SECS: f32 = 0.9;
pub const BEAM_SECS: f32 = 0.8;
pub const BEAM_SWEEP_SPEED: f32 = 180.0;
pub const BEAM_WIDTH: f32 = 26.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Boss {
    pub x: f32,
    pub y: f32,
    pub core_hp: u32,
    pub core_max: u32,
    pub turrets: [Turret; 2],
    pub t: f32,
    pub laser: Laser,
    pub level: u32,
}

impl Boss {
    pub const W: f32 = 220.0;
    pub const H: f32 = 72.0;

    pub fn body(&self) -> Aabb {
        Aabb::from_center(self.x, self.y, Self::W, Self::H)
    }

    pub fn core(&self) -> Aabb {
        Aabb::from_center(self.x, self.y + 4.0, 56.0, 40.0)
    }

    pub fn turret_box(&self, i: usize) -> Aabb {
        Aabb::from_center(self.x + self.turrets[i].dx, self.y + 28.0, 30.0, 24.0)
    }

    /// The beam while firing.
    pub fn beam(&self) -> Option<Aabb> {
        match self.laser {
            Laser::Fire { x, .. } => {
                let top = self.y + Self::H * 0.5;
                Some(Aabb::new(
                    x - BEAM_WIDTH * 0.5,
                    top,
                    BEAM_WIDTH,
                    ARENA_H - top,
                ))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ship {
    pub x: f32,
    pub fire_cd: f32,
    pub dash_cd: f32,
    pub dash_t: f32,
    pub dash_dir: f32,
    pub iframes: f32,
    pub shield: f32,
    pub last_dir: f32,
}

impl Ship {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center(self.x, SHIP_Y, SHIP_W, SHIP_H)
    }

    pub fn invulnerable(&self) -> bool {
        self.iframes > 0.0 || self.shield > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Formation {
    /// Centre x of column 0 (before individual sway).
    pub x: f32,
    /// Centre y of row 0.
    pub y: f32,
    pub dir: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArmadaEvent {
    Fired {
        x: f32,
        y: f32,
        rail: bool,
    },
    EnemyHit {
        x: f32,
        y: f32,
        shield_broke: bool,
    },
    EnemyKilled {
        x: f32,
        y: f32,
        kind: EnemyKind,
        points: u64,
    },
    UfoSpawned,
    UfoKilled {
        x: f32,
        points: u64,
    },
    PlayerHit,
    Graze {
        x: f32,
        y: f32,
    },
    Dash,
    PowerUp {
        kind: PowerKind,
        x: f32,
        y: f32,
    },
    CapsuleDropped {
        x: f32,
        y: f32,
    },
    BunkerHit {
        x: f32,
        y: f32,
    },
    WaveStart {
        wave: u32,
        boss: bool,
    },
    WaveCleared {
        wave: u32,
    },
    BossTelegraph {
        x: f32,
    },
    BossFire,
    BossTurretDown {
        x: f32,
        y: f32,
    },
    BossDown,
    Emp,
    Died(Cause),
}

#[derive(Clone, Debug)]
pub struct ArmadaGame {
    pub mode: ArmadaMode,
    pub seed: u64,
    pub status: Status,
    pub wave: u32,
    pub score: u64,
    pub kills: u32,
    pub shots: u32,
    pub grazes: u32,
    pub elapsed: f64,
    pub hull: u32,
    pub ship: Ship,
    pub formation: Formation,
    pub enemies: Vec<Enemy>,
    pub bunkers: [Bunker; 3],
    pub player_bolts: Vec<Bolt>,
    pub enemy_bolts: Vec<Bolt>,
    pub capsules: Vec<Capsule>,
    pub ufo: Option<Ufo>,
    pub boss: Option<Boss>,
    pub flow: FlowEngine,
    pub combo: u32,
    pub combo_timer: f32,
    /// Active weapon upgrade and its remaining time.
    pub weapon: Option<(PowerKind, f32)>,
    pub overclock: f32,
    pub intermission: f32,
    /// Remaining time of the rail beam flash and its x (rendering only).
    pub rail_flash: f32,
    pub rail_x: f32,
    total_enemies: usize,
    enemy_fire_cd: f32,
    ufo_plan: UfoPlan,
    play_rng: SplitMix64,
    acc: f32,
    pending_fire: f32,
    pending_dash: f32,
    events: Vec<ArmadaEvent>,
}

fn bunker_positions() -> [f32; 3] {
    // Left edges so the three domes are evenly spread over the arena.
    let w = bunker::COLS as f32 * bunker::CELL;
    [180.0 - w * 0.5, 450.0 - w * 0.5, 720.0 - w * 0.5]
}

impl ArmadaGame {
    pub fn new(mode: ArmadaMode, seed: u64) -> Self {
        Self::new_at_wave(mode, seed, 1)
    }

    /// Starts directly at `wave` (used by tests and the boss checks).
    pub fn new_at_wave(mode: ArmadaMode, seed: u64, wave: u32) -> Self {
        let xs = bunker_positions();
        let mut g = ArmadaGame {
            mode,
            seed,
            status: Status::Playing,
            wave: 0,
            score: 0,
            kills: 0,
            shots: 0,
            grazes: 0,
            elapsed: 0.0,
            hull: HULL_START,
            ship: Ship {
                x: ARENA_W * 0.5,
                fire_cd: 0.0,
                dash_cd: 0.0,
                dash_t: 0.0,
                dash_dir: 1.0,
                iframes: 0.0,
                shield: 0.0,
                last_dir: 1.0,
            },
            formation: Formation {
                x: 0.0,
                y: 0.0,
                dir: 1.0,
            },
            enemies: Vec::new(),
            bunkers: [
                Bunker::new(xs[0], BUNKER_Y),
                Bunker::new(xs[1], BUNKER_Y),
                Bunker::new(xs[2], BUNKER_Y),
            ],
            player_bolts: Vec::new(),
            enemy_bolts: Vec::new(),
            capsules: Vec::new(),
            ufo: None,
            boss: None,
            flow: FlowEngine::new(),
            combo: 0,
            combo_timer: 0.0,
            weapon: None,
            overclock: 0.0,
            intermission: 0.0,
            rail_flash: 0.0,
            rail_x: 0.0,
            total_enemies: 1,
            enemy_fire_cd: 1.0,
            ufo_plan: UfoPlan::new(seed),
            play_rng: SplitMix64::new(seed ^ 0xBADC_0FFE_E0DD_F00D),
            acc: 0.0,
            pending_fire: 0.0,
            pending_dash: 0.0,
            events: Vec::new(),
        };
        g.start_wave(wave.max(1));
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
        self.ship.dash_cd <= 0.0
    }

    pub fn drain_events(&mut self) -> Vec<ArmadaEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn alive_enemies(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive).count()
    }

    /// Wobble of Phase Weavers around their slot.
    fn sway(&self, e: &Enemy) -> f32 {
        if e.kind == EnemyKind::Weaver {
            14.0 * (self.elapsed as f32 * 2.2 + e.col as f32 * 0.9).sin()
        } else {
            0.0
        }
    }

    /// Current centre of an enemy.
    pub fn enemy_pos(&self, e: &Enemy) -> (f32, f32) {
        (
            self.formation.x + e.col as f32 * SLOT_X + self.sway(e),
            self.formation.y + e.row as f32 * SLOT_Y,
        )
    }

    pub fn enemy_box(&self, e: &Enemy) -> Aabb {
        let (x, y) = self.enemy_pos(e);
        let (w, h) = e.kind.size();
        Aabb::from_center(x, y, w, h)
    }

    /// Indices of the enemies allowed to fire: the lowest living enemy of
    /// every column.
    pub fn bottom_shooters(&self) -> Vec<usize> {
        let mut best: [Option<usize>; COLS] = [None; COLS];
        for (i, e) in self.enemies.iter().enumerate() {
            if !e.alive {
                continue;
            }
            match best[e.col] {
                Some(j) if self.enemies[j].row >= e.row => {}
                _ => best[e.col] = Some(i),
            }
        }
        best.into_iter().flatten().collect()
    }

    /// `(left, right)` extent of the living formation.
    pub fn extent(&self) -> Option<(f32, f32)> {
        let mut it = self
            .enemies
            .iter()
            .filter(|e| e.alive)
            .map(|e| self.enemy_box(e));
        let first = it.next()?;
        Some(it.fold((first.x, first.right()), |(l, r), b| {
            (l.min(b.x), r.max(b.right()))
        }))
    }

    pub fn enemy_bolt_cap(&self) -> usize {
        let base = 4 + (self.wave.saturating_sub(1) / 2) as usize;
        (base + (self.mode == ArmadaMode::BulletSwarm) as usize).min(7)
    }

    pub fn max_hull(&self) -> u32 {
        HULL_MAX
    }

    // ------------------------------------------------------------ waves

    fn start_wave(&mut self, wave: u32) {
        let spec: WaveSpec = wave_spec(self.seed, wave, self.mode);
        self.wave = wave;
        self.enemies = spec.enemies;
        self.total_enemies = self.enemies.len().max(1);
        self.enemy_bolts.clear();
        self.player_bolts.clear();
        self.capsules.clear();
        self.intermission = 0.0;
        let span = (COLS as f32 - 1.0) * SLOT_X;
        self.formation = Formation {
            x: (ARENA_W - span) * 0.5,
            y: 84.0 + 6.0 * (wave.saturating_sub(1)).min(6) as f32,
            dir: 1.0,
        };
        self.enemy_fire_cd = 1.0;
        self.boss = if spec.boss {
            let level = wave / 5;
            Some(Boss {
                x: ARENA_W * 0.5,
                y: 120.0,
                core_hp: 25 + 8 * (level - 1),
                core_max: 25 + 8 * (level - 1),
                turrets: [
                    Turret {
                        dx: -88.0,
                        hp: 6,
                        alive: true,
                        cooldown: 1.6,
                    },
                    Turret {
                        dx: 88.0,
                        hp: 6,
                        alive: true,
                        cooldown: 2.4,
                    },
                ],
                t: 0.0,
                laser: Laser::Idle(3.0),
                level,
            })
        } else {
            None
        };
        self.events.push(ArmadaEvent::WaveStart {
            wave,
            boss: spec.boss,
        });
    }

    fn formation_speed(&self) -> f32 {
        let base = 38.0 + 4.0 * self.wave as f32;
        let total = self.total_enemies as f32;
        let alive = self.alive_enemies().max(1) as f32;
        let mult = if self.mode == ArmadaMode::BulletSwarm {
            1.15
        } else {
            1.0
        };
        base * (1.0 + 0.9 * (total / alive).ln()) * mult
    }

    // ------------------------------------------------------------ main loop

    /// Advances by a frame of `dt` seconds (capped at 0.05 s).
    pub fn update(&mut self, dt: f32, input: &ArmadaInput) {
        if !self.is_playing() || !dt.is_finite() {
            return;
        }
        if input.fire_pressed {
            self.pending_fire = INPUT_BUFFER;
        }
        if input.dash_pressed {
            self.pending_dash = INPUT_BUFFER;
        }
        self.acc += dt.clamp(0.0, MAX_FRAME_DT);
        while self.acc >= STEP && self.is_playing() {
            self.acc -= STEP;
            self.step(input);
        }
    }

    /// One fixed step.
    pub fn step(&mut self, input: &ArmadaInput) {
        if !self.is_playing() {
            return;
        }
        let dt = STEP;
        self.elapsed += dt as f64;
        self.tick_timers(dt);
        if self.intermission > 0.0 {
            self.intermission -= dt;
            if self.intermission <= 0.0 {
                self.start_wave(self.wave + 1);
            }
        }
        self.move_ship(input.move_x.clamp(-1.0, 1.0), dt);
        self.handle_dash();
        self.handle_fire(input.fire_held);
        self.update_player_bolts(dt);
        if self.intermission <= 0.0 {
            self.update_formation(dt);
            self.update_boss(dt);
            self.update_ufo(dt);
        }
        self.update_enemy_bolts(dt);
        self.update_capsules(dt);
        self.flow.update(dt);
        self.check_wave_clear();
        self.check_breach();
    }

    fn tick_timers(&mut self, dt: f32) {
        let s = &mut self.ship;
        s.fire_cd = (s.fire_cd - dt).max(0.0);
        s.dash_cd = (s.dash_cd - dt).max(0.0);
        s.iframes = (s.iframes - dt).max(0.0);
        s.shield = (s.shield - dt).max(0.0);
        self.pending_fire = (self.pending_fire - dt).max(0.0);
        self.pending_dash = (self.pending_dash - dt).max(0.0);
        self.rail_flash = (self.rail_flash - dt).max(0.0);
        self.overclock = (self.overclock - dt).max(0.0);
        if let Some((_, t)) = &mut self.weapon {
            *t -= dt;
            if *t <= 0.0 {
                self.weapon = None;
            }
        }
        if self.combo_timer > 0.0 {
            self.combo_timer -= dt;
            if self.combo_timer <= 0.0 {
                self.combo = 0;
            }
        }
    }

    // ------------------------------------------------------------ ship

    fn move_ship(&mut self, move_x: f32, dt: f32) {
        if move_x != 0.0 {
            self.ship.last_dir = move_x.signum();
        }
        let speed = SHIP_SPEED * if self.overclock > 0.0 { 1.25 } else { 1.0 };
        let s = &mut self.ship;
        if s.dash_t > 0.0 {
            s.x += s.dash_dir * DASH_SPEED * dt;
            s.dash_t = (s.dash_t - dt).max(0.0);
        } else {
            s.x += move_x * speed * dt;
        }
        s.x = s.x.clamp(SHIP_MIN_X, SHIP_MAX_X);
    }

    fn handle_dash(&mut self) {
        if self.pending_dash > 0.0 && self.ship.dash_cd <= 0.0 {
            self.pending_dash = 0.0;
            let cd = self.mode.dash_cooldown();
            let s = &mut self.ship;
            s.dash_dir = s.last_dir;
            s.dash_t = DASH_DURATION;
            s.iframes = DASH_IFRAMES;
            s.dash_cd = cd;
            self.events.push(ArmadaEvent::Dash);
        }
    }

    fn handle_fire(&mut self, held: bool) {
        if (held || self.pending_fire > 0.0) && self.ship.fire_cd <= 0.0 && self.fire() {
            self.pending_fire = 0.0;
        }
    }

    /// Fires the current weapon. Returns `false` if the salvo limit blocks it.
    fn fire(&mut self) -> bool {
        let x = self.ship.x;
        let y = SHIP_Y - SHIP_H * 0.5;
        let weapon = self.weapon.map(|w| w.0);
        if weapon == Some(PowerKind::RailBeam) {
            self.shots += 1;
            self.fire_rail();
        } else {
            let limit = if weapon == Some(PowerKind::TriSpread) {
                3 * MAX_STD_BOLTS
            } else {
                MAX_STD_BOLTS
            };
            if self.player_bolts.len() >= limit {
                return false;
            }
            let mk = |x: f32, vx: f32| Bolt {
                x,
                y,
                vx,
                vy: -BOLT_SPEED,
                w: 4.0,
                h: 14.0,
                near: false,
                grazed: false,
            };
            if weapon == Some(PowerKind::TriSpread) {
                for ang in [-0.2f32, 0.0, 0.2] {
                    let mut b = mk(x, BOLT_SPEED * ang.sin());
                    b.vy = -BOLT_SPEED * ang.cos();
                    self.player_bolts.push(b);
                }
            } else {
                self.player_bolts.push(mk(x, 0.0));
            }
            if self.overdrive() {
                // Wing-tip blasters of the Overdrive state.
                self.player_bolts.push(mk(x - 16.0, 0.0));
                self.player_bolts.push(mk(x + 16.0, 0.0));
            }
            self.shots += 1;
            self.events.push(ArmadaEvent::Fired { x, y, rail: false });
        }
        let cd = FIRE_COOLDOWN * if self.overclock > 0.0 { 0.5 } else { 1.0 };
        self.ship.fire_cd = cd;
        true
    }

    /// RAIL-BEAM: an instant vertical ray that pierces everything in its band.
    fn fire_rail(&mut self) {
        let x = self.ship.x;
        self.rail_flash = 0.12;
        self.rail_x = x;
        self.events.push(ArmadaEvent::Fired {
            x,
            y: SHIP_Y,
            rail: true,
        });
        let band = Aabb::new(x - 5.0, 0.0, 10.0, SHIP_Y - SHIP_H * 0.5);
        for i in 0..self.enemies.len() {
            if self.enemies[i].alive && self.enemy_box(&self.enemies[i]).overlaps(&band) {
                self.damage_enemy(i, 2);
            }
        }
        if let Some(u) = self.ufo
            && u.aabb().overlaps(&band)
        {
            self.kill_ufo();
        }
        if self.boss.is_some() {
            self.damage_boss_with(&band, 2);
        }
        // The ray also chips the lowest bunker cell of every dome in its way.
        for b in 0..self.bunkers.len() {
            if let Some((r, c)) = self.bunkers[b].hit(&band, true) {
                let cb = self.bunkers[b].cell_box(r, c);
                self.events.push(ArmadaEvent::BunkerHit {
                    x: cb.center().0,
                    y: cb.center().1,
                });
            }
        }
    }

    fn hit_player(&mut self) {
        self.hull = self.hull.saturating_sub(1);
        self.ship.shield = SPAWN_SHIELD;
        self.weapon = None;
        self.overclock = 0.0;
        self.combo = 0;
        self.flow.flow = (self.flow.flow - 20.0).max(0.0);
        self.events.push(ArmadaEvent::PlayerHit);
        if self.hull == 0 {
            if self.mode == ArmadaMode::Zen {
                self.emp_reset();
            } else {
                self.status = Status::Dead(Cause::HullDestroyed);
                self.events.push(ArmadaEvent::Died(Cause::HullDestroyed));
            }
        }
    }

    /// Zen: an automatic EMP wipes enemy fire and gently restarts the wave.
    fn emp_reset(&mut self) {
        self.events.push(ArmadaEvent::Emp);
        self.hull = HULL_START;
        self.ship.shield = SPAWN_SHIELD;
        let wave = self.wave;
        self.start_wave(wave);
    }

    fn check_breach(&mut self) {
        let breached = self
            .enemies
            .iter()
            .filter(|e| e.alive)
            .any(|e| self.enemy_box(e).bottom() >= DEFENSE_Y);
        if !breached {
            return;
        }
        if self.mode == ArmadaMode::Zen {
            self.emp_reset();
        } else {
            self.status = Status::Dead(Cause::DefenseBreach);
            self.events.push(ArmadaEvent::Died(Cause::DefenseBreach));
        }
    }

    // ------------------------------------------------------------ scoring

    fn award(&mut self, base: u64) -> u64 {
        let pts = (base as f32 * self.flow.multiplier()).round() as u64;
        self.score += pts;
        pts
    }

    fn register_kill(&mut self) {
        self.kills += 1;
        self.combo = if self.combo_timer > 0.0 {
            self.combo + 1
        } else {
            1
        };
        self.combo_timer = COMBO_WINDOW;
        self.flow
            .register_event(3.0 + 0.8 * self.combo.min(10) as f32);
    }

    fn damage_enemy(&mut self, i: usize, dmg: u8) {
        let (x, y) = self.enemy_pos(&self.enemies[i]);
        let e = &mut self.enemies[i];
        if !e.alive {
            return;
        }
        if e.hp > dmg {
            e.hp -= dmg;
            let broke = e.kind == EnemyKind::Aegis;
            self.events.push(ArmadaEvent::EnemyHit {
                x,
                y,
                shield_broke: broke,
            });
            return;
        }
        e.hp = 0;
        e.alive = false;
        let kind = e.kind;
        let drop = e.drop;
        let points = self.award(kind.points());
        self.register_kill();
        self.events
            .push(ArmadaEvent::EnemyKilled { x, y, kind, points });
        if let Some(k) = drop {
            self.capsules.push(Capsule { x, y, kind: k });
            self.events.push(ArmadaEvent::CapsuleDropped { x, y });
        }
    }

    fn kill_ufo(&mut self) {
        let Some(u) = self.ufo.take() else { return };
        let points = self.award(u.flight.points);
        self.register_kill();
        self.flow.register_event(6.0);
        self.capsules.push(Capsule {
            x: u.x,
            y: UFO_Y,
            kind: u.flight.drop,
        });
        self.events.push(ArmadaEvent::UfoKilled { x: u.x, points });
    }

    fn damage_boss_with(&mut self, area: &Aabb, dmg: u32) -> bool {
        let Some(mut b) = self.boss.take() else {
            return false;
        };
        let mut hit = false;
        for i in 0..2 {
            if b.turrets[i].alive && b.turret_box(i).overlaps(area) {
                hit = true;
                b.turrets[i].hp = b.turrets[i].hp.saturating_sub(dmg);
                if b.turrets[i].hp == 0 {
                    b.turrets[i].alive = false;
                    let c = b.turret_box(i).center();
                    self.award(400);
                    self.events
                        .push(ArmadaEvent::BossTurretDown { x: c.0, y: c.1 });
                }
            }
        }
        if !hit && b.core().overlaps(area) {
            hit = true;
            b.core_hp = b.core_hp.saturating_sub(dmg);
            self.flow.register_event(1.0);
            if b.core_hp == 0 {
                self.award(3000 * b.level as u64);
                self.events.push(ArmadaEvent::BossDown);
                self.kills += 1;
                // Boss gone: the wave is cleared below.
                self.boss = None;
                self.enemy_bolts.clear();
                return true;
            }
        } else if !hit && b.body().overlaps(area) {
            hit = true; // armour plating absorbs the shot
        }
        self.boss = Some(b);
        hit
    }

    // ------------------------------------------------------------ player bolts

    fn update_player_bolts(&mut self, dt: f32) {
        let mut bolts = std::mem::take(&mut self.player_bolts);
        bolts.retain_mut(|b| {
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            !(b.y < -20.0 || b.x < -20.0 || b.x > ARENA_W + 20.0)
        });
        let mut keep = Vec::with_capacity(bolts.len());
        for b in bolts {
            if !self.player_bolt_hits(&b) {
                keep.push(b);
            }
        }
        self.player_bolts = keep;
    }

    /// `true` if the bolt hit something (and is consumed).
    fn player_bolt_hits(&mut self, b: &Bolt) -> bool {
        let area = b.aabb();
        for i in 0..self.bunkers.len() {
            if let Some((r, c)) = self.bunkers[i].hit(&area, true) {
                let cb = self.bunkers[i].cell_box(r, c);
                self.events.push(ArmadaEvent::BunkerHit {
                    x: cb.center().0,
                    y: cb.center().1,
                });
                return true;
            }
        }
        for i in 0..self.enemies.len() {
            if self.enemies[i].alive && self.enemy_box(&self.enemies[i]).overlaps(&area) {
                self.damage_enemy(i, 1);
                return true;
            }
        }
        if let Some(u) = self.ufo
            && u.aabb().overlaps(&area)
        {
            self.kill_ufo();
            return true;
        }
        if self.boss.is_some() {
            return self.damage_boss_with(&area, 1);
        }
        false
    }

    // ------------------------------------------------------------ enemies

    fn update_formation(&mut self, dt: f32) {
        if self.alive_enemies() == 0 {
            return;
        }
        let speed = self.formation_speed();
        self.formation.x += self.formation.dir * speed * dt;
        // Edge bounce on the extent of the LIVING enemies; only reverse when
        // moving towards the edge, so a clamped formation never re-triggers.
        if let Some((l, r)) = self.extent() {
            if self.formation.dir > 0.0 && r >= EDGE_RIGHT {
                self.formation.x -= r - EDGE_RIGHT;
                self.formation.dir = -1.0;
                self.formation.y += STEP_Y;
            } else if self.formation.dir < 0.0 && l <= EDGE_LEFT {
                self.formation.x += EDGE_LEFT - l;
                self.formation.dir = 1.0;
                self.formation.y += STEP_Y;
            }
        }
        // Crushing bunker cells by touching them.
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            let b = self.enemy_box(&self.enemies[i]);
            for k in 0..self.bunkers.len() {
                self.bunkers[k].crush(&b);
            }
        }
        self.enemy_fire_cd -= dt;
        if self.enemy_fire_cd <= 0.0 {
            self.enemy_fire();
        }
    }

    fn enemy_fire(&mut self) {
        let base = (1.2 - 0.06 * self.wave as f32).max(0.35)
            * if self.mode == ArmadaMode::BulletSwarm {
                0.7
            } else {
                1.0
            };
        self.enemy_fire_cd = base * (0.6 + 0.8 * self.play_rng.next_f32());
        if self.enemy_bolts.len() >= self.enemy_bolt_cap() {
            return;
        }
        let shooters = self.bottom_shooters();
        if shooters.is_empty() {
            return;
        }
        let idx = shooters[self.play_rng.next_below(shooters.len() as u64) as usize];
        let e = self.enemies[idx].clone();
        let (x, y) = self.enemy_pos(&e);
        let speed = (250.0 + 12.0 * self.wave as f32).min(420.0)
            * if self.mode == ArmadaMode::BulletSwarm {
                1.15
            } else {
                1.0
            };
        let mk = |x: f32, vx: f32| Bolt {
            x,
            y: y + 12.0,
            vx,
            vy: speed,
            w: 6.0,
            h: 12.0,
            near: false,
            grazed: false,
        };
        if e.kind == EnemyKind::Interceptor {
            // Aimed at the ship's x; sometimes as a twin shot.
            let travel = ((SHIP_Y - y) / speed).max(0.2);
            let vx = ((self.ship.x - x) / travel).clamp(-150.0, 150.0);
            let twin = self.play_rng.next_below(100) < 40;
            if twin && self.enemy_bolts.len() + 2 <= 7 {
                self.enemy_bolts.push(mk(x - 8.0, vx));
                self.enemy_bolts.push(mk(x + 8.0, vx));
            } else {
                self.enemy_bolts.push(mk(x, vx));
            }
        } else {
            self.enemy_bolts.push(mk(x, 0.0));
        }
    }

    // ------------------------------------------------------------ enemy bolts

    fn update_enemy_bolts(&mut self, dt: f32) {
        let mut bolts = std::mem::take(&mut self.enemy_bolts);
        let mut keep = Vec::with_capacity(bolts.len());
        for mut b in bolts.drain(..) {
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            if b.y > ARENA_H + 20.0 || b.x < -20.0 || b.x > ARENA_W + 20.0 {
                continue;
            }
            if self.enemy_bolt_hits(&mut b) {
                continue;
            }
            keep.push(b);
        }
        // Bolts spawned while iterating (none) would be lost; the vector
        // only gets new bolts in `enemy_fire`, outside this loop.
        keep.append(&mut self.enemy_bolts);
        self.enemy_bolts = keep;
    }

    /// `true` if the bolt is consumed.
    fn enemy_bolt_hits(&mut self, b: &mut Bolt) -> bool {
        let area = b.aabb();
        for i in 0..self.bunkers.len() {
            if let Some((r, c)) = self.bunkers[i].hit(&area, false) {
                let cb = self.bunkers[i].cell_box(r, c);
                self.events.push(ArmadaEvent::BunkerHit {
                    x: cb.center().0,
                    y: cb.center().1,
                });
                return true;
            }
        }
        let ship = self.ship.aabb();
        if ship.overlaps(&area) {
            if self.ship.invulnerable() {
                // i-frames / shield: the bolt passes through the ship.
                b.near = true;
                return false;
            }
            b.grazed = true; // a hit never counts as a graze
            self.hit_player();
            return true;
        }
        // Graze: scored once, when a bolt that came close leaves the radius.
        let d = ship.distance_to(b.x, b.y);
        if d <= GRAZE_RADIUS {
            b.near = true;
        } else if b.near && !b.grazed {
            b.grazed = true;
            let (flow, pts) = self.mode.graze();
            self.grazes += 1;
            self.flow.register_event(flow);
            self.score += pts;
            self.events.push(ArmadaEvent::Graze {
                x: self.ship.x,
                y: SHIP_Y,
            });
        }
        false
    }

    // ------------------------------------------------------------ boss, ufo

    fn update_boss(&mut self, dt: f32) {
        let Some(mut b) = self.boss.take() else {
            return;
        };
        b.t += dt;
        b.x = ARENA_W * 0.5 + 220.0 * (b.t * 0.6).sin();
        // Turrets fire aimed bolts.
        for i in 0..2 {
            if !b.turrets[i].alive {
                continue;
            }
            b.turrets[i].cooldown -= dt;
            if b.turrets[i].cooldown <= 0.0 {
                b.turrets[i].cooldown = 2.2 - 0.2 * (b.level.min(5)) as f32;
                let (tx, ty) = b.turret_box(i).center();
                let speed = 300.0;
                let travel = ((SHIP_Y - ty) / speed).max(0.2);
                let vx = ((self.ship.x - tx) / travel).clamp(-160.0, 160.0);
                self.enemy_bolts.push(Bolt {
                    x: tx,
                    y: ty + 12.0,
                    vx,
                    vy: speed,
                    w: 8.0,
                    h: 14.0,
                    near: false,
                    grazed: false,
                });
            }
        }
        // Laser sweep: idle -> telegraph (0.9 s warning) -> beam.
        b.laser = match b.laser {
            Laser::Idle(t) => {
                if t - dt <= 0.0 {
                    self.events
                        .push(ArmadaEvent::BossTelegraph { x: self.ship.x });
                    Laser::Telegraph {
                        t: TELEGRAPH_SECS,
                        x: self.ship.x,
                    }
                } else {
                    Laser::Idle(t - dt)
                }
            }
            Laser::Telegraph { t, x } => {
                if t - dt <= 0.0 {
                    self.events.push(ArmadaEvent::BossFire);
                    let dir = if self.ship.x >= x { 1.0 } else { -1.0 };
                    Laser::Fire {
                        t: BEAM_SECS,
                        x,
                        dir,
                    }
                } else {
                    Laser::Telegraph { t: t - dt, x }
                }
            }
            Laser::Fire { t, x, dir } => {
                if t - dt <= 0.0 {
                    Laser::Idle((3.4 - 0.3 * b.level.min(5) as f32).max(1.8))
                } else {
                    Laser::Fire {
                        t: t - dt,
                        x: (x + dir * BEAM_SWEEP_SPEED * dt).clamp(20.0, ARENA_W - 20.0),
                        dir,
                    }
                }
            }
        };
        let beam = b.beam();
        self.boss = Some(b);
        if let Some(beam) = beam
            && self.ship.aabb().overlaps(&beam)
            && !self.ship.invulnerable()
        {
            self.hit_player();
        }
    }

    fn update_ufo(&mut self, dt: f32) {
        if self.elapsed >= self.ufo_plan.next_at {
            let flight = self.ufo_plan.take();
            if self.boss.is_none() && self.ufo.is_none() {
                self.ufo = Some(Ufo {
                    x: if flight.from_left {
                        -30.0
                    } else {
                        ARENA_W + 30.0
                    },
                    dir: if flight.from_left { 1.0 } else { -1.0 },
                    flight,
                });
                self.events.push(ArmadaEvent::UfoSpawned);
            }
        }
        if let Some(u) = &mut self.ufo {
            u.x += u.dir * 110.0 * dt;
            if u.x < -40.0 || u.x > ARENA_W + 40.0 {
                self.ufo = None;
            }
        }
    }

    // ------------------------------------------------------------ capsules

    fn update_capsules(&mut self, dt: f32) {
        let ship = self.ship.aabb();
        let mut caps = std::mem::take(&mut self.capsules);
        caps.retain_mut(|c| {
            c.y += 90.0 * dt;
            c.y < ARENA_H + 20.0
        });
        let mut keep = Vec::with_capacity(caps.len());
        for c in caps {
            if c.aabb().overlaps(&ship) {
                self.collect(c);
            } else {
                keep.push(c);
            }
        }
        keep.append(&mut self.capsules);
        self.capsules = keep;
    }

    fn collect(&mut self, c: Capsule) {
        self.score += 150;
        self.events.push(ArmadaEvent::PowerUp {
            kind: c.kind,
            x: c.x,
            y: c.y,
        });
        match c.kind {
            PowerKind::TriSpread | PowerKind::RailBeam => {
                self.weapon = Some((c.kind, c.kind.duration()));
            }
            PowerKind::Overclock => self.overclock = c.kind.duration(),
            PowerKind::AegisRepair => {
                if self.hull < HULL_MAX {
                    self.hull += 1;
                } else {
                    for b in &mut self.bunkers {
                        b.repair();
                    }
                }
            }
            PowerKind::VibeSurge => {
                self.flow.register_event(30.0);
                self.enemy_bolts.clear();
                self.events.push(ArmadaEvent::Emp);
            }
        }
    }

    // ------------------------------------------------------------ waves

    fn check_wave_clear(&mut self) {
        if self.intermission > 0.0 {
            return;
        }
        let cleared = if is_boss_wave(self.wave) {
            self.boss.is_none()
        } else {
            self.alive_enemies() == 0
        };
        if cleared {
            let wave = self.wave;
            self.award(500 * wave as u64);
            self.enemy_bolts.clear();
            self.intermission = INTERMISSION;
            self.events.push(ArmadaEvent::WaveCleared { wave });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> ArmadaInput {
        ArmadaInput::default()
    }

    fn run(g: &mut ArmadaGame, seconds: f32, input: ArmadaInput) {
        for _ in 0..(seconds / STEP) as usize {
            g.step(&input);
        }
    }

    fn enemy_bolt(x: f32, y: f32) -> Bolt {
        Bolt {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            w: 6.0,
            h: 12.0,
            near: false,
            grazed: false,
        }
    }

    /// Game with all enemy fire suppressed.
    fn quiet(mode: ArmadaMode) -> ArmadaGame {
        let mut g = ArmadaGame::new(mode, 7);
        g.enemy_fire_cd = 1e9;
        g
    }

    #[test]
    fn starts_with_three_hull_and_a_full_first_wave() {
        let g = ArmadaGame::new(ArmadaMode::Classic, 1);
        assert_eq!((g.hull, g.wave, g.ship.x), (3, 1, 450.0));
        assert_eq!(g.alive_enemies(), 40);
        assert_eq!(g.bunkers.len(), 3);
        assert!(g.bunkers.iter().all(|b| b.alive() == Bunker::full_count()));
        assert_eq!(g.bunkers[0].y, 505.0);
    }

    #[test]
    fn player_bolt_hits_enemy_and_scores_with_the_flow_multiplier() {
        let mut g = quiet(ArmadaMode::Classic);
        let i = g
            .enemies
            .iter()
            .position(|e| e.kind == EnemyKind::Scout)
            .unwrap();
        let (x, y) = g.enemy_pos(&g.enemies[i].clone());
        g.player_bolts.push(Bolt {
            x,
            y: y + 20.0,
            vx: 0.0,
            vy: -BOLT_SPEED,
            w: 4.0,
            h: 14.0,
            near: false,
            grazed: false,
        });
        run(&mut g, 0.2, idle());
        assert!(!g.enemies[i].alive);
        assert_eq!(g.score, 100);
        assert_eq!(g.kills, 1);
        assert_eq!(g.combo, 1);
        // In Sync (1.5x) the same kill is worth more.
        let mut g = quiet(ArmadaMode::Classic);
        g.flow.flow = 70.0;
        let i = g
            .enemies
            .iter()
            .position(|e| e.kind == EnemyKind::Scout)
            .unwrap();
        g.damage_enemy(i, 1);
        assert_eq!(g.score, 150);
    }

    #[test]
    fn aegis_needs_two_hits_and_reports_the_shield_break() {
        let mut g = quiet(ArmadaMode::Classic);
        let i = g
            .enemies
            .iter()
            .position(|e| e.kind == EnemyKind::Aegis)
            .unwrap();
        g.damage_enemy(i, 1);
        assert!(g.enemies[i].alive && g.enemies[i].hp == 1);
        assert!(g.drain_events().iter().any(|e| matches!(
            e,
            ArmadaEvent::EnemyHit {
                shield_broke: true,
                ..
            }
        )));
        assert_eq!(g.score, 0, "no points for breaking the shield");
        g.damage_enemy(i, 1);
        assert!(!g.enemies[i].alive);
        assert_eq!(g.score, 250);
    }

    #[test]
    fn enemy_points_match_the_archetypes() {
        let pts: Vec<u64> = [
            EnemyKind::Scout,
            EnemyKind::Interceptor,
            EnemyKind::Aegis,
            EnemyKind::Weaver,
        ]
        .iter()
        .map(|k| k.points())
        .collect();
        assert_eq!(pts, vec![100, 150, 250, 300]);
    }

    #[test]
    fn bunker_cells_fall_to_player_and_enemy_fire() {
        let mut g = quiet(ArmadaMode::Classic);
        let before = g.bunkers[1].alive();
        // Enemy bolt from above.
        g.enemy_bolts.push(Bolt {
            vy: 300.0,
            ..enemy_bolt(450.0, 495.0)
        });
        run(&mut g, 0.1, idle());
        assert_eq!(g.bunkers[1].alive(), before - 1);
        // Player bolt from below, a different column.
        g.player_bolts.push(Bolt {
            x: 436.0,
            y: 545.0,
            vx: 0.0,
            vy: -BOLT_SPEED,
            w: 4.0,
            h: 14.0,
            near: false,
            grazed: false,
        });
        run(&mut g, 0.2, idle());
        assert_eq!(g.bunkers[1].alive(), before - 2);
    }

    #[test]
    fn formation_bounces_at_both_edges_and_steps_down_exactly_18() {
        let mut g = quiet(ArmadaMode::Classic);
        let y0 = g.formation.y;
        let mut reversals = 0;
        let mut last_dir = g.formation.dir;
        let mut last_y = y0;
        for _ in 0..(40.0 / STEP) as usize {
            g.enemy_bolts.clear();
            g.step(&idle());
            if !g.is_playing() {
                break;
            }
            if g.formation.dir != last_dir {
                reversals += 1;
                assert!(
                    (g.formation.y - last_y - STEP_Y).abs() < 1e-3,
                    "steps down 18"
                );
                last_dir = g.formation.dir;
                last_y = g.formation.y;
                let (l, r) = g.extent().unwrap();
                assert!(
                    l >= EDGE_LEFT - 0.01 && r <= EDGE_RIGHT + 0.01,
                    "clamped inside"
                );
            }
            if reversals >= 3 {
                break;
            }
        }
        assert!(reversals >= 3, "formation keeps bouncing");
    }

    #[test]
    fn killing_the_outer_column_at_the_edge_does_not_stick_or_flip_flop() {
        let mut g = quiet(ArmadaMode::Classic);
        // Run until the formation touches the right edge.
        for _ in 0..(20.0 / STEP) as usize {
            g.step(&idle());
            if g.formation.dir < 0.0 {
                break;
            }
        }
        assert!(g.formation.dir < 0.0, "reached the right edge");
        let y_after = g.formation.y;
        // Kill the whole outermost live column while it sits at the edge.
        let right_col = g.enemies.iter().map(|e| e.col).max().unwrap();
        for i in 0..g.enemies.len() {
            if g.enemies[i].col == right_col {
                g.enemies[i].alive = false;
            }
        }
        let mut flips = 0;
        let mut d = g.formation.dir;
        let mut x_prev = g.formation.x;
        for _ in 0..(1.0 / STEP) as usize {
            g.step(&idle());
            if g.formation.dir != d {
                flips += 1;
                d = g.formation.dir;
            }
        }
        assert_eq!(flips, 0, "no re-reversal after the outer column died");
        assert!(g.formation.x < x_prev, "moves away from the edge");
        assert_eq!(g.formation.y, y_after, "no extra step down");
        x_prev = g.formation.x;
        let _ = x_prev;
    }

    #[test]
    fn only_the_lowest_enemy_of_each_column_may_fire() {
        let mut g = quiet(ArmadaMode::Classic);
        let shooters = g.bottom_shooters();
        assert_eq!(shooters.len(), COLS);
        for &i in &shooters {
            let e = &g.enemies[i];
            assert!(
                !g.enemies
                    .iter()
                    .any(|o| o.alive && o.col == e.col && o.row > e.row)
            );
        }
        // Kill the bottom enemy of column 3: the one above takes over.
        let bottom = *shooters.iter().find(|&&i| g.enemies[i].col == 3).unwrap();
        let row = g.enemies[bottom].row;
        g.enemies[bottom].alive = false;
        let now = g.bottom_shooters();
        let new = *now.iter().find(|&&i| g.enemies[i].col == 3).unwrap();
        assert_eq!(g.enemies[new].row, row - 1);
        // An entirely dead column has no shooter.
        for e in g.enemies.iter_mut().filter(|e| e.col == 3) {
            e.alive = false;
        }
        assert_eq!(g.bottom_shooters().len(), COLS - 1);
    }

    #[test]
    fn every_enemy_bolt_comes_from_a_column_bottom_and_the_cap_holds() {
        for wave in [1u32, 5 - 1, 9] {
            let mut g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 99, wave);
            g.ship.shield = 1e9; // survive to observe
            let cap = g.enemy_bolt_cap();
            assert!((4..=7).contains(&cap));
            let mut max_seen = 0;
            for _ in 0..(8.0 / STEP) as usize {
                g.step(&idle());
                max_seen = max_seen.max(g.enemy_bolts.len());
                if !g.is_playing() {
                    break;
                }
            }
            assert!(max_seen <= cap + 1, "wave {wave}: {max_seen} > cap {cap}");
            assert!(max_seen > 0, "enemies do fire");
        }
        let mut swarm = ArmadaGame::new(ArmadaMode::BulletSwarm, 1);
        swarm.wave = 20;
        assert_eq!(swarm.enemy_bolt_cap(), 7);
    }

    #[test]
    fn phase_dash_grants_exactly_022_seconds_of_i_frames_and_has_a_cooldown() {
        let mut g = quiet(ArmadaMode::Classic);
        let x0 = g.ship.x;
        g.update(
            0.0,
            &ArmadaInput {
                dash_pressed: true,
                move_x: 1.0,
                ..idle()
            },
        );
        g.step(&ArmadaInput {
            move_x: 1.0,
            ..idle()
        });
        assert!(g.ship.iframes > 0.2 && g.ship.dash_cd > 1.7);
        run(
            &mut g,
            DASH_DURATION,
            ArmadaInput {
                move_x: 1.0,
                ..idle()
            },
        );
        assert!(
            g.ship.x - x0 > 130.0,
            "dash moved ~152 units: {}",
            g.ship.x - x0
        );
        // A bolt on the ship inside the i-frames does no damage.
        let mut g2 = quiet(ArmadaMode::Classic);
        g2.update(
            0.0,
            &ArmadaInput {
                dash_pressed: true,
                ..idle()
            },
        );
        g2.step(&idle());
        g2.ship.x = 450.0;
        g2.enemy_bolts.push(enemy_bolt(g2.ship.x, SHIP_Y));
        g2.step(&idle());
        assert_eq!(g2.hull, 3, "i-frames protect");
        assert_eq!(g2.enemy_bolts.len(), 1, "the bolt passes through");
        // After the i-frames the same overlap hurts.
        run(&mut g2, 0.25, idle());
        g2.enemy_bolts.clear();
        g2.enemy_bolts.push(enemy_bolt(g2.ship.x, SHIP_Y));
        g2.ship.shield = 0.0;
        g2.step(&idle());
        assert_eq!(g2.hull, 2, "no i-frames any more");
    }

    #[test]
    fn dash_cooldown_is_1_8_seconds_and_1_1_in_bullet_swarm() {
        for (mode, cd) in [
            (ArmadaMode::Classic, 1.8f32),
            (ArmadaMode::BulletSwarm, 1.1),
        ] {
            let mut g = quiet(mode);
            g.update(
                0.0,
                &ArmadaInput {
                    dash_pressed: true,
                    ..idle()
                },
            );
            g.step(&idle());
            assert!(!g.dash_ready());
            run(&mut g, cd - 0.1, idle());
            assert!(!g.dash_ready(), "{mode:?}: not ready before {cd}");
            run(&mut g, 0.2, idle());
            assert!(g.dash_ready(), "{mode:?}: ready after {cd}");
        }
    }

    #[test]
    fn a_hit_costs_hull_grants_a_spawn_shield_and_clears_upgrades() {
        let mut g = quiet(ArmadaMode::Classic);
        g.weapon = Some((PowerKind::TriSpread, 5.0));
        g.overclock = 5.0;
        g.enemy_bolts.push(enemy_bolt(g.ship.x, SHIP_Y));
        g.step(&idle());
        assert_eq!(g.hull, 2);
        assert!((g.ship.shield - SPAWN_SHIELD).abs() < 0.02);
        assert!(g.weapon.is_none() && g.overclock == 0.0);
        // The shield protects against the next hit.
        g.enemy_bolts.push(enemy_bolt(g.ship.x, SHIP_Y));
        g.step(&idle());
        assert_eq!(g.hull, 2);
    }

    #[test]
    fn graze_is_scored_once_per_bolt_and_never_for_a_hit() {
        let mut g = quiet(ArmadaMode::Classic);
        g.ship.x = 450.0;
        // A bolt falling just beside the ship (within 22, no overlap).
        g.enemy_bolts.push(Bolt {
            vy: 200.0,
            ..enemy_bolt(450.0 + 15.0 + 3.0 + 8.0, 560.0)
        });
        run(&mut g, 1.0, idle());
        assert_eq!(
            g.grazes, 1,
            "scored exactly once even though it lingered in range"
        );
        assert_eq!(g.score, 50);
        assert!((g.flow.flow - 4.0).abs() < 0.2);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, ArmadaEvent::Graze { .. }))
        );
        // A direct hit is never a graze.
        let mut h = quiet(ArmadaMode::Classic);
        h.enemy_bolts.push(Bolt {
            vy: 200.0,
            ..enemy_bolt(h.ship.x, 560.0)
        });
        run(&mut h, 1.0, idle());
        assert_eq!((h.hull, h.grazes), (2, 0));
        // Bullet Swarm pays more.
        let mut s = quiet(ArmadaMode::BulletSwarm);
        s.enemy_bolts.push(Bolt {
            vy: 200.0,
            ..enemy_bolt(450.0 + 26.0, 560.0)
        });
        run(&mut s, 1.0, idle());
        assert_eq!((s.grazes, s.score), (1, 80));
    }

    #[test]
    fn a_far_bolt_is_no_graze() {
        let mut g = quiet(ArmadaMode::Classic);
        g.enemy_bolts.push(Bolt {
            vy: 200.0,
            ..enemy_bolt(450.0 + 100.0, 560.0)
        });
        run(&mut g, 1.0, idle());
        assert_eq!(g.grazes, 0);
    }

    #[test]
    fn fire_limits_cooldown_and_buffering() {
        let mut g = quiet(ArmadaMode::Classic);
        // A single tap shorter than one step still fires (buffered).
        g.update(
            0.001,
            &ArmadaInput {
                fire_pressed: true,
                ..idle()
            },
        );
        g.step(&idle());
        assert_eq!(g.player_bolts.len(), 1);
        // Holding fire: cooldown 0.22 s and at most 3 bolts in flight.
        let mut h = quiet(ArmadaMode::Classic);
        let mut max = 0;
        for _ in 0..(0.5 / STEP) as usize {
            h.step(&ArmadaInput {
                fire_held: true,
                ..idle()
            });
            max = max.max(h.player_bolts.len());
        }
        assert_eq!(h.shots, 3, "0.22 s cooldown: shots at 0, 0.22, 0.44");
        assert!(max <= 3);
        // Overclock halves the cooldown.
        let mut o = quiet(ArmadaMode::Classic);
        o.overclock = 10.0;
        run(
            &mut o,
            0.5,
            ArmadaInput {
                fire_held: true,
                ..idle()
            },
        );
        assert!(o.shots >= 5);
    }

    #[test]
    fn tri_spread_fires_three_and_rail_pierces_a_whole_column() {
        let mut g = quiet(ArmadaMode::Classic);
        g.weapon = Some((PowerKind::TriSpread, 10.0));
        g.step(&ArmadaInput {
            fire_held: true,
            ..idle()
        });
        assert_eq!(g.player_bolts.len(), 3);
        let xs: Vec<f32> = g.player_bolts.iter().map(|b| b.vx).collect();
        assert!(xs.iter().any(|v| *v < 0.0) && xs.iter().any(|v| *v > 0.0));

        let mut r = quiet(ArmadaMode::Classic);
        r.weapon = Some((PowerKind::RailBeam, 8.0));
        let col_x = r.formation.x + 4.0 * SLOT_X;
        r.ship.x = col_x;
        let in_col = r.enemies.iter().filter(|e| e.col == 4 && e.alive).count();
        assert!(in_col >= 3);
        r.step(&ArmadaInput {
            fire_held: true,
            ..idle()
        });
        // Rail damage 2 kills every enemy of the column, even armoured ones.
        let left = r.enemies.iter().filter(|e| e.col == 4 && e.alive).count();
        assert_eq!(left, 0, "the beam pierced the whole column");
        assert!(r.rail_flash > 0.0);
    }

    #[test]
    fn power_ups_apply_their_effects() {
        let mut g = quiet(ArmadaMode::Classic);
        g.hull = 2;
        g.collect(Capsule {
            x: 0.0,
            y: 0.0,
            kind: PowerKind::AegisRepair,
        });
        assert_eq!(g.hull, 3);
        // At full hull, repair restores bunker cells instead.
        g.hull = HULL_MAX;
        g.bunkers[0].cells[0][1] = false;
        g.collect(Capsule {
            x: 0.0,
            y: 0.0,
            kind: PowerKind::AegisRepair,
        });
        assert_eq!(g.hull, HULL_MAX);
        assert!(!g.bunkers[0].is_damaged());
        g.enemy_bolts.push(enemy_bolt(10.0, 10.0));
        let flow = g.flow.flow;
        g.collect(Capsule {
            x: 0.0,
            y: 0.0,
            kind: PowerKind::VibeSurge,
        });
        assert!(g.enemy_bolts.is_empty(), "EMP clears bolts");
        assert!((g.flow.flow - flow - 30.0).abs() < 1e-3);
        g.collect(Capsule {
            x: 0.0,
            y: 0.0,
            kind: PowerKind::Overclock,
        });
        assert_eq!(g.overclock, 10.0);
        g.collect(Capsule {
            x: 0.0,
            y: 0.0,
            kind: PowerKind::RailBeam,
        });
        assert_eq!(g.weapon, Some((PowerKind::RailBeam, 8.0)));
        assert_eq!(g.score, 5 * 150, "+150 per capsule");
    }

    #[test]
    fn weapon_upgrades_expire() {
        let mut g = quiet(ArmadaMode::Classic);
        g.weapon = Some((PowerKind::TriSpread, 0.05));
        run(&mut g, 0.1, idle());
        assert!(g.weapon.is_none());
    }

    #[test]
    fn overclock_speeds_the_ship_up_by_a_quarter() {
        let mut a = quiet(ArmadaMode::Classic);
        let mut b = quiet(ArmadaMode::Classic);
        b.overclock = 10.0;
        let (xa, xb) = (a.ship.x, b.ship.x);
        run(
            &mut a,
            0.2,
            ArmadaInput {
                move_x: 1.0,
                ..idle()
            },
        );
        run(
            &mut b,
            0.2,
            ArmadaInput {
                move_x: 1.0,
                ..idle()
            },
        );
        let ratio = (b.ship.x - xb) / (a.ship.x - xa);
        assert!((ratio - 1.25).abs() < 0.02, "{ratio}");
        // The ship never leaves its lane.
        run(
            &mut a,
            5.0,
            ArmadaInput {
                move_x: 1.0,
                ..idle()
            },
        );
        assert_eq!(a.ship.x, SHIP_MAX_X);
    }

    #[test]
    fn overdrive_adds_wing_blasters() {
        let mut g = quiet(ArmadaMode::Classic);
        g.flow.flow = 95.0;
        assert!(g.overdrive());
        g.step(&ArmadaInput {
            fire_held: true,
            ..idle()
        });
        assert_eq!(g.player_bolts.len(), 3);
    }

    #[test]
    fn ufo_flies_at_the_top_drops_a_capsule_and_scores_500_to_1500() {
        let mut g = quiet(ArmadaMode::Classic);
        g.ufo_plan.next_at = 0.0;
        g.step(&idle());
        g.ufo.as_mut().expect("ufo spawned").x = 300.0;
        let u = g.ufo.unwrap();
        assert!((500..=1500).contains(&u.flight.points));
        let drop = u.flight.drop;
        g.player_bolts.push(Bolt {
            x: u.x,
            y: UFO_Y + 12.0,
            vx: 0.0,
            vy: -BOLT_SPEED,
            w: 4.0,
            h: 14.0,
            near: false,
            grazed: false,
        });
        g.step(&idle());
        assert!(g.ufo.is_none());
        assert_eq!(
            g.capsules.last().map(|c| c.kind),
            Some(drop),
            "guaranteed drop"
        );
        assert_eq!(g.score, u.flight.points);
    }

    #[test]
    fn clearing_a_wave_starts_the_next_after_a_short_break() {
        let mut g = quiet(ArmadaMode::Classic);
        for e in g.enemies.iter_mut() {
            e.alive = false;
        }
        g.step(&idle());
        assert!(g.intermission > 0.0);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, ArmadaEvent::WaveCleared { wave: 1 }))
        );
        run(&mut g, INTERMISSION + 0.1, idle());
        assert_eq!(g.wave, 2);
        assert!(g.alive_enemies() > 0);
    }

    #[test]
    fn breaching_the_defense_line_ends_classic_but_zen_resets_with_an_emp() {
        let mut g = quiet(ArmadaMode::Classic);
        g.formation.y = DEFENSE_Y;
        g.step(&idle());
        assert_eq!(g.status, Status::Dead(Cause::DefenseBreach));
        let mut z = quiet(ArmadaMode::Zen);
        z.formation.y = DEFENSE_Y;
        z.step(&idle());
        assert!(z.is_playing(), "Zen cannot lose");
        assert!(z.drain_events().contains(&ArmadaEvent::Emp));
        assert!(z.formation.y < 200.0, "wave restarted at the top");
        assert_eq!(z.hull, 3);
    }

    #[test]
    fn zero_hull_ends_classic_but_zen_resets() {
        let mut g = quiet(ArmadaMode::Classic);
        for _ in 0..3 {
            g.ship.shield = 0.0;
            g.enemy_bolts.push(enemy_bolt(g.ship.x, SHIP_Y));
            g.step(&idle());
        }
        assert_eq!(g.status, Status::Dead(Cause::HullDestroyed));
        let mut z = quiet(ArmadaMode::Zen);
        for _ in 0..4 {
            z.ship.shield = 0.0;
            z.enemy_bolts.push(enemy_bolt(z.ship.x, SHIP_Y));
            z.step(&idle());
        }
        assert!(z.is_playing());
        assert!(z.enemy_bolts.is_empty());
    }

    // ------------------------------------------------------------ daily determinism

    #[test]
    fn daily_waves_ufos_and_drops_do_not_depend_on_the_player() {
        let seed = 20261116;
        let mut a = ArmadaGame::new(ArmadaMode::DailyVibe, seed);
        let mut b = ArmadaGame::new(ArmadaMode::DailyVibe, seed);
        assert_eq!(a.enemies, b.enemies);
        let drops = |g: &ArmadaGame| -> Vec<_> {
            g.enemies.iter().map(|e| (e.col, e.row, e.drop)).collect()
        };
        assert_eq!(drops(&a), drops(&b));
        // Two very different input sequences...
        a.ship.shield = 1e9;
        b.ship.shield = 1e9;
        for i in 0..(30.0 / STEP) as usize {
            a.step(&ArmadaInput {
                move_x: if i % 400 < 200 { 1.0 } else { -1.0 },
                fire_held: true,
                ..idle()
            });
            b.step(&idle());
        }
        // ...still produce the same UFO schedule and next-wave layout.
        assert_eq!(a.ufo_plan.next_at, b.ufo_plan.next_at);
        assert_eq!(
            wave_spec(seed, 2, ArmadaMode::DailyVibe),
            wave_spec(seed, 2, ArmadaMode::DailyVibe)
        );
        let other = ArmadaGame::new(ArmadaMode::DailyVibe, seed + 1);
        assert_ne!(drops(&a), drops(&other));
        // The UFO sequence (points, side, drop) is identical for both.
        let mut pa = UfoPlan::new(seed);
        let mut pb = UfoPlan::new(seed);
        for _ in 0..10 {
            assert_eq!(pa.take(), pb.take());
        }
    }

    // ------------------------------------------------------------ boss

    #[test]
    fn boss_wave_has_core_turrets_and_no_formation() {
        let g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 5);
        assert!(is_boss_wave(g.wave));
        let b = g.boss.as_ref().expect("dreadnought");
        assert!(b.core_hp >= 25);
        assert!(b.turrets.iter().all(|t| t.alive));
        assert_eq!(g.enemies.len(), 0);
        let later = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 10);
        assert!(later.boss.unwrap().core_hp > b.core_hp, "tougher each time");
    }

    #[test]
    fn boss_laser_telegraphs_for_0_9_seconds_before_the_beam_can_hurt() {
        let mut g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 5);
        g.ship.x = 450.0;
        g.ship.shield = 0.0;
        // Turrets silenced, laser about to start.
        for t in g.boss.as_mut().unwrap().turrets.iter_mut() {
            t.cooldown = 1e9;
        }
        g.boss.as_mut().unwrap().laser = Laser::Idle(0.0);
        g.step(&idle());
        let Some(Laser::Telegraph { t, x }) = g.boss.as_ref().map(|b| b.laser) else {
            panic!("telegraph expected");
        };
        assert!((t - TELEGRAPH_SECS).abs() < 0.02);
        assert!((x - 450.0).abs() < 1.0);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, ArmadaEvent::BossTelegraph { .. }))
        );
        // During the warning there is no beam and no damage, even right on the line.
        let mut elapsed = 0.0;
        while elapsed < TELEGRAPH_SECS - 0.05 {
            g.step(&idle());
            elapsed += STEP;
            assert!(g.boss.as_ref().unwrap().beam().is_none());
            assert_eq!(g.hull, 3);
        }
        // Shortly after 0.9 s the beam fires and hurts.
        run(&mut g, 0.15, idle());
        assert_eq!(g.hull, 2, "beam hit the ship that stayed on the line");
        assert!(g.drain_events().contains(&ArmadaEvent::BossFire));
    }

    #[test]
    fn dashing_through_the_beam_avoids_damage() {
        let mut g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 5);
        g.ship.x = 450.0;
        for t in g.boss.as_mut().unwrap().turrets.iter_mut() {
            t.cooldown = 1e9;
        }
        g.boss.as_mut().unwrap().laser = Laser::Fire {
            t: BEAM_SECS,
            x: 450.0,
            dir: 1.0,
        };
        g.update(
            0.0,
            &ArmadaInput {
                dash_pressed: true,
                move_x: 1.0,
                ..idle()
            },
        );
        g.step(&ArmadaInput {
            move_x: 1.0,
            ..idle()
        });
        assert_eq!(g.hull, 3, "i-frames beat the beam");
    }

    #[test]
    fn turrets_can_be_destroyed_and_the_core_ends_the_wave() {
        let mut g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 5);
        let tb = g.boss.as_ref().unwrap().turret_box(0);
        for _ in 0..6 {
            g.player_bolts.push(Bolt {
                x: tb.center().0,
                y: tb.center().1,
                vx: 0.0,
                vy: 0.0,
                w: 4.0,
                h: 14.0,
                near: false,
                grazed: false,
            });
            g.step(&idle());
        }
        assert!(!g.boss.as_ref().unwrap().turrets[0].alive);
        assert!(g.boss.as_ref().unwrap().turrets[1].alive);
        assert!(
            g.drain_events()
                .iter()
                .any(|e| matches!(e, ArmadaEvent::BossTurretDown { .. }))
        );
        // Rail beams chew through the core.
        let core = g.boss.as_ref().unwrap().core_hp;
        let cx = g.boss.as_ref().unwrap().core().center().0;
        g.ship.x = cx;
        g.weapon = Some((PowerKind::RailBeam, 8.0));
        g.ship.fire_cd = 0.0;
        g.step(&ArmadaInput {
            fire_held: true,
            ..idle()
        });
        assert!(g.boss.as_ref().unwrap().core_hp < core);
        // Finish it.
        g.boss.as_mut().unwrap().core_hp = 1;
        let core_box = g.boss.as_ref().unwrap().core();
        g.player_bolts.push(Bolt {
            x: core_box.center().0,
            y: core_box.center().1,
            vx: 0.0,
            vy: 0.0,
            w: 4.0,
            h: 14.0,
            near: false,
            grazed: false,
        });
        g.step(&idle());
        assert!(g.boss.is_none());
        assert!(g.intermission > 0.0, "wave cleared");
        assert!(g.score >= 3000);
    }

    #[test]
    fn armour_plating_absorbs_shots() {
        let mut g = ArmadaGame::new_at_wave(ArmadaMode::Classic, 3, 5);
        let b = g.boss.as_ref().unwrap().body();
        let core = g.boss.as_ref().unwrap().core_hp;
        // Aim at the left edge of the body, away from core and turrets.
        g.player_bolts.push(Bolt {
            x: b.x + 4.0,
            y: b.y + 10.0,
            vx: 0.0,
            vy: 0.0,
            w: 4.0,
            h: 4.0,
            near: false,
            grazed: false,
        });
        g.step(&idle());
        assert_eq!(g.boss.as_ref().unwrap().core_hp, core);
        assert!(g.player_bolts.is_empty(), "absorbed");
    }

    #[test]
    fn frame_time_is_capped_and_the_step_is_fixed() {
        let mut g = quiet(ArmadaMode::Classic);
        g.update(10.0, &idle());
        assert!(g.elapsed <= MAX_FRAME_DT as f64 + 1e-3, "{}", g.elapsed);
        let before = g.elapsed;
        g.update(f32::NAN, &idle());
        assert_eq!(g.elapsed, before);
    }

    #[test]
    fn long_random_play_keeps_the_invariants() {
        for mode in ArmadaMode::ALL {
            let mut g = ArmadaGame::new(mode, 4242);
            let mut rng = SplitMix64::new(1);
            for i in 0..(40.0 / STEP) as usize {
                let input = ArmadaInput {
                    move_x: (rng.next_f32() * 2.0 - 1.0).round(),
                    fire_held: true,
                    fire_pressed: i % 50 == 0,
                    dash_pressed: i % 300 == 0,
                };
                g.step(&input);
                if !g.is_playing() {
                    g = ArmadaGame::new(mode, rng.next_u64());
                    continue;
                }
                assert!((SHIP_MIN_X..=SHIP_MAX_X).contains(&g.ship.x));
                assert!((0.0..=100.0).contains(&g.flow.flow));
                assert!(g.hull <= HULL_MAX && g.hull >= 1);
                assert!(g.enemy_bolts.len() <= 8);
                for b in &g.bunkers {
                    assert!(b.alive() <= Bunker::full_count());
                }
            }
        }
    }
}
