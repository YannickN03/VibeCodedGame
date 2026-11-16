//! The 28 x 23 maze: tiles, the hand-made Classic layout, the procedural
//! symmetric Daily generator and the validator both must pass.
//!
//! `(0, 0)` is the top-left tile, `+x` is right and `+y` is down. The tunnel
//! row `y = 11` wraps horizontally: `x = 0` and `x = 27` are neighbours.

use std::collections::VecDeque;

use crate::core::rng::SplitMix64;

pub const W: usize = 28;
pub const H: usize = 23;
pub const TUNNEL_ROW: usize = 11;

/// The gate of the ghost box (two tiles wide) and the tile above it.
pub const GATE: [(i32, i32); 2] = [(13, 9), (14, 9)];
/// Interior of the ghost box (ghost-only area).
pub const BOX_X: (i32, i32) = (11, 16);
pub const BOX_Y: (i32, i32) = (10, 12);
/// Tile in the box where eaten ghosts regenerate.
pub const BOX_HOME: (i32, i32) = (13, 11);
/// Where ghosts appear when they leave the box.
pub const EXIT_TILE: (i32, i32) = (13, 8);
pub const PLAYER_SPAWN: (i32, i32) = (13, 17);
pub const CORNER_ORBS: [(i32, i32); 4] = [(1, 1), (26, 1), (1, 21), (26, 21)];
/// Tile where the Vibe-Core appears ("in the middle", below the box).
pub const CORE_TILE: (i32, i32) = (13, 14);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Wall,
    Empty,
    Dot,
    PowerOrb,
    GhostGate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maze {
    tiles: Vec<Tile>,
}

/// Left half of the Classic maze (columns 0..13); the right half is its
/// mirror image. `#` wall, `.` dot, `o` power orb, ` ` empty, `G` gate,
/// `-` ghost box interior.
const CLASSIC_LEFT: [&str; H] = [
    "##############",
    "#o............",
    "#.###.###.###.",
    "#.###.....###.",
    "#.###.###.###.",
    "#.............",
    "#.###.###.###.",
    "#.....###.###.",
    "#.............",
    "#.###.###.###G",
    "#.###.###.#---",
    " .....###.#---",
    "#.###.###.#---",
    "#.###.###.####",
    "#.............",
    "#.....###.###.",
    "#.###.###.###.",
    "#.............",
    "#.###.###.###.",
    "#.###.....###.",
    "#.###.###.###.",
    "#o............",
    "##############",
];

pub fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H
}

/// Wraps `x` horizontally (the tunnel).
pub fn wrap_x(x: i32) -> i32 {
    x.rem_euclid(W as i32)
}

pub fn in_box(x: i32, y: i32) -> bool {
    x >= BOX_X.0 && x <= BOX_X.1 && y >= BOX_Y.0 && y <= BOX_Y.1
}

pub const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (0, 1), (1, 0)]; // Up, Left, Down, Right

