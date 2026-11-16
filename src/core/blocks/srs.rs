//! Super Rotation System: official wall-kick tables and rotation resolution.
//!
//! The tables below are copied verbatim from the SRS guideline, where `+y`
//! points **up**. Our matrix uses `+y` pointing **down**, therefore every
//! offset is applied as `x + dx`, `y - dy` (see [`apply_kick`]). Getting this
//! wrong turns floor kicks into "push into the floor" kicks.

use super::board::{Board, is_valid_position};
use super::piece::{Piece, PieceKind, RotateDir, Rotation};

/// A kick offset as written in the official (y-up) SRS tables.
pub type Kick = (i32, i32);

/// J, L, S, T, Z kicks, indexed by `[from][dir]` with dir 0 = CW, 1 = CCW.
const JLSTZ_KICKS: [[[Kick; 5]; 2]; 4] = [
    // from Spawn (0)
    [
        [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 0 -> R
        [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 0 -> L
    ],
    // from Right (1)
    [
        [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)], // R -> 2
        [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)], // R -> 0
    ],
    // from Reverse (2)
    [
        [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 2 -> L
        [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 2 -> R
    ],
    // from Left (3)
    [
        [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)], // L -> 0
        [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)], // L -> 2
    ],
];

/// I kicks, indexed by `[from][dir]` with dir 0 = CW, 1 = CCW.
const I_KICKS: [[[Kick; 5]; 2]; 4] = [
    // from Spawn (0)
    [
        [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // 0 -> R
        [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)], // 0 -> L
    ],
    // from Right (1)
    [
        [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)], // R -> 2
        [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // R -> 0
    ],
    // from Reverse (2)
    [
        [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // 2 -> L
        [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)], // 2 -> R
    ],
    // from Left (3)
    [
        [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)], // L -> 0
        [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // L -> 2
    ],
];

/// Kicks for 180° rotation (not part of the original SRS; modern-guideline
/// style, also written y-up).
const HALF_KICKS: [Kick; 5] = [(0, 0), (0, 1), (1, 0), (-1, 0), (0, -1)];

/// The five official kick tests for a state transition (y-up notation).
pub fn kick_table(kind: PieceKind, from: Rotation, dir: RotateDir) -> [Kick; 5] {
    let d = match dir {
        RotateDir::Cw => 0,
        RotateDir::Ccw => 1,
        RotateDir::Half => return HALF_KICKS,
    };
    match kind {
        PieceKind::O => [(0, 0); 5],
        PieceKind::I => I_KICKS[from.index()][d],
        _ => JLSTZ_KICKS[from.index()][d],
    }
}

/// Applies an official (y-up) kick to a piece in our y-down matrix.
pub fn apply_kick(piece: &Piece, kick: Kick) -> Piece {
    let (dx, dy_official) = kick;
    Piece {
        x: piece.x + dx,
        y: piece.y - dy_official,
        ..*piece
    }
}

/// Result of a successful rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotationResult {
    pub piece: Piece,
    /// Index of the kick test that succeeded (`0` = no kick).
    pub kick_index: usize,
}

/// Tries all five kick tests in order; returns the first valid placement.
pub fn try_rotate(board: &Board, piece: &Piece, dir: RotateDir) -> Option<RotationResult> {
    if piece.kind == PieceKind::O {
        // The O piece is rotationally symmetric: a no-op rotation. We still
        // report success only if the state actually changes visually (never).
        return None;
    }
    let target = piece.rot.apply(dir);
    let rotated = piece.with_rotation(target);
    kick_table(piece.kind, piece.rot, dir)
        .iter()
        .enumerate()
        .map(|(i, &k)| (i, apply_kick(&rotated, k)))
        .find(|(_, p)| is_valid_position(board, p))
        .map(|(kick_index, piece)| RotationResult { piece, kick_index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::board::{Cell, WIDTH};

    #[test]
    fn floor_kick_lifts_piece_up() {
        // T lying flat on the floor: bottom row of its box state 0 is row 39.
        let b = Board::new();
        let t = Piece::new(PieceKind::T, Rotation::Spawn, 4, 38);
        assert!(is_valid_position(&b, &t));
        let r = try_rotate(&b, &t, RotateDir::Cw).expect("floor kick must succeed");
        // Test 3 (-1, +1 official) => x - 1, y - 1 (lifted, not pushed down).
        assert_eq!(r.kick_index, 2);
        assert_eq!((r.piece.x, r.piece.y), (3, 37));
        assert!(r.piece.minos().iter().all(|m| m.1 <= 39));
    }

    #[test]
    fn wall_kick_from_left_wall() {
        let b = Board::new();
        // T in state R hugging the left wall (box column 0 is outside).
        let t = Piece::new(PieceKind::T, Rotation::Right, -1, 20);
        assert!(is_valid_position(&b, &t));
        let r = try_rotate(&b, &t, RotateDir::Cw).expect("wall kick must succeed");
        assert_eq!(r.kick_index, 1);
        assert_eq!(r.piece.x, 0);
        assert_eq!(r.piece.rot, Rotation::Reverse);
    }

    #[test]
    fn i_piece_wall_kick_right_wall() {
        let b = Board::new();
        // Vertical I (state R, column 2 of box) flush against right wall.
        let i = Piece::new(PieceKind::I, Rotation::Right, 7, 20);
        assert_eq!(i.minos()[0].0, 9);
        let r = try_rotate(&b, &i, RotateDir::Ccw).expect("I kick");
        assert!(r.piece.minos().iter().all(|m| m.0 < WIDTH as i32));
        assert!(r.kick_index > 0);
    }

    #[test]
    fn rotation_fails_when_all_kicks_blocked() {
        let mut b = Board::new();
        // Fill everything except the cells of a flat T at the bottom.
        for y in 30..40 {
            for x in 0..WIDTH as i32 {
                b.set(x, y, Cell::Filled(PieceKind::Z));
            }
        }
        let t = Piece::new(PieceKind::T, Rotation::Spawn, 4, 34);
        for (x, y) in t.minos() {
            b.set(x, y, Cell::Empty);
        }
        assert!(is_valid_position(&b, &t));
        assert!(try_rotate(&b, &t, RotateDir::Cw).is_none());
        assert!(try_rotate(&b, &t, RotateDir::Ccw).is_none());
    }

    #[test]
    fn tables_use_inverted_y() {
        let p = Piece::new(PieceKind::T, Rotation::Spawn, 4, 30);
        let k = apply_kick(&p, (0, 2));
        assert_eq!(
            k.y, 28,
            "official +y (up) must move the piece up the matrix"
        );
    }
}
