//! Directions, the four ghost archetypes and their pure targeting rules.
//!
//! At every tile centre a ghost looks at its walkable neighbours (never the
//! one it just came from) and picks the one closest to its target tile;
//! ties are broken in the order Up > Left > Down > Right.

use super::maze::{DIRS, Maze, wrap_x};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    Left,
    Down,
    Right,
}

impl Dir {
    /// In tie-break priority order.
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Left, Dir::Down, Dir::Right];

    pub fn vec(self) -> (i32, i32) {
        match self {
            Dir::Up => DIRS[0],
            Dir::Left => DIRS[1],
            Dir::Down => DIRS[2],
            Dir::Right => DIRS[3],
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Dir::Left | Dir::Right)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GhostKind {
    /// Red chaser: targets the player's tile.
    Apex,
    /// Pink ambusher: targets 4 tiles ahead of the player.
    Vektor,
    /// Cyan pincer: doubles the vector from Apex over 2 tiles ahead.
    Flank,
    /// Orange patroller: chases from afar, retreats when close.
    Drift,
}

impl GhostKind {
    pub const ALL: [GhostKind; 4] = [
        GhostKind::Apex,
        GhostKind::Vektor,
        GhostKind::Flank,
        GhostKind::Drift,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GhostKind::Apex => "APEX",
            GhostKind::Vektor => "VEKTOR",
            GhostKind::Flank => "FLANK",
            GhostKind::Drift => "DRIFT",
        }
    }

    /// Scatter corner (Drift retreats to the lower left).
    pub fn corner(self) -> (i32, i32) {
        match self {
            GhostKind::Apex => (26, 0),
            GhostKind::Vektor => (1, 0),
            GhostKind::Flank => (26, 22),
            GhostKind::Drift => (1, 22),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostMode {
    Scatter,
    Chase,
    Frightened,
    Eaten,
}

/// Distance (in tiles) beyond which Drift chases and below which it retreats.
pub const DRIFT_RADIUS: i32 = 8;

fn ahead(p: (i32, i32), d: Dir, n: i32) -> (i32, i32) {
    let (dx, dy) = d.vec();
    (p.0 + dx * n, p.1 + dy * n)
}

/// The Chase target of `kind`.
pub fn chase_target(
    kind: GhostKind,
    ghost: (i32, i32),
    player: (i32, i32),
    player_dir: Dir,
    apex: (i32, i32),
) -> (i32, i32) {
    match kind {
        GhostKind::Apex => player,
        GhostKind::Vektor => ahead(player, player_dir, 4),
        GhostKind::Flank => {
            let pivot = ahead(player, player_dir, 2);
            (
                apex.0 + 2 * (pivot.0 - apex.0),
                apex.1 + 2 * (pivot.1 - apex.1),
            )
        }
        GhostKind::Drift => {
            let (dx, dy) = (ghost.0 - player.0, ghost.1 - player.1);
            if dx * dx + dy * dy > DRIFT_RADIUS * DRIFT_RADIUS {
                player
            } else {
                kind.corner()
            }
        }
    }
}

/// Squared distance between two tiles.
pub fn dist2(a: (i32, i32), b: (i32, i32)) -> i32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dy * dy
}

/// Walkable directions from `tile` that are not a reversal.
pub fn options(m: &Maze, tile: (i32, i32), cur: Dir, through_gate: bool) -> Vec<Dir> {
    Dir::ALL
        .into_iter()
        .filter(|&d| d != cur.opposite())
        .filter(|&d| {
            let (dx, dy) = d.vec();
            m.ghost_walkable(tile.0 + dx, tile.1 + dy, through_gate)
        })
        .collect()
}

/// The next direction towards `target` (minimum squared distance, ties in
/// priority order Up > Left > Down > Right, never a reversal).
pub fn choose_dir(
    m: &Maze,
    tile: (i32, i32),
    cur: Dir,
    target: (i32, i32),
    through_gate: bool,
) -> Option<Dir> {
    let mut best: Option<(i32, Dir)> = None;
    for d in options(m, tile, cur, through_gate) {
        let (dx, dy) = d.vec();
        let next = (wrap_x(tile.0 + dx), tile.1 + dy);
        let dist = dist2(next, target);
        if best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, d));
        }
    }
    best.map(|(_, d)| d)
}

