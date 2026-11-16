//! Sub-step interpolation of the snake for smooth rendering.
//!
//! Each segment glides from its cell before the last step (`prev`) to its
//! cell after it (`curr`). Jumps of more than one cell (portal wrap, Zen
//! trim) are never interpolated, so no streak is drawn across the field,
//! and links between segments that are not neighbours are dropped.

use std::collections::VecDeque;

use super::grid::GridPos;

/// A segment position in fractional cell coordinates (top-left of the cell).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderPoint {
    pub x: f32,
    pub y: f32,
    /// `true` if this segment is joined to the previous (closer to the head)
    /// one by body; `false` for the head and across wrap jumps.
    pub linked: bool,
}

/// Interpolated segments for `alpha` in `0..=1` (`0` = before the step).
pub fn render_points(
    prev: &VecDeque<GridPos>,
    curr: &VecDeque<GridPos>,
    alpha: f32,
) -> Vec<RenderPoint> {
    let a = alpha.clamp(0.0, 1.0);
    curr.iter()
        .enumerate()
        .map(|(i, &to)| {
            // While growing, `prev` is shorter: the new tail segment starts
            // on the old tail (it stays in place).
            let from = prev.get(i).or(prev.back()).copied().unwrap_or(to);
            let (x, y) = if from.manhattan(to) <= 1 {
                (
                    from.x as f32 + (to.x - from.x) as f32 * a,
                    from.y as f32 + (to.y - from.y) as f32 * a,
                )
            } else {
                (to.x as f32, to.y as f32)
            };
            let linked = i > 0 && curr[i - 1].manhattan(to) == 1;
            RenderPoint { x, y, linked }
        })
        .collect()
}

/// Looking direction of the eyes: unit vector from `head` towards `target`
/// (zero if they coincide), in cell coordinates.
pub fn look_vector(head: (f32, f32), target: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (target.0 - head.0, target.1 - head.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        (0.0, 0.0)
    } else {
        (dx / len, dy / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dq(cells: &[(i32, i32)]) -> VecDeque<GridPos> {
        cells.iter().map(|&(x, y)| GridPos::new(x, y)).collect()
    }

    #[test]
    fn glides_between_cells() {
        let prev = dq(&[(5, 5), (4, 5), (3, 5)]);
        let curr = dq(&[(6, 5), (5, 5), (4, 5)]);
        let p0 = render_points(&prev, &curr, 0.0);
        assert_eq!((p0[0].x, p0[2].x), (5.0, 3.0));
        let p5 = render_points(&prev, &curr, 0.5);
        assert_eq!((p5[0].x, p5[1].x, p5[2].x), (5.5, 4.5, 3.5));
        let p1 = render_points(&prev, &curr, 1.0);
        assert_eq!((p1[0].x, p1[2].x), (6.0, 4.0));
        assert!(!p1[0].linked && p1[1].linked && p1[2].linked);
    }

    #[test]
    fn wrap_jump_is_not_interpolated_and_not_linked() {
        // Head crosses the right edge: (31,5) -> (0,5).
        let prev = dq(&[(31, 5), (30, 5), (29, 5)]);
        let curr = dq(&[(0, 5), (31, 5), (30, 5)]);
        for alpha in [0.0, 0.3, 0.5, 0.99, 1.0] {
            let p = render_points(&prev, &curr, alpha);
            // The head never appears in the middle of the field.
            assert_eq!(p[0].x, 0.0, "alpha {alpha}");
            assert!(!p[1].linked, "no body drawn across the field");
            assert!(p[2].linked);
        }
    }

    #[test]
    fn growth_keeps_the_new_tail_in_place() {
        let prev = dq(&[(5, 5), (4, 5)]);
        let curr = dq(&[(6, 5), (5, 5), (4, 5)]);
        for alpha in [0.0, 0.5, 1.0] {
            let p = render_points(&prev, &curr, alpha);
            assert_eq!(p.len(), 3);
            assert_eq!(p[2].x, 4.0, "tail does not move while growing");
        }
    }

    #[test]
    fn never_longer_than_one_cell_from_the_previous_cell() {
        // Property: every interpolated position lies within one cell of its
        // target cell (no streaks), including across wraps and trims.
        let prev = dq(&[(0, 0), (0, 17), (0, 16), (1, 16)]);
        let curr = dq(&[(0, 1), (0, 0), (0, 17), (0, 16)]);
        for alpha in [0.0, 0.25, 0.75, 1.0] {
            for (pt, c) in render_points(&prev, &curr, alpha).iter().zip(&curr) {
                assert!((pt.x - c.x as f32).abs() <= 1.0 && (pt.y - c.y as f32).abs() <= 1.0);
            }
        }
    }

    #[test]
    fn eyes_look_towards_the_apple() {
        let (x, y) = look_vector((1.0, 1.0), (4.0, 5.0));
        assert!((x - 0.6).abs() < 1e-5 && (y - 0.8).abs() < 1e-5);
        assert_eq!(look_vector((2.0, 2.0), (2.0, 2.0)), (0.0, 0.0));
    }
}
