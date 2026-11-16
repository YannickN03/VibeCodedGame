//! Daily Vibe obstacles: a 4-fold mirror-symmetric wall layout derived from
//! the `YYYYMMDD` seed. A layout is only accepted when
//!
//! * the middle rows around the start stay clear,
//! * all free cells are connected, and
//! * no free cell is a dead end (every free cell has at least two free
//!   neighbours inside the field).
//!
//! Otherwise the next derived seed is tried, so the result is deterministic
//! for a given day and always playable.

use std::collections::{HashSet, VecDeque};

use crate::core::rng::SplitMix64;

use super::grid::{Direction, GridPos};

const MAX_ATTEMPTS: u64 = 400;

/// Free neighbours of `p` inside the field.
fn free_neighbours(p: GridPos, w: i32, h: i32, walls: &HashSet<GridPos>) -> usize {
    Direction::ALL
        .iter()
        .map(|&d| p.step(d))
        .filter(|n| n.x >= 0 && n.y >= 0 && n.x < w && n.y < h && !walls.contains(n))
        .count()
}

/// `true` if `walls` leave a connected field without dead ends.
pub fn is_playable(walls: &HashSet<GridPos>, w: i32, h: i32) -> bool {
    let free: Vec<GridPos> = (0..h)
        .flat_map(|y| (0..w).map(move |x| GridPos::new(x, y)))
        .filter(|p| !walls.contains(p))
        .collect();
    let Some(&first) = free.first() else {
        return false;
    };
    if free.iter().any(|&p| free_neighbours(p, w, h, walls) < 2) {
        return false;
    }
    // Flood fill.
    let mut seen = HashSet::from([first]);
    let mut queue = VecDeque::from([first]);
    while let Some(p) = queue.pop_front() {
        for d in Direction::ALL {
            let n = p.step(d);
            if n.x >= 0 && n.y >= 0 && n.x < w && n.y < h && !walls.contains(&n) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    seen.len() == free.len()
}

fn candidate(seed: u64, w: i32, h: i32) -> HashSet<GridPos> {
    let mut rng = SplitMix64::new(seed);
    let mut walls = HashSet::new();
    // Quadrant (top-left); rows around the middle stay clear for the start.
    let (qx_max, qy_max) = (w / 2 - 2, h / 2 - 3);
    let segments = 6 + rng.next_below(4) as i32;
    for _ in 0..segments {
        let len = 2 + rng.next_below(3) as i32;
        let horizontal = rng.next_below(2) == 0;
        let x0 = 2 + rng.next_below((qx_max - 1).max(1) as u64) as i32;
        let y0 = 1 + rng.next_below(qy_max.max(1) as u64) as i32;
        for i in 0..len {
            let (x, y) = if horizontal {
                (x0 + i, y0)
            } else {
                (x0, y0 + i)
            };
            if x <= qx_max && y <= qy_max {
                for (mx, my) in [
                    (x, y),
                    (w - 1 - x, y),
                    (x, h - 1 - y),
                    (w - 1 - x, h - 1 - y),
                ] {
                    walls.insert(GridPos::new(mx, my));
                }
            }
        }
    }
    walls
}

/// The obstacles of a day for a `w × h` field, sorted for determinism.
pub fn generate(seed: u64, w: i32, h: i32) -> Vec<GridPos> {
    for attempt in 0..MAX_ATTEMPTS {
        let s = seed.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let walls = candidate(s, w, h);
        if !walls.is_empty() && is_playable(&walls, w, h) {
            let mut v: Vec<GridPos> = walls.into_iter().collect();
            v.sort();
            return v;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = 32;
    const H: i32 = 18;

    #[test]
    fn same_seed_same_layout() {
        for seed in [20261116u64, 20260101, 1, 0] {
            assert_eq!(generate(seed, W, H), generate(seed, W, H));
        }
    }

    #[test]
    fn different_days_give_different_layouts() {
        let a = generate(20261116, W, H);
        let b = generate(20261117, W, H);
        assert_ne!(a, b);
    }

    #[test]
    fn layouts_are_symmetric_clear_at_the_start_and_playable() {
        for seed in 20260101u64..20260160 {
            let walls: HashSet<GridPos> = generate(seed, W, H).into_iter().collect();
            assert!(walls.len() >= 4, "seed {seed}: too few obstacles");
            for p in &walls {
                assert!(walls.contains(&GridPos::new(W - 1 - p.x, p.y)), "mirror x");
                assert!(walls.contains(&GridPos::new(p.x, H - 1 - p.y)), "mirror y");
                assert!(p.x >= 0 && p.y >= 0 && p.x < W && p.y < H);
            }
            // The start row and the snake's body cells are free.
            for x in 0..W {
                assert!(
                    !walls.contains(&GridPos::new(x, H / 2)),
                    "start row blocked"
                );
            }
            assert!(is_playable(&walls, W, H), "seed {seed}: not playable");
        }
    }

    #[test]
    fn dead_ends_and_split_fields_are_rejected() {
        // A wall column splitting the field in two.
        let split: HashSet<GridPos> = (0..H).map(|y| GridPos::new(10, y)).collect();
        assert!(!is_playable(&split, W, H));
        // A one-cell pocket (dead end) at the top border.
        let pocket: HashSet<GridPos> = [GridPos::new(4, 0), GridPos::new(6, 0), GridPos::new(5, 1)]
            .into_iter()
            .collect();
        assert!(!is_playable(&pocket, W, H));
        assert!(is_playable(&HashSet::new(), W, H));
    }
}
