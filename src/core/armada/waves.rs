//! Enemy archetypes, power-ups and the per-wave layout.
//!
//! Everything that must be identical for all players of a `Daily Vibe`
//! (formation, which enemies carry drops, UFO schedule) is derived from the
//! seed alone, with streams that never depend on what the player does.

use crate::core::rng::SplitMix64;

use super::game::ArmadaMode;

pub const COLS: usize = 10;
pub const ROWS: usize = 5;
/// Horizontal / vertical slot distance of the formation.
pub const SLOT_X: f32 = 62.0;
pub const SLOT_Y: f32 = 44.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnemyKind {
    Scout,
    Interceptor,
    Aegis,
    Weaver,
}

impl EnemyKind {
    pub fn hp(self) -> u8 {
        match self {
            EnemyKind::Scout | EnemyKind::Interceptor => 1,
            EnemyKind::Aegis | EnemyKind::Weaver => 2,
        }
    }

    pub fn points(self) -> u64 {
        match self {
            EnemyKind::Scout => 100,
            EnemyKind::Interceptor => 150,
            EnemyKind::Aegis => 250,
            EnemyKind::Weaver => 300,
        }
    }

    /// Hit box `(width, height)`.
    pub fn size(self) -> (f32, f32) {
        match self {
            EnemyKind::Scout => (30.0, 22.0),
            EnemyKind::Interceptor => (34.0, 24.0),
            EnemyKind::Aegis => (38.0, 28.0),
            EnemyKind::Weaver => (34.0, 26.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EnemyKind::Scout => "SCOUT DRONE",
            EnemyKind::Interceptor => "INTERCEPTOR",
            EnemyKind::Aegis => "AEGIS STRIKER",
            EnemyKind::Weaver => "PHASE WEAVER",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerKind {
    TriSpread,
    RailBeam,
    Overclock,
    AegisRepair,
    VibeSurge,
}

impl PowerKind {
    pub const ALL: [PowerKind; 5] = [
        PowerKind::TriSpread,
        PowerKind::RailBeam,
        PowerKind::Overclock,
        PowerKind::AegisRepair,
        PowerKind::VibeSurge,
    ];

    /// Effect duration in seconds (`0` = instant).
    pub fn duration(self) -> f32 {
        match self {
            PowerKind::TriSpread => 10.0,
            PowerKind::RailBeam => 8.0,
            PowerKind::Overclock => 10.0,
            PowerKind::AegisRepair | PowerKind::VibeSurge => 0.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PowerKind::TriSpread => "TRI-SPREAD",
            PowerKind::RailBeam => "RAIL-BEAM",
            PowerKind::Overclock => "OVERCLOCK",
            PowerKind::AegisRepair => "AEGIS REPAIR",
            PowerKind::VibeSurge => "VIBE SURGE",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            PowerKind::TriSpread => "3",
            PowerKind::RailBeam => "R",
            PowerKind::Overclock => "O",
            PowerKind::AegisRepair => "+",
            PowerKind::VibeSurge => "V",
        }
    }
}

/// One enemy of the formation. Its slot `(col, row)` never changes; the
/// position follows from the formation origin.
#[derive(Clone, Debug, PartialEq)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub col: usize,
    pub row: usize,
    pub hp: u8,
    pub alive: bool,
    pub drop: Option<PowerKind>,
}

/// The formation of one wave.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveSpec {
    pub wave: u32,
    pub boss: bool,
    pub enemies: Vec<Enemy>,
}

pub fn is_boss_wave(wave: u32) -> bool {
    wave > 0 && wave.is_multiple_of(5)
}

const LAYOUT_SALT: u64 = 0xA12D_A1A1_5EED_0001;

fn row_kinds(mode: ArmadaMode) -> [EnemyKind; ROWS] {
    use EnemyKind::*;
    match mode {
        // Bullet Swarm: more interceptors and weavers.
        ArmadaMode::BulletSwarm => [Weaver, Interceptor, Weaver, Interceptor, Scout],
        _ => [Weaver, Aegis, Interceptor, Scout, Scout],
    }
}

/// Pure function of `(seed, wave, mode)`: the same layout and drop table for
/// everybody, whatever the player does.
pub fn wave_spec(seed: u64, wave: u32, mode: ArmadaMode) -> WaveSpec {
    if is_boss_wave(wave) {
        return WaveSpec {
            wave,
            boss: true,
            enemies: Vec::new(),
        };
    }
    let mut rng = SplitMix64::new(seed ^ LAYOUT_SALT ^ (wave as u64).wrapping_mul(0x9E37_79B9));
    let kinds = row_kinds(mode);
    let rows = if wave == 1 { 4 } else { ROWS };
    let holes = ((wave.saturating_sub(1)) as usize).min(8);
    let mut skip = std::collections::HashSet::new();
    for _ in 0..holes {
        skip.insert((
            rng.next_below(ROWS as u64) as usize,
            rng.next_below(COLS as u64) as usize,
        ));
    }
    let drop_mult = if mode == ArmadaMode::BulletSwarm {
        1.8
    } else {
        1.0
    };
    let mut enemies = Vec::new();
    for row in 0..rows {
        for col in 0..COLS {
            // Always draw, so the stream stays aligned whatever is skipped.
            let drop_roll = rng.next_f32();
            let kind_roll = rng.next_below(100);
            if skip.contains(&(row, col)) {
                continue;
            }
            let kind = kinds[row + (ROWS - rows)];
            let chance = if kind == EnemyKind::Weaver {
                0.30
            } else {
                0.05
            } * drop_mult;
            let drop = (drop_roll < chance).then(|| {
                // Weighted: repair and surge are rarer.
                let weights = [3u64, 3, 3, 2, 2];
                let mut k = kind_roll % weights.iter().sum::<u64>();
                let mut chosen = PowerKind::ALL[0];
                for (p, w) in PowerKind::ALL.iter().zip(weights) {
                    if k < w {
                        chosen = *p;
                        break;
                    }
                    k -= w;
                }
                chosen
            });
            enemies.push(Enemy {
                kind,
                col,
                row,
                hp: kind.hp(),
                alive: true,
                drop,
            });
        }
    }
    WaveSpec {
        wave,
        boss: false,
        enemies,
    }
}

/// The UFO plan: an independent stream, advanced only when a UFO spawns.
#[derive(Clone, Debug)]
pub struct UfoPlan {
    rng: SplitMix64,
    /// Elapsed game time of the next spawn.
    pub next_at: f64,
}

/// What one UFO flight looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UfoFlight {
    pub points: u64,
    pub from_left: bool,
    pub drop: PowerKind,
}

impl UfoPlan {
    pub fn new(seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed ^ 0x000F_05EE_D0F0_0002);
        let first = 18.0 + 7.0 * rng.next_f32() as f64;
        UfoPlan {
            rng,
            next_at: first,
        }
    }

