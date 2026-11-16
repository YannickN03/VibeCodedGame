//! Grid coordinates and headings. `(0, 0)` is the top-left cell, `+y` goes down.

/// A cell of the playing field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

impl GridPos {
    pub const fn new(x: i32, y: i32) -> Self {
        GridPos { x, y }
    }

    /// The neighbouring cell in `dir` (not wrapped, may be out of bounds).
    pub fn step(self, dir: Direction) -> GridPos {
        let (dx, dy) = dir.delta();
        GridPos::new(self.x + dx, self.y + dy)
    }

    /// Manhattan distance.
    pub fn manhattan(self, other: GridPos) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];

    pub fn delta(self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }

    pub fn opposite(self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposites_cancel_and_deltas_are_unit_steps() {
        for d in Direction::ALL {
            assert_eq!(d.opposite().opposite(), d);
            let (dx, dy) = d.delta();
            let (ox, oy) = d.opposite().delta();
            assert_eq!((dx + ox, dy + oy), (0, 0));
            assert_eq!(dx.abs() + dy.abs(), 1);
        }
        assert_eq!(GridPos::new(3, 3).step(Direction::Up), GridPos::new(3, 2));
        assert_eq!(GridPos::new(0, 0).manhattan(GridPos::new(2, 3)), 5);
    }
}
