//! Destructible energy bunkers: a 7x4 grid of micro cells in a dome shape
//! (the bottom middle is open). Every hit removes exactly one cell.

use super::geom::Aabb;

pub const COLS: usize = 7;
pub const ROWS: usize = 4;
pub const CELL: f32 = 8.0;

/// Dome: rows 0..3 from the top; the bottom row has an opening in the middle.
pub const SHAPE: [[bool; COLS]; ROWS] = [
    [false, true, true, true, true, true, false],
    [true, true, true, true, true, true, true],
    [true, true, true, true, true, true, true],
    [true, true, false, false, false, true, true],
];

#[derive(Clone, Debug, PartialEq)]
pub struct Bunker {
    /// Top-left corner.
    pub x: f32,
    pub y: f32,
    pub cells: [[bool; COLS]; ROWS],
}

impl Bunker {
    pub fn new(x: f32, y: f32) -> Self {
        Bunker { x, y, cells: SHAPE }
    }

    pub fn aabb(&self) -> Aabb {
        Aabb::new(self.x, self.y, COLS as f32 * CELL, ROWS as f32 * CELL)
    }

    pub fn cell_box(&self, row: usize, col: usize) -> Aabb {
        Aabb::new(
            self.x + col as f32 * CELL,
            self.y + row as f32 * CELL,
            CELL,
            CELL,
        )
    }

    pub fn alive(&self) -> usize {
        self.cells.iter().flatten().filter(|c| **c).count()
    }

    pub fn full_count() -> usize {
        SHAPE.iter().flatten().filter(|c| **c).count()
    }

    pub fn is_damaged(&self) -> bool {
        self.cells != SHAPE
    }

    /// A projectile `area` hits the bunker: removes ONE overlapping cell and
    /// returns it. `from_below` picks the lowest overlapping cell (a player
    /// bolt flying up), otherwise the highest (an enemy bolt flying down).
    pub fn hit(&mut self, area: &Aabb, from_below: bool) -> Option<(usize, usize)> {
        if !self.aabb().overlaps(area) {
            return None;
        }
        let order: Vec<usize> = if from_below {
            (0..ROWS).rev().collect()
        } else {
            (0..ROWS).collect()
        };
        let cols = |r: usize| -> Vec<usize> {
            // Prefer the column closest to the projectile centre.
            let cx = area.center().0;
            let mut c: Vec<usize> = (0..COLS).collect();
            c.sort_by(|&a, &b| {
                let da = (self.cell_box(r, a).center().0 - cx).abs();
                let db = (self.cell_box(r, b).center().0 - cx).abs();
                da.total_cmp(&db)
            });
            c
        };
        for r in order {
            for c in cols(r) {
                if self.cells[r][c] && self.cell_box(r, c).overlaps(area) {
                    self.cells[r][c] = false;
                    return Some((r, c));
                }
            }
        }
        None
    }

    /// Removes every cell overlapped by a big body (e.g. an enemy crossing it).
    pub fn crush(&mut self, area: &Aabb) -> usize {
        let mut n = 0;
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.cells[r][c] && self.cell_box(r, c).overlaps(area) {
                    self.cells[r][c] = false;
                    n += 1;
                }
            }
        }
        n
    }

    pub fn repair(&mut self) {
        self.cells = SHAPE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dome_shape_has_an_open_bottom_middle() {
        let b = Bunker::new(100.0, 505.0);
        assert_eq!(Bunker::full_count(), 5 + 7 + 7 + 4);
        assert_eq!(b.alive(), Bunker::full_count());
        for c in 2..=4 {
            assert!(!b.cells[3][c], "bottom middle open");
        }
        assert!(b.cells[3][0] && b.cells[3][6], "feet");
        assert!(!b.cells[0][0] && !b.cells[0][6], "rounded top corners");
        assert_eq!(b.aabb().w, 56.0);
        assert_eq!(b.aabb().h, 32.0);
    }

    #[test]
    fn one_hit_removes_exactly_one_cell_from_either_side() {
        // Enemy bolt from above removes the topmost cell of its column.
        let mut b = Bunker::new(0.0, 0.0);
        let bolt = Aabb::from_center(28.0, 2.0, 6.0, 12.0);
        let (r, _) = b.hit(&bolt, false).expect("hit");
        assert_eq!(r, 0);
        assert_eq!(b.alive(), Bunker::full_count() - 1);
        // A player bolt from below removes the lowest cell of its column.
        let mut b = Bunker::new(0.0, 0.0);
        let bolt = Aabb::from_center(4.0, 30.0, 4.0, 14.0);
        let (r, c) = b.hit(&bolt, true).expect("hit");
        assert_eq!((r, c), (3, 0));
        assert_eq!(b.alive(), Bunker::full_count() - 1);
    }

    #[test]
    fn a_bolt_through_the_opening_misses_and_cells_are_not_removed_twice() {
        let mut b = Bunker::new(0.0, 0.0);
        // Straight up the open bottom middle: first cell hit is row 2.
        let bolt = Aabb::from_center(28.0, 22.0, 4.0, 4.0);
        assert!(!b.cells[3][3]);
        let (r, _) = b.hit(&bolt, true).expect("hits the cell above the opening");
        assert_eq!(r, 2);
        // Same spot again: the next cell up is hit, never the removed one.
        assert!(
            b.hit(&Aabb::from_center(28.0, 20.0, 4.0, 4.0), true)
                .is_none()
        );
        let (r2, c2) = b
            .hit(&Aabb::from_center(28.0, 12.0, 4.0, 4.0), true)
            .unwrap();
        assert_eq!((r2, c2), (1, 3), "next cell up");
        assert_eq!(b.alive(), Bunker::full_count() - 2);
        // Outside the bunker: no hit.
        assert!(b.hit(&Aabb::new(500.0, 500.0, 4.0, 4.0), false).is_none());
    }

    #[test]
    fn crush_and_repair_restore_the_dome() {
        let mut b = Bunker::new(0.0, 0.0);
        let n = b.crush(&Aabb::new(0.0, 0.0, 56.0, 16.0));
        assert!(n > 0 && b.is_damaged());
        b.repair();
        assert_eq!(b.cells, SHAPE);
        assert!(!b.is_damaged());
    }

    #[test]
    fn a_fast_bolt_cannot_tunnel_through_a_cell_row() {
        // Step a bolt upwards at 760 u/s with a 1/120 s step from many phases:
        // every run must remove a cell before reaching the top of the bunker.
        for phase in 0..16 {
            let mut b = Bunker::new(0.0, 100.0);
            let mut y = 140.0 + phase as f32 * 0.37;
            let mut hit = false;
            for _ in 0..200 {
                y -= 760.0 / 120.0;
                let bolt = Aabb::from_center(4.0, y, 4.0, 14.0);
                if b.hit(&bolt, true).is_some() {
                    hit = true;
                    break;
                }
            }
            assert!(hit, "phase {phase}: bolt tunnelled");
        }
    }
}