    /// Draws the flight due at `next_at` and schedules the following one.
    pub fn take(&mut self) -> UfoFlight {
        let points = 500 + 100 * self.rng.next_below(11);
        let from_left = self.rng.next_below(2) == 0;
        let drop = PowerKind::ALL[self.rng.next_below(5) as usize];
        self.next_at += 18.0 + 7.0 * self.rng.next_f32() as f64;
        UfoFlight {
            points,
            from_left,
            drop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_a_pure_function_of_seed_wave_and_mode() {
        for mode in [ArmadaMode::Classic, ArmadaMode::BulletSwarm] {
            assert_eq!(wave_spec(20261116, 3, mode), wave_spec(20261116, 3, mode));
        }
        assert_ne!(
            wave_spec(20261116, 3, ArmadaMode::Classic),
            wave_spec(20261117, 3, ArmadaMode::Classic)
        );
        assert_ne!(
            wave_spec(1, 2, ArmadaMode::Classic),
            wave_spec(1, 3, ArmadaMode::Classic)
        );
    }

    #[test]
    fn wave_one_is_a_full_grid_with_five_archetype_rules() {
        let w = wave_spec(5, 1, ArmadaMode::Classic);
        assert_eq!(w.enemies.len(), 4 * COLS);
        assert!(w.enemies.iter().all(|e| e.hp == e.kind.hp() && e.alive));
        let later = wave_spec(5, 4, ArmadaMode::Classic);
        let kinds: std::collections::HashSet<_> = later.enemies.iter().map(|e| e.kind).collect();
        assert_eq!(kinds.len(), 4, "all four formation archetypes appear");
        assert!(
            later.enemies.len() < ROWS * COLS,
            "holes appear in later waves"
        );
    }

    #[test]
    fn every_fifth_wave_is_a_boss_wave_without_formation() {
        for w in 1..=20 {
            assert_eq!(is_boss_wave(w), w % 5 == 0);
        }
        let b = wave_spec(1, 5, ArmadaMode::Classic);
        assert!(b.boss && b.enemies.is_empty());
    }

    #[test]
    fn weavers_carry_drops_more_often_and_swarm_drops_more() {
        let count = |mode| {
            let (mut weaver, mut other) = (0usize, 0usize);
            let (mut wn, mut on) = (0usize, 0usize);
            for seed in 0..40u64 {
                for e in wave_spec(seed, 2, mode).enemies {
                    if e.kind == EnemyKind::Weaver {
                        wn += 1;
                        weaver += e.drop.is_some() as usize;
                    } else {
                        on += 1;
                        other += e.drop.is_some() as usize;
                    }
                }
            }
            (weaver as f32 / wn as f32, other as f32 / on as f32)
        };
        let (w, o) = count(ArmadaMode::Classic);
        assert!(w > o * 2.0, "weaver bias: {w} vs {o}");
        let (ws, os) = count(ArmadaMode::BulletSwarm);
        assert!(ws > w && os > o, "swarm drops power-ups more often");
    }

    #[test]
    fn ufo_plan_is_deterministic_and_spaced_18_to_25_seconds() {
        let mut a = UfoPlan::new(77);
        let mut b = UfoPlan::new(77);
        let mut prev = a.next_at;
        assert!((18.0..=25.0).contains(&prev), "first flight at {prev}");
        for _ in 0..30 {
            let (fa, fb) = (a.take(), b.take());
            assert_eq!(fa, fb);
            assert!((500..=1500).contains(&fa.points));
            let gap = a.next_at - prev;
            assert!((18.0..=25.0).contains(&gap), "gap {gap}");
            prev = a.next_at;
        }
        let c = UfoPlan::new(78);
        assert_ne!(a.next_at, c.next_at);
    }
}
