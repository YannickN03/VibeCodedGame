//! A small autopilot for the headless smoke test and the attract mode: BFS on
//! the player graph to the nearest dot (or Vibe-Core, or a frightened
//! ghost), treating the surroundings of dangerous ghosts as blocked.

use std::collections::VecDeque;

use super::game::{House, MazeGame};
use super::ghost::Dir;
use super::maze::{H, W, wrap_x};

const DANGER_RADIUS: i32 = 3;

fn manhattan(a: (i32, i32), b: (i32, i32)) -> i32 {
    let dx = (a.0 - b.0).abs();
    dx.min(W as i32 - dx) + (a.1 - b.1).abs()
}

/// Tiles of ghosts that can hurt the player right now, plus the tiles they
/// are about to enter.
fn danger_tiles(g: &MazeGame) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for gh in &g.ghosts {
        if gh.eaten || gh.house != House::Out {
            continue;
        }
        // Nearly-over Frightened phases are not trusted.
        if g.fright_t > 1.2 {
            continue;
        }
        out.push(gh.tile());
        if let Some(d) = gh.dir {
            let (dx, dy) = d.vec();
            let t = gh.tile();
            out.push((wrap_x(t.0 + dx), t.1 + dy));
            out.push((wrap_x(t.0 + 2 * dx), t.1 + 2 * dy));
        }
    }
    out
}

/// The direction the bot wants to go.
pub fn choose(g: &MazeGame) -> Option<Dir> {
    let start = g.player.tile();
    let danger = danger_tiles(g);
    let blocked =
        |t: (i32, i32)| t != start && danger.iter().any(|&d| manhattan(d, t) < DANGER_RADIUS);
    let is_goal = |t: (i32, i32)| {
        matches!(
            g.maze.get(t.0, t.1),
            super::maze::Tile::Dot | super::maze::Tile::PowerOrb
        ) || g.core.is_some_and(|c| c.tile == t)
            || (g.fright_t > 3.0
                && g.ghosts
                    .iter()
                    .any(|gh| !gh.eaten && gh.house == House::Out && gh.tile() == t))
    };
    // BFS with parents.
    let mut parent: Vec<Option<(i32, i32)>> = vec![None; W * H];
    let mut seen = vec![false; W * H];
    let idx = |t: (i32, i32)| t.1 as usize * W + t.0 as usize;
    let mut q = VecDeque::from([start]);
    seen[idx(start)] = true;
    let mut found = None;
    while let Some(t) = q.pop_front() {
        if t != start && is_goal(t) {
            found = Some(t);
            break;
        }
        for d in Dir::ALL {
            let (dx, dy) = d.vec();
            let n = (wrap_x(t.0 + dx), t.1 + dy);
            if g.maze.player_walkable(n.0, n.1) && !seen[idx(n)] && !blocked(n) {
                seen[idx(n)] = true;
                parent[idx(n)] = Some(t);
                q.push_back(n);
            }
        }
    }
    if let Some(mut t) = found {
        while let Some(p) = parent[idx(t)] {
            if p == start {
                break;
            }
            t = p;
        }
        for d in Dir::ALL {
            let (dx, dy) = d.vec();
            if (wrap_x(start.0 + dx), start.1 + dy) == t {
                return Some(d);
            }
        }
    }
    // Cornered: run to the neighbour that is farthest from every danger.
    let mut best: Option<(i32, Dir)> = None;
    for d in Dir::ALL {
        let (dx, dy) = d.vec();
        let n = (wrap_x(start.0 + dx), start.1 + dy);
        if !g.maze.player_walkable(n.0, n.1) {
            continue;
        }
        let score = danger.iter().map(|&t| manhattan(t, n)).min().unwrap_or(99);
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, d));
        }
    }
    best.map(|(_, d)| d)
}
