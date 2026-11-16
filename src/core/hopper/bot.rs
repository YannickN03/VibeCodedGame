//! A planning bot for the headless smoke test and the attract mode.
//!
//! Because every lane is a pure function of `(seed, row, time)`, the bot can
//! look into the future exactly: it searches a time-expanded graph of
//! `(row, x, time)` states (hop forward, hop sideways or wait one hop) and
//! only walks into states that are safe for the whole hop, including being
//! carried along by a log. No heuristic tuning, no special cases per biome.

use std::collections::HashMap;

use super::game::{Dir, HopperGame, HopperInput, PLATFORM_TOLERANCE};
use super::world::{COLS, HITBOX, LaneKind, TrainPhase, WIDTH, laser_charge_left, train_phase};

/// Extra safety margin on each side of the hit box (tiles).
const MARGIN: f32 = 0.16;
/// Planned duration of one action: the real hop takes 14 fixed steps
/// (0.1167 s) and a press may wait up to one more step.
const PLAN_HOP: f32 = 0.13;
/// Hop duration with Phase-Boots and without (rounded up to whole steps).
const FAST_HOP: f32 = 0.077;
const SLOW_HOP: f32 = 0.1167;
/// Platform tolerance used while planning (stricter than the game's).
const PLAN_TOLERANCE: f32 = PLATFORM_TOLERANCE - 0.22;
const DEPTH: usize = 9;

#[derive(Clone, Copy, Debug)]
struct State {
    row: i64,
    x: f32,
    t: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Up,
    Left,
    Right,
    Wait,
}

fn tile(x: f32) -> usize {
    (x.floor() as i64).clamp(0, COLS as i64 - 1) as usize
}

/// Is standing at `(x, y)` on the rows it overlaps deadly at time `t`?
fn deadly(g: &mut HopperGame, x: f32, y: f32, t: f32) -> bool {
    // The real hop starts up to one fixed step away from the planned time and
    // the planner samples the hop coarsely, so smear every check in time.
    [-0.012, 0.0, 0.012]
        .iter()
        .any(|d| deadly_at(g, x, y, t + d))
}

