//! A small, safe Snake bot for the headless smoke test, the attract-mode
//! demo and the menu preview logic. It heads for the nearest apple along the
//! shortest free path and prefers moves that keep enough room to live.

use std::collections::{HashMap, HashSet, VecDeque};

use super::game::SnakeGame;
use super::grid::{Direction, GridPos};

/// Cells blocked for the bot: obstacles and the body (the tail tip is free
/// unless the snake is about to grow).
fn blocked_cells(g: &SnakeGame) -> HashSet<GridPos> {
    let keep = if g.pending_growth() > 0 {
        g.segments.len()
    } else {
        g.segments.len().saturating_sub(1)
    };
    g.segments.iter().take(keep).copied().collect()
}

/// Breadth-first search distances from `start` over free cells.
fn distances(g: &SnakeGame, start: GridPos, blocked: &HashSet<GridPos>) -> HashMap<GridPos, u32> {
    let mut dist = HashMap::from([(start, 0u32)]);
    let mut queue = VecDeque::from([start]);
    while let Some(p) = queue.pop_front() {
        let d = dist[&p];
        for dir in Direction::ALL {
            let Some(n) = g.next_cell(p, dir) else {
                continue;
            };
            if blocked.contains(&n) || g.is_obstacle(n) || dist.contains_key(&n) {
                continue;
            }
            dist.insert(n, d + 1);
            queue.push_back(n);
        }
    }
    dist
}

/// The heading the bot wants for the next step.
pub fn next_direction(g: &SnakeGame) -> Direction {
    let blocked = blocked_cells(g);
    let head = g.head();
    let targets: Vec<GridPos> = [g.pulse, g.apple]
        .into_iter()
        .flatten()
        .map(|a| a.pos)
        .collect();
    let len = g.segments.len();

    let mut best: Option<((u8, u32), Direction)> = None;
    for dir in Direction::ALL {
        if dir == g.dir.opposite() && len > 1 {
            continue;
        }
        let Some(n) = g.next_cell(head, dir) else {
            continue;
        };
        if blocked.contains(&n) || g.is_obstacle(n) {
            continue;
        }
        let reach = distances(g, n, &blocked);
        // Prefer moves with enough free room, then the shortest way to food.
        let cramped = (reach.len() < len + 2) as u8;
        let to_food = targets
            .iter()
            .filter_map(|t| reach.get(t))
            .min()
            .copied()
            .unwrap_or(u32::MAX - reach.len() as u32);
        let key = (cramped, to_food);
        // Ties keep the current heading (fewer turns).
        let better = match best {
            None => true,
            Some((k, d)) => key < k || (key == k && dir == g.dir && d != g.dir),
        };
        if better {
            best = Some((key, dir));
        }
    }
    best.map(|(_, d)| d).unwrap_or(g.dir)
}

/// Plans the bot's next turn if it differs from the current heading.
pub fn steer(g: &mut SnakeGame) {
    let want = next_direction(g);
    if g.planned_turns() == 0 && want != g.dir {
        g.queue_turn(want);
    }
}

#[cfg(test)]
mod tests {
    use super::super::game::{SnakeMode, Status};
    use super::*;

    #[test]
    fn bot_survives_and_eats_in_every_mode() {
        for mode in SnakeMode::ALL {
            let mut g = SnakeGame::new(mode, 1234);
            for _ in 0..600 {
                steer(&mut g);
                g.step();
                assert!(
                    g.is_playing(),
                    "{mode:?}: bot died with status {:?}",
                    g.status
                );
            }
            assert!(
                g.apples_eaten >= 10,
                "{mode:?}: only {} apples",
                g.apples_eaten
            );
        }
    }

    #[test]
    fn bot_handles_the_daily_obstacle_layout() {
        for seed in [20261116u64, 20260101, 20260707] {
            let mut g = SnakeGame::new(SnakeMode::DailyVibe, seed);
            for _ in 0..400 {
                steer(&mut g);
                g.step();
                if g.status == Status::Victory {
                    break;
                }
                assert!(g.is_playing(), "seed {seed}: died with {:?}", g.status);
            }
        }
    }
}
