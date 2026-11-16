//! Axis-aligned boxes in tile units. `x` grows to the right, `y` is the
//! row coordinate (`+y` = forward). A row `r` covers `y` in `r..r + 1`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Aabb {
    pub fn from_center(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Aabb {
            x: cx - w * 0.5,
            y: cy - h * 0.5,
            w,
            h,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn top(&self) -> f32 {
        self.y + self.h
    }

    /// Strict overlap: boxes that merely touch do not collide.
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.top() && o.y < self.top()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_is_strict_and_symmetric() {
        let a = Aabb::from_center(5.0, 5.0, 2.0, 2.0);
        let touching = Aabb::from_center(7.0, 5.0, 2.0, 2.0);
        assert!(!a.overlaps(&touching) && !touching.overlaps(&a));
        let over = Aabb::from_center(6.9, 5.0, 2.0, 2.0);
        assert!(a.overlaps(&over) && over.overlaps(&a));
        assert!(
            a.overlaps(&Aabb::from_center(5.0, 5.0, 0.1, 0.1)),
            "contained"
        );
        assert!(!a.overlaps(&Aabb::from_center(5.0, 9.0, 2.0, 2.0)));
    }
}