fn deadly_at(g: &mut HopperGame, x: f32, y: f32, t: f32) -> bool {
    let r0 = (y - HITBOX * 0.5).floor() as i64;
    let r1 = (y + HITBOX * 0.5).floor() as i64;
    for r in r0..=r1 {
        if r < 0 {
            continue;
        }
        let spec = g.world.row(r);
        // Never stand on a rail row when the train is close: besides the
        // pass itself, the last 0.7 s of the warning count as deadly, so the
        // bot always has time to hop off again.
        if let LaneKind::Rail { period, phase, .. } = &spec.lane {
            match train_phase(*period, *phase, t) {
                TrainPhase::Pass { .. } => return true,
                TrainPhase::Warning { left } if left < 0.7 => return true,
                _ => {}
            }
        }
        for dx in [-MARGIN, 0.0, MARGIN] {
            if spec.lethal_at(x + dx, y, t) {
                return true;
            }
            if let LaneKind::Laser {
                period,
                phase,
                active,
                ..
            } = &spec.lane
            {
                let col = tile(x + dx);
                // Active, or about to fire within 0.4 s (time to hop off).
                if active[col]
                    && (spec.laser_active(col, t)
                        || laser_charge_left(*period, *phase, t).is_some_and(|l| l < 0.4))
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Result of performing `a` from `s`, or `None` if it is blocked or deadly.
fn advance(g: &mut HopperGame, s: State, a: Action) -> Option<State> {
    let (dx, dy) = match a {
        Action::Up => (0.0, 1),
        Action::Left => (-1.0, 0),
        Action::Right => (1.0, 0),
        Action::Wait => (0.0, 0),
    };
    let to_row = s.row + dy;
    let mut to_x = s.x + dx;
    let spec_to = g.world.row(to_row).clone();
    let river_to = matches!(spec_to.lane, LaneKind::River { .. });
    let moving = a != Action::Wait;
    if moving {
        let ok = if river_to {
            (0.2..=WIDTH - 0.2).contains(&to_x)
        } else {
            (0.0..WIDTH).contains(&to_x)
        };
        if !ok || spec_to.is_tree(tile(to_x)) {
            return None;
        }
    }
    let t_end = s.t + PLAN_HOP;
    // Safe during the whole hop. The real hop takes 0.077 s with Phase-Boots
    // and 0.117 s without, so both timings are checked at every sample.
    for k in 1..=10 {
        let tau = PLAN_HOP * k as f32 / 10.0;
        for dur in [FAST_HOP, SLOW_HOP] {
            let p = (tau / dur).min(1.0);
            let xp = s.x + (to_x - s.x) * p;
            let yp = s.row as f32 + dy as f32 * p + 0.5;
            if deadly(g, xp, yp, s.t + tau) {
                return None;
            }
        }
    }
    // Landing.
    if river_to {
        let v = spec_to.platform_under(to_x, t_end, PLAN_TOLERANCE)?;
        // Carried while the hop ends / while waiting.
        to_x += if moving { 0.0 } else { v * PLAN_HOP };
        if !(0.35..=WIDTH - 0.35).contains(&to_x) {
            return None;
        }
        // Still on the log after being carried?
        spec_to.platform_under(to_x, t_end, PLAN_TOLERANCE)?;
    } else {
        to_x = to_x.floor() + 0.5;
        match &spec_to.lane {
            LaneKind::Bridge { solid, .. } if !solid[tile(to_x)] => return None,
            _ => {}
        }
        if deadly(g, to_x, to_row as f32 + 0.5, t_end) {
            return None;
        }
    }
    Some(State {
        row: to_row,
        x: to_x,
        t: t_end,
    })
}

/// Plans a few hops ahead and returns the first action of the best plan.
fn plan(g: &mut HopperGame, start: State) -> Option<Action> {
    // key -> (first action, steps)
    let mut frontier: Vec<(State, Action)> = Vec::new();
    let mut seen: HashMap<(i64, i32, usize), ()> = HashMap::new();
    let mut best: Option<(f32, Action)> = None;
    let consider = |s: &State, first: Action, steps: usize, best: &mut Option<(f32, Action)>| {
        // Progress is worth far more than a step of time; sideways hops
        // and waiting are cheap but not free.
        let score = (s.row - start.row) as f32 * 10.0 - steps as f32 - (s.x - 7.5).abs() * 0.02;
        if best.is_none_or(|(b, _)| score > b) {
            *best = Some((score, first));
        }
    };
    for a in [Action::Up, Action::Left, Action::Right, Action::Wait] {
        if let Some(n) = advance(g, start, a) {
            consider(&n, a, 1, &mut best);
            frontier.push((n, a));
        }
    }
    for depth in 2..=DEPTH {
        let mut next: Vec<(State, Action)> = Vec::new();
        for (s, first) in &frontier {
            for a in [Action::Up, Action::Left, Action::Right, Action::Wait] {
                if let Some(n) = advance(g, *s, a) {
                    let key = (n.row, (n.x * 4.0).round() as i32, depth);
                    if seen.insert(key, ()).is_none() {
                        consider(&n, *first, depth, &mut best);
                        next.push((n, *first));
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    best.map(|(_, a)| a)
}

/// The input for the next frame (acts only between hops).
pub fn choose(g: &mut HopperGame) -> HopperInput {
    if !g.is_playing() || !g.can_accept() {
        return HopperInput::default();
    }
    g.ensure_rows();
    let start = State {
        row: g.row,
        x: g.x,
        t: g.time + 0.01,
    };
    let action = plan(g, start).unwrap_or(Action::Up);
    let dir = match action {
        Action::Up => Some(Dir::Up),
        Action::Left => Some(Dir::Left),
        Action::Right => Some(Dir::Right),
        Action::Wait => None,
    };
    HopperInput { dir }
}

#[cfg(test)]
mod tests {
    use super::super::game::{HopperMode, Status};
    use super::*;

    fn fly(mode: HopperMode, seed: u64, seconds: f32) -> HopperGame {
        let mut g = HopperGame::new(mode, seed);
        for _ in 0..(seconds * 120.0) as usize {
            let i = choose(&mut g);
            g.step(&i);
            if !g.is_playing() {
                break;
            }
        }
        g
    }

    #[test]
    fn the_planner_crosses_roads_trains_and_rivers_without_dying() {
        for mode in [HopperMode::Classic, HopperMode::DailyVibe, HopperMode::Zen] {
            for seed in [1u64, 20261116, 777, 4242] {
                let g = fly(mode, seed, 25.0);
                assert!(
                    g.is_playing(),
                    "{mode:?}/{seed}: died: {:?} at row {}",
                    g.status,
                    g.max_row
                );
                assert!(
                    g.max_row >= 15,
                    "{mode:?}/{seed}: only reached row {}",
                    g.max_row
                );
            }
        }
    }

    #[test]
    fn the_planner_keeps_up_with_the_fast_blitz_camera() {
        for seed in [3u64, 99, 31337] {
            let g = fly(HopperMode::BlitzRush, seed, 20.0);
            assert!(
                g.is_playing(),
                "blitz/{seed}: {:?} at row {}",
                g.status,
                g.max_row
            );
            assert!(g.max_row >= 20, "blitz/{seed}: row {}", g.max_row);
        }
    }

    #[test]
    fn the_planner_gets_through_hazard_zones_too() {
        for seed in [5u64, 12, 2026] {
            let mut g = HopperGame::new_at_row(HopperMode::Zen, seed, 34);
            let start = g.max_row;
            for _ in 0..(30.0 * 120.0) as usize {
                let i = choose(&mut g);
                g.step(&i);
            }
            assert!(
                g.max_row > start + 15,
                "seed {seed}: {} -> {}",
                start,
                g.max_row
            );
            assert!(!matches!(g.status, Status::Dead(_)));
        }
    }
}