impl Maze {
    pub fn empty() -> Maze {
        Maze {
            tiles: vec![Tile::Wall; W * H],
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Tile {
        if !in_bounds(x, y) {
            return Tile::Wall;
        }
        self.tiles[y as usize * W + x as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, t: Tile) {
        if in_bounds(x, y) {
            self.tiles[y as usize * W + x as usize] = t;
        }
    }

    /// Sets a tile and its mirror image.
    fn set_sym(&mut self, x: i32, y: i32, t: Tile) {
        self.set(x, y, t);
        self.set(W as i32 - 1 - x, y, t);
    }

    pub fn count(&self, t: Tile) -> usize {
        self.tiles.iter().filter(|&&k| k == t).count()
    }

    pub fn dots_total(&self) -> usize {
        self.count(Tile::Dot) + self.count(Tile::PowerOrb)
    }

    /// Can the player stand on this tile? (The gate and the box are closed.)
    pub fn player_walkable(&self, x: i32, y: i32) -> bool {
        let x = wrap_x(x);
        matches!(self.get(x, y), Tile::Dot | Tile::Empty | Tile::PowerOrb) && !in_box(x, y)
    }

    /// Can a ghost stand on this tile? (`through_gate`: the gate is open.)
    pub fn ghost_walkable(&self, x: i32, y: i32, through_gate: bool) -> bool {
        let x = wrap_x(x);
        match self.get(x, y) {
            Tile::Wall => false,
            Tile::GhostGate => through_gate,
            _ => true,
        }
    }

    pub fn from_left_half(rows: &[&str; H]) -> Maze {
        let mut m = Maze::empty();
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let t = match ch {
                    '#' => Tile::Wall,
                    '.' => Tile::Dot,
                    'o' => Tile::PowerOrb,
                    'G' => Tile::GhostGate,
                    _ => Tile::Empty,
                };
                m.set_sym(x as i32, y as i32, t);
            }
        }
        // No dot under the player and no dots in the tunnel mouth.
        m.set(PLAYER_SPAWN.0, PLAYER_SPAWN.1, Tile::Empty);
        m
    }

    /// The hand-made Classic layout.
    pub fn classic() -> Maze {
        Maze::from_left_half(&CLASSIC_LEFT)
    }

    /// Counts walkable neighbours on the player graph.
    fn player_neighbours(&self, x: i32, y: i32) -> usize {
        DIRS.iter()
            .filter(|(dx, dy)| self.player_walkable(x + dx, y + dy))
            .count()
    }

    fn ghost_neighbours(&self, x: i32, y: i32) -> usize {
        DIRS.iter()
            .filter(|(dx, dy)| self.ghost_walkable(x + dx, y + dy, true))
            .count()
    }

    /// BFS distances from `from` on the ghost graph (gate open).
    pub fn ghost_distances(&self, from: (i32, i32)) -> Vec<i32> {
        let mut dist = vec![-1; W * H];
        let mut q = VecDeque::new();
        dist[from.1 as usize * W + from.0 as usize] = 0;
        q.push_back(from);
        while let Some((x, y)) = q.pop_front() {
            let d = dist[y as usize * W + x as usize];
            for (dx, dy) in DIRS {
                let (nx, ny) = (wrap_x(x + dx), y + dy);
                if self.ghost_walkable(nx, ny, true) && dist[ny as usize * W + nx as usize] < 0 {
                    dist[ny as usize * W + nx as usize] = d + 1;
                    q.push_back((nx, ny));
                }
            }
        }
        dist
    }
}

/// Checks every structural rule of a maze. Both the Classic layout and every
/// generated Daily maze must pass.
pub fn validate(m: &Maze) -> Result<(), String> {
    // Left/right mirror symmetry (of the structure).
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let (a, b) = (m.get(x, y), m.get(W as i32 - 1 - x, y));
            let kind = |t: Tile| match t {
                Tile::Wall => 0,
                Tile::GhostGate => 1,
                _ => 2,
            };
            if kind(a) != kind(b) {
                return Err(format!("not symmetric at ({x},{y})"));
            }
        }
    }
    // Outer walls; the tunnel row is open at both ends.
    for x in 0..W as i32 {
        if m.get(x, 0) != Tile::Wall || m.get(x, H as i32 - 1) != Tile::Wall {
            return Err(format!("top/bottom border open at x={x}"));
        }
    }
    for y in 0..H as i32 {
        let open = y == TUNNEL_ROW as i32;
        for x in [0, W as i32 - 1] {
            if (m.get(x, y) != Tile::Wall) != open {
                return Err(format!("edge tile ({x},{y}) wrong"));
            }
        }
    }
    // Ghost box with a gate.
    for (x, y) in GATE {
        if m.get(x, y) != Tile::GhostGate {
            return Err("gate missing".into());
        }
    }
    for y in BOX_Y.0..=BOX_Y.1 {
        for x in BOX_X.0..=BOX_X.1 {
            if m.get(x, y) != Tile::Empty {
                return Err(format!("box interior ({x},{y}) must be empty"));
            }
        }
    }
    // Exactly four power orbs, in the corners.
    for (x, y) in CORNER_ORBS {
        if m.get(x, y) != Tile::PowerOrb {
            return Err(format!("corner orb missing at ({x},{y})"));
        }
    }
    if m.count(Tile::PowerOrb) != 4 {
        return Err("exactly four power orbs expected".into());
    }
    if !m.player_walkable(PLAYER_SPAWN.0, PLAYER_SPAWN.1) {
        return Err("player spawn blocked".into());
    }
    // No dead ends on either graph.
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            if m.player_walkable(x, y) && m.player_neighbours(x, y) < 2 {
                return Err(format!("player dead end at ({x},{y})"));
            }
            if m.ghost_walkable(x, y, true) && m.ghost_neighbours(x, y) < 2 {
                return Err(format!("ghost dead end at ({x},{y})"));
            }
        }
    }
    // Every player tile is reachable.
    let mut seen = vec![false; W * H];
    let mut q = VecDeque::from([PLAYER_SPAWN]);
    seen[PLAYER_SPAWN.1 as usize * W + PLAYER_SPAWN.0 as usize] = true;
    while let Some((x, y)) = q.pop_front() {
        for (dx, dy) in DIRS {
            let (nx, ny) = (wrap_x(x + dx), y + dy);
            if m.player_walkable(nx, ny) && !seen[ny as usize * W + nx as usize] {
                seen[ny as usize * W + nx as usize] = true;
                q.push_back((nx, ny));
            }
        }
    }
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            if m.player_walkable(x, y) && !seen[y as usize * W + x as usize] {
                return Err(format!("tile ({x},{y}) is not reachable"));
            }
        }
    }
    // The ghosts can get from the box home to the exit.
    let d = m.ghost_distances(BOX_HOME);
    if d[EXIT_TILE.1 as usize * W + EXIT_TILE.0 as usize] < 0 {
        return Err("the box is sealed".into());
    }
    Ok(())
}

