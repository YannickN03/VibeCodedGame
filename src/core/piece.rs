//! Tetromino definitions, rotation states and the active-piece value type.
//!
//! Coordinates follow the matrix convention of the whole crate: `(0, 0)` is the
//! top-left cell, `+x` points right and `+y` points **down**.

/// The seven one-sided tetrominoes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PieceKind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl PieceKind {
    /// All kinds in canonical order (used by the 7-bag).
    pub const ALL: [PieceKind; 7] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];

    /// Stable index `0..7`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Edge length of the SRS bounding box the piece rotates in.
    pub fn box_size(self) -> i32 {
        match self {
            PieceKind::I | PieceKind::O => 4,
            _ => 3,
        }
    }

    /// Mino offsets inside the bounding box for rotation state `Spawn`.
    fn spawn_cells(self) -> [(i32, i32); 4] {
        match self {
            PieceKind::I => [(0, 1), (1, 1), (2, 1), (3, 1)],
            PieceKind::O => [(1, 0), (2, 0), (1, 1), (2, 1)],
            PieceKind::T => [(1, 0), (0, 1), (1, 1), (2, 1)],
            PieceKind::S => [(1, 0), (2, 0), (0, 1), (1, 1)],
            PieceKind::Z => [(0, 0), (1, 0), (1, 1), (2, 1)],
            PieceKind::J => [(0, 0), (0, 1), (1, 1), (2, 1)],
            PieceKind::L => [(2, 0), (0, 1), (1, 1), (2, 1)],
        }
    }

    /// Mino offsets inside the bounding box for the given rotation state.
    ///
    /// Rotation is a true rotation around the bounding-box centre, which is
    /// exactly what the SRS kick tables assume. The `O` piece never changes.
    pub fn cells(self, rot: Rotation) -> [(i32, i32); 4] {
        let mut cells = self.spawn_cells();
        if self == PieceKind::O {
            return cells;
        }
        let n = self.box_size();
        for _ in 0..rot.index() {
            for c in cells.iter_mut() {
                // Clockwise rotation in a y-down system: (x, y) -> (n-1-y, x)
                *c = (n - 1 - c.1, c.0);
            }
        }
        cells
    }

    /// Short single-letter name.
    pub fn letter(self) -> char {
        match self {
            PieceKind::I => 'I',
            PieceKind::O => 'O',
            PieceKind::T => 'T',
            PieceKind::S => 'S',
            PieceKind::Z => 'Z',
            PieceKind::J => 'J',
            PieceKind::L => 'L',
        }
    }
}

/// SRS rotation states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rotation {
    Spawn = 0,
    Right = 1,
    Reverse = 2,
    Left = 3,
}

impl Rotation {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Rotation {
        match i % 4 {
            0 => Rotation::Spawn,
            1 => Rotation::Right,
            2 => Rotation::Reverse,
            _ => Rotation::Left,
        }
    }

    pub fn cw(self) -> Rotation {
        Rotation::from_index(self.index() + 1)
    }

    pub fn ccw(self) -> Rotation {
        Rotation::from_index(self.index() + 3)
    }

    pub fn flip(self) -> Rotation {
        Rotation::from_index(self.index() + 2)
    }

    pub fn apply(self, dir: RotateDir) -> Rotation {
        match dir {
            RotateDir::Cw => self.cw(),
            RotateDir::Ccw => self.ccw(),
            RotateDir::Half => self.flip(),
        }
    }
}

/// Requested rotation direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateDir {
    Cw,
    Ccw,
    Half,
}

/// Horizontal spawn column of every bounding box (centres the piece).
pub const SPAWN_X: i32 = 3;
/// Spawn row of every bounding box: minos land on rows 18..=19 (skyline).
pub const SPAWN_Y: i32 = 18;

/// A tetromino placed on the matrix (top-left corner of its bounding box).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub rot: Rotation,
    pub x: i32,
    pub y: i32,
}

impl Piece {
    pub fn new(kind: PieceKind, rot: Rotation, x: i32, y: i32) -> Self {
        Piece { kind, rot, x, y }
    }

    /// Fresh piece in state `Spawn` at the spawn position.
    pub fn spawn(kind: PieceKind) -> Self {
        Piece::new(kind, Rotation::Spawn, SPAWN_X, SPAWN_Y)
    }

    /// Absolute matrix coordinates of the four minos.
    pub fn minos(&self) -> [(i32, i32); 4] {
        let mut out = self.kind.cells(self.rot);
        for c in out.iter_mut() {
            c.0 += self.x;
            c.1 += self.y;
        }
        out
    }

    pub fn shifted(&self, dx: i32, dy: i32) -> Piece {
        Piece {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }

    pub fn with_rotation(&self, rot: Rotation) -> Piece {
        Piece { rot, ..*self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_rotations_return_to_spawn() {
        for kind in PieceKind::ALL {
            let mut r = Rotation::Spawn;
            for _ in 0..4 {
                r = r.cw();
            }
            assert_eq!(kind.cells(r), kind.cells(Rotation::Spawn));
        }
    }

    #[test]
    fn t_piece_right_state_points_right() {
        let mut cells = PieceKind::T.cells(Rotation::Right).to_vec();
        cells.sort();
        assert_eq!(cells, vec![(1, 0), (1, 1), (1, 2), (2, 1)]);
    }

    #[test]
    fn i_piece_right_state_is_column_two() {
        let cells = PieceKind::I.cells(Rotation::Right);
        assert!(cells.iter().all(|c| c.0 == 2));
    }

    #[test]
    fn spawn_is_centered() {
        for kind in PieceKind::ALL {
            let p = Piece::spawn(kind);
            let xs: Vec<i32> = p.minos().iter().map(|m| m.0).collect();
            assert!(*xs.iter().min().unwrap() >= 3);
            assert!(*xs.iter().max().unwrap() <= 6);
            assert!(p.minos().iter().all(|m| m.1 == 18 || m.1 == 19));
        }
    }
}