/// Step along a BFS distance field (Eaten ghosts going home, ghosts leaving
/// the box): the neighbour with the smallest distance, ties by priority.
pub fn descend(m: &Maze, dist: &[i32], tile: (i32, i32)) -> Option<Dir> {
    use super::maze::W;
    let here = dist[tile.1 as usize * W + tile.0 as usize];
    let mut best: Option<(i32, Dir)> = None;
    for d in Dir::ALL {
        let (dx, dy) = d.vec();
        let (nx, ny) = (wrap_x(tile.0 + dx), tile.1 + dy);
        if !m.ghost_walkable(nx, ny, true) {
            continue;
        }
        let v = dist[ny as usize * W + nx as usize];
        if v >= 0 && v < here && best.is_none_or(|(b, _)| v < b) {
            best = Some((v, d));
        }
    }
    best.map(|(_, d)| d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::maze::maze::Maze;

    #[test]
    fn apex_targets_the_players_tile() {
        let t = chase_target(GhostKind::Apex, (3, 3), (10, 12), Dir::Left, (3, 3));
        assert_eq!(t, (10, 12));
    }

    #[test]
    fn vektor_targets_four_tiles_ahead_in_the_players_direction() {
        for (d, expect) in [
            (Dir::Up, (10, 8)),
            (Dir::Down, (10, 16)),
            (Dir::Left, (6, 12)),
            (Dir::Right, (14, 12)),
        ] {
            let t = chase_target(GhostKind::Vektor, (0, 0), (10, 12), d, (0, 0));
            assert_eq!(t, expect, "{d:?}");
        }
    }

    #[test]
    fn flank_doubles_the_vector_from_apex_over_two_tiles_ahead() {
        // Player (10,12) heading Right: pivot (12,12). Apex at (4,10).
        // Vector apex -> pivot = (8, 2); doubled and added to apex = (20, 14).
        let t = chase_target(GhostKind::Flank, (0, 0), (10, 12), Dir::Right, (4, 10));
        assert_eq!(t, (20, 14));
        // With Apex right on the pivot the target is the pivot itself.
        let t = chase_target(GhostKind::Flank, (0, 0), (10, 12), Dir::Up, (10, 10));
        assert_eq!(t, (10, 10));
    }

    #[test]
    fn drift_chases_beyond_8_tiles_and_retreats_at_8_or_less() {
        let player = (10, 10);
        // Squared distances 65 (8.06 tiles) / 64 (exactly 8) / 63 (7.94).
        let far = (player.0 + 8, player.1 + 1);
        let exactly = (player.0 + 8, player.1);
        let near = (player.0 + 7, player.1 + 3);
        assert_eq!(dist2(far, player), 65);
        assert_eq!(dist2(exactly, player), 64);
        assert_eq!(dist2(near, player), 58);
        let corner = GhostKind::Drift.corner();
        assert_eq!(corner, (1, 22), "lower left");
        assert_eq!(
            chase_target(GhostKind::Drift, far, player, Dir::Up, (0, 0)),
            player
        );
        assert_eq!(
            chase_target(GhostKind::Drift, exactly, player, Dir::Up, (0, 0)),
            corner
        );
        assert_eq!(
            chase_target(GhostKind::Drift, near, player, Dir::Up, (0, 0)),
            corner
        );
    }

    #[test]
    fn ghosts_never_reverse_at_an_intersection() {
        let m = Maze::classic();
        // Tile (5,5) is a 4-way crossing; arriving heading Right, Left is excluded.
        let opts = options(&m, (5, 5), Dir::Right, false);
        assert!(!opts.contains(&Dir::Left));
        assert!(opts.contains(&Dir::Up) && opts.contains(&Dir::Down) && opts.contains(&Dir::Right));
        // Even if the target lies straight behind, the ghost does not turn around.
        let d = choose_dir(&m, (5, 5), Dir::Right, (1, 5), false).unwrap();
        assert_ne!(d, Dir::Left);
    }

    #[test]
    fn ties_are_broken_up_then_left_then_down_then_right() {
        let m = Maze::classic();
        // At the crossing (5,5) heading Up: options Up, Left, Right (Down is
        // the reversal). Target (5,5) itself: Up -> (5,4) dist 1, Left (4,5)
        // dist 1, Right (6,5) dist 1: all equal, Up wins.
        assert_eq!(
            choose_dir(&m, (5, 5), Dir::Up, (5, 5), false),
            Some(Dir::Up)
        );
        // Heading Right (Left excluded): Up, Down, Right tie -> Up.
        assert_eq!(
            choose_dir(&m, (5, 5), Dir::Right, (5, 5), false),
            Some(Dir::Up)
        );
        // Heading Down (Up excluded): Left, Down, Right tie -> Left beats Down and Right.
        assert_eq!(
            choose_dir(&m, (5, 5), Dir::Down, (5, 5), false),
            Some(Dir::Left)
        );
        // Heading Left (Right excluded): Up, Left, Down tie -> Up.
        assert_eq!(
            choose_dir(&m, (5, 5), Dir::Left, (5, 5), false),
            Some(Dir::Up)
        );
        // A real preference: target far to the right picks Right.
        assert_eq!(
            choose_dir(&m, (5, 5), Dir::Up, (20, 5), false),
            Some(Dir::Right)
        );
    }

    #[test]
    fn the_gate_is_only_taken_when_allowed() {
        let m = Maze::classic();
        // At (13,8) heading Left, target inside the box: Down needs the gate.
        let closed = choose_dir(&m, (13, 8), Dir::Left, (13, 11), false).unwrap();
        assert_ne!(closed, Dir::Down);
        let open = choose_dir(&m, (13, 8), Dir::Left, (13, 11), true).unwrap();
        assert_eq!(open, Dir::Down);
    }

    #[test]
    fn eaten_ghosts_descend_the_distance_field_to_the_box() {
        let m = Maze::classic();
        let dist = m.ghost_distances((13, 11));
        // From a far corner the path always gets shorter until home.
        let mut tile = (1, 1);
        let mut guard = 0;
        while tile != (13, 11) && guard < 200 {
            let d = descend(&m, &dist, tile).expect("a way home");
            let (dx, dy) = d.vec();
            tile = (wrap_x(tile.0 + dx), tile.1 + dy);
            guard += 1;
        }
        assert_eq!(tile, (13, 11));
        assert!(guard < 80, "{guard} tiles");
    }
}