/// Reserved tiles the Daily generator never changes.
fn reserved(x: i32, y: i32) -> bool {
    // The ring around the ghost box, the tunnel mouth and the fixed spots.
    let ring = (9..=18).contains(&x) && (8..=14).contains(&y);
    let tunnel = y == TUNNEL_ROW as i32 && (x <= 5 || x >= 22);
    ring || tunnel || CORNER_ORBS.contains(&(x, y)) || (x, y) == PLAYER_SPAWN
}

fn generate(seed: u64) -> Maze {
    let mut rng = SplitMix64::new(seed ^ 0x3A2E_5EED_0001_0001);
    let mut m = Maze::empty();
    // Fixed parts: the ghost box and the ring around it.
    for y in 8..=14 {
        for x in 9..=18 {
            let ring = x == 9 || x == 18 || y == 8 || y == 14;
            m.set(x, y, if ring { Tile::Dot } else { Tile::Wall });
        }
    }
    for y in BOX_Y.0..=BOX_Y.1 {
        for x in BOX_X.0..=BOX_X.1 {
            m.set(x, y, Tile::Empty);
        }
    }
    for (x, y) in GATE {
        m.set(x, y, Tile::GhostGate);
    }
    for x in 0..=5 {
        m.set_sym(x, TUNNEL_ROW as i32, Tile::Dot);
    }
    // Depth-first carving over the odd/odd cells of the left half.
    let cells: Vec<(i32, i32)> = (1..=13)
        .step_by(2)
        .flat_map(|x| (1..=21).step_by(2).map(move |y| (x, y)))
        .filter(|&(x, y)| !(9..=18).contains(&x) || !(8..=14).contains(&y))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let start = cells[rng.next_below(cells.len() as u64) as usize];
    let mut stack = vec![start];
    seen.insert(start);
    m.set_sym(start.0, start.1, Tile::Dot);
    while let Some(&(x, y)) = stack.last() {
        let mut next: Vec<(i32, i32)> = DIRS
            .iter()
            .map(|(dx, dy)| (x + 2 * dx, y + 2 * dy))
            .filter(|c| cells.contains(c) && !seen.contains(c))
            .collect();
        if next.is_empty() {
            stack.pop();
            continue;
        }
        let (nx, ny) = next.swap_remove(rng.next_below(next.len() as u64) as usize);
        m.set_sym((x + nx) / 2, (y + ny) / 2, Tile::Dot);
        m.set_sym(nx, ny, Tile::Dot);
        seen.insert((nx, ny));
        stack.push((nx, ny));
    }
    for (x, y) in CORNER_ORBS {
        m.set_sym(x, y, Tile::PowerOrb);
    }
    m.set_sym(PLAYER_SPAWN.0, PLAYER_SPAWN.1, Tile::Dot);
    // Braid: open a wall next to every dead end; prune what cannot be fixed.
    for _ in 0..200 {
        let mut changed = false;
        for y in 1..H as i32 - 1 {
            for x in 0..=13 {
                if !m.player_walkable(x, y) || m.player_neighbours(x, y) >= 2 || reserved(x, y) {
                    continue;
                }
                let mut cands: Vec<(i32, i32)> = DIRS
                    .iter()
                    .filter(|(dx, dy)| {
                        let (wx, wy) = (x + dx, y + dy);
                        let (ox, oy) = (x + 2 * dx, y + 2 * dy);
                        (1..=26).contains(&wx)
                            && (1..=H as i32 - 2).contains(&wy)
                            && m.get(wx, wy) == Tile::Wall
                            && !in_box(wx, wy)
                            && m.player_walkable(ox, oy)
                    })
                    .map(|(dx, dy)| (x + dx, y + dy))
                    .collect();
                if cands.is_empty() {
                    m.set_sym(x, y, Tile::Wall);
                } else {
                    let (cx, cy) = cands.swap_remove(rng.next_below(cands.len() as u64) as usize);
                    m.set_sym(cx, cy, Tile::Dot);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    m.set_sym(PLAYER_SPAWN.0, PLAYER_SPAWN.1, Tile::Empty);
    for (x, y) in CORNER_ORBS {
        m.set_sym(x, y, Tile::PowerOrb);
    }
    m
}

impl Maze {
    /// The symmetric, dead-end-free Daily maze for `seed` (the same for
    /// everyone). Falls back to derived seeds and finally to the Classic
    /// layout if a candidate fails validation.
    pub fn daily(seed: u64) -> Maze {
        for attempt in 0..64u64 {
            let m = generate(seed.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            if validate(&m).is_ok() && m.count(Tile::Dot) >= 150 {
                return m;
            }
        }
        Maze::classic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_classic_maze_is_valid_and_has_28_by_23_tiles() {
        let m = Maze::classic();
        assert_eq!(validate(&m), Ok(()));
        assert_eq!(m.tiles.len(), 28 * 23);
        assert_eq!(m.count(Tile::PowerOrb), 4);
        assert!(m.count(Tile::Dot) > 200, "{} dots", m.count(Tile::Dot));
        assert_eq!(m.get(13, 9), Tile::GhostGate);
        assert_eq!(m.get(0, 11), Tile::Empty, "tunnel mouth");
        assert_eq!(m.get(27, 11), Tile::Empty);
    }

    #[test]
    fn every_walkable_tile_has_two_neighbours_on_both_graphs() {
        let m = Maze::classic();
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                if m.player_walkable(x, y) {
                    assert!(m.player_neighbours(x, y) >= 2, "player ({x},{y})");
                }
                if m.ghost_walkable(x, y, true) {
                    assert!(m.ghost_neighbours(x, y) >= 2, "ghost ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn the_player_cannot_use_the_gate_or_the_box_but_ghosts_can() {
        let m = Maze::classic();
        assert!(!m.player_walkable(13, 9));
        assert!(!m.player_walkable(13, 11));
        assert!(m.ghost_walkable(13, 9, true));
        assert!(
            !m.ghost_walkable(13, 9, false),
            "the gate is closed unless leaving/entering"
        );
        assert!(m.ghost_walkable(13, 11, false));
    }

    #[test]
    fn the_tunnel_connects_both_sides() {
        let m = Maze::classic();
        assert!(m.player_walkable(-1, 11), "x = -1 wraps to 27");
        assert!(m.player_walkable(28, 11), "x = 28 wraps to 0");
        assert_eq!(wrap_x(-1), 27);
        assert_eq!(wrap_x(28), 0);
        // The distance between both mouths over the seam is 1.
        let d = m.ghost_distances((0, 11));
        assert_eq!(d[11 * W + 27], 1);
    }

    #[test]
    fn the_validator_rejects_broken_mazes() {
        let mut dead_end = Maze::classic();
        // Wall off so that (1, 2) becomes a dead end: close (1,3).
        dead_end.set(1, 3, Tile::Wall);
        dead_end.set(26, 3, Tile::Wall);
        assert!(validate(&dead_end).unwrap_err().contains("dead end"));
        let mut asym = Maze::classic();
        asym.set(3, 5, Tile::Wall);
        assert!(validate(&asym).unwrap_err().contains("symmetric"));
        let mut no_orb = Maze::classic();
        no_orb.set_sym(1, 1, Tile::Dot);
        assert!(validate(&no_orb).unwrap_err().contains("orb"));
        let mut open_edge = Maze::classic();
        open_edge.set_sym(0, 5, Tile::Empty);
        assert!(validate(&open_edge).is_err());
        let mut tunnel_shut = Maze::classic();
        tunnel_shut.set_sym(0, 11, Tile::Wall);
        assert!(validate(&tunnel_shut).is_err());
    }

    #[test]
    fn daily_mazes_are_valid_symmetric_deterministic_and_vary() {
        let mut layouts = std::collections::HashSet::new();
        for seed in 20260101u64..20260131 {
            let m = Maze::daily(seed);
            assert_eq!(validate(&m), Ok(()), "seed {seed}");
            assert_eq!(m, Maze::daily(seed), "deterministic");
            let dots = m.count(Tile::Dot);
            assert!((150..=450).contains(&dots), "seed {seed}: {dots} dots");
            layouts.insert(format!("{:?}", m.tiles));
        }
        assert!(
            layouts.len() >= 25,
            "different days give different mazes: {}",
            layouts.len()
        );
        assert_ne!(Maze::daily(1), Maze::daily(2));
    }

    #[test]
    fn daily_mazes_keep_the_fixed_parts() {
        for seed in [1u64, 20261116, 99] {
            let m = Maze::daily(seed);
            for (x, y) in CORNER_ORBS {
                assert_eq!(m.get(x, y), Tile::PowerOrb);
            }
            assert!(m.player_walkable(PLAYER_SPAWN.0, PLAYER_SPAWN.1));
            assert!(m.player_walkable(0, 11) && m.player_walkable(27, 11));
            let d = m.ghost_distances(BOX_HOME);
            assert!(d[EXIT_TILE.1 as usize * W + EXIT_TILE.0 as usize] > 0);
        }
    }
}
