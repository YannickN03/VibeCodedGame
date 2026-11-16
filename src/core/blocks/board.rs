//! The 10×40 matrix (20 hidden buffer rows + 20 visible rows) and the central
//! collision predicate [`is_valid_position`].

use std::ops::Index;

use super::piece::{Piece, PieceKind};

/// Number of columns.
pub const WIDTH: usize = 10;
/// Total number of rows including the hidden buffer zone.
pub const HEIGHT: usize = 40;
/// First visible row (rows `0..20` are the invisible skyline buffer).
pub const VISIBLE_TOP: usize = 20;
/// Number of visible rows.
pub const VISIBLE_ROWS: usize = HEIGHT - VISIBLE_TOP;

/// Content of a single matrix cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Cell {
    #[default]
    Empty,
    Filled(PieceKind),
}

impl Cell {
    pub fn is_empty(self) -> bool {
        self == Cell::Empty
    }
}

/// One matrix row.
pub type Row = [Cell; WIDTH];

/// The playfield matrix. Index with `board[y][x]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    rows: [Row; HEIGHT],
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Index<usize> for Board {
    type Output = Row;
    fn index(&self, y: usize) -> &Row {
        &self.rows[y]
    }
}

impl Board {
    pub fn new() -> Self {
        Board {
            rows: [[Cell::Empty; WIDTH]; HEIGHT],
        }
    }

    /// Returns `true` when `(x, y)` lies inside the matrix.
    pub fn in_bounds(x: i32, y: i32) -> bool {
        x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32
    }

    /// Cell content, `None` outside the matrix.
    pub fn get(&self, x: i32, y: i32) -> Option<Cell> {
        if Board::in_bounds(x, y) {
            Some(self.rows[y as usize][x as usize])
        } else {
            None
        }
    }

    /// Occupied **or** out of bounds (used for the T-spin corner rule).
    pub fn is_blocked(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_none_or(|c| !c.is_empty())
    }

    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        if Board::in_bounds(x, y) {
            self.rows[y as usize][x as usize] = cell;
        }
    }

    /// Writes the piece's minos into the matrix.
    pub fn place(&mut self, piece: &Piece) {
        for (x, y) in piece.minos() {
            self.set(x, y, Cell::Filled(piece.kind));
        }
    }

    /// Indices of all completely filled rows (top to bottom).
    pub fn full_rows(&self) -> Vec<usize> {
        (0..HEIGHT)
            .filter(|&y| self.rows[y].iter().all(|c| !c.is_empty()))
            .collect()
    }

    /// Removes the given rows and shifts everything above down
    /// ("naive gravity": rows move as a whole, no cascade).
    pub fn clear_rows(&mut self, rows: &[usize]) {
        if rows.is_empty() {
            return;
        }
        let mut kept: Vec<Row> = Vec::with_capacity(HEIGHT);
        for (y, row) in self.rows.iter().enumerate() {
            if !rows.contains(&y) {
                kept.push(*row);
            }
        }
        let missing = HEIGHT - kept.len();
        let mut new_rows = [[Cell::Empty; WIDTH]; HEIGHT];
        for (i, row) in kept.into_iter().enumerate() {
            new_rows[i + missing] = row;
        }
        self.rows = new_rows;
    }

    /// Detects, removes and returns the full rows.
    pub fn clear_full_rows(&mut self) -> Vec<usize> {
        let rows = self.full_rows();
        self.clear_rows(&rows);
        rows
    }

    /// Empties the whole matrix.
    pub fn clear_all(&mut self) {
        self.rows = [[Cell::Empty; WIDTH]; HEIGHT];
    }

    /// Topmost row containing at least one block.
    pub fn highest_occupied_row(&self) -> Option<usize> {
        (0..HEIGHT).find(|&y| self.rows[y].iter().any(|c| !c.is_empty()))
    }

    /// All occupied cells as `(x, y, kind)`.
    pub fn filled_cells(&self) -> Vec<(i32, i32, PieceKind)> {
        let mut out = Vec::new();
        for (y, row) in self.rows.iter().enumerate() {
            for (x, c) in row.iter().enumerate() {
                if let Cell::Filled(k) = c {
                    out.push((x as i32, y as i32, *k));
                }
            }
        }
        out
    }

    /// Number of occupied cells.
    pub fn occupied_count(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.iter().filter(|c| !c.is_empty()).count())
            .sum()
    }
}

/// Central, pure collision check.
///
/// A position is invalid as soon as any mino leaves the matrix (left wall
/// `x < 0`, right wall `x >= 10`, floor `y >= 40`, buffer ceiling `y < 0`) or
/// overlaps an occupied cell.
pub fn is_valid_position(board: &Board, piece: &Piece) -> bool {
    piece.minos().iter().all(|&(x, y)| {
        x >= 0
            && x < WIDTH as i32
            && y >= 0
            && y < HEIGHT as i32
            && board[y as usize][x as usize] == Cell::Empty
    })
}

/// Drops a copy of `piece` straight down until the next step would collide.
pub fn drop_position(board: &Board, piece: &Piece) -> Piece {
    let mut p = *piece;
    while is_valid_position(board, &p.shifted(0, 1)) {
        p = p.shifted(0, 1);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::piece::Rotation;

    #[test]
    fn walls_floor_and_ceiling_collide() {
        let b = Board::new();
        let p = Piece::new(PieceKind::O, Rotation::Spawn, 3, 30);
        assert!(is_valid_position(&b, &p));
        // O minos at box x 1..=2 -> x = -2 puts a mino at -1.
        assert!(!is_valid_position(&b, &p.shifted(-5, 0)));
        assert!(!is_valid_position(&b, &p.shifted(5, 0)));
        assert!(!is_valid_position(&b, &p.shifted(0, 9)));
        assert!(!is_valid_position(
            &b,
            &Piece::new(PieceKind::O, Rotation::Spawn, 3, -1)
        ));
    }

    #[test]
    fn occupied_cells_collide() {
        let mut b = Board::new();
        b.set(4, 30, Cell::Filled(PieceKind::Z));
        let p = Piece::new(PieceKind::O, Rotation::Spawn, 3, 29);
        assert!(!is_valid_position(&b, &p));
    }

    #[test]
    fn drop_position_lands_on_floor() {
        let b = Board::new();
        let p = drop_position(&b, &Piece::spawn(PieceKind::T));
        assert_eq!(p.minos().iter().map(|m| m.1).max(), Some(39));
    }

    #[test]
    fn clearing_rows_shifts_down_naively() {
        let mut b = Board::new();
        for x in 0..WIDTH as i32 {
            b.set(x, 39, Cell::Filled(PieceKind::I));
        }
        b.set(0, 38, Cell::Filled(PieceKind::T));
        b.set(5, 36, Cell::Filled(PieceKind::T)); // floating block
        let cleared = b.clear_full_rows();
        assert_eq!(cleared, vec![39]);
        assert_eq!(b[39][0], Cell::Filled(PieceKind::T));
        // floating block moved exactly one row, no cascade
        assert_eq!(b[37][5], Cell::Filled(PieceKind::T));
        assert!(b[38][5].is_empty());
    }
}
