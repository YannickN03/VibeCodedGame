//! Axis-aligned boxes for deterministic collision checks (core cannot use
//! `macroquad::Rect`). `(x, y)` is the top-left corner, `+y` points down.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Aabb {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Aabb { x, y, w, h }
    }

    pub fn from_center(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Aabb::new(cx - w * 0.5, cy - h * 0.5, w, h)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Strict overlap: boxes that merely touch along an edge do not collide.
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    /// Distance from the point to the box (`0` inside or on the border).
    pub fn distance_to(&self, px: f32, py: f32) -> f32 {
        let dx = (self.x - px).max(0.0).max(px - self.right());
        let dy = (self.y - py).max(0.0).max(py - self.bottom());
        (dx * dx + dy * dy).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_touching_edges_and_containment() {
        let a = Aabb::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.overlaps(&Aabb::new(5.0, 5.0, 10.0, 10.0)));
        assert!(
            !a.overlaps(&Aabb::new(10.0, 0.0, 5.0, 5.0)),
            "touching on x"
        );
        assert!(
            !a.overlaps(&Aabb::new(0.0, 10.0, 5.0, 5.0)),
            "touching on y"
        );
        assert!(
            !a.overlaps(&Aabb::new(10.0, 10.0, 5.0, 5.0)),
            "touching corner"
        );
        assert!(a.overlaps(&Aabb::new(2.0, 2.0, 3.0, 3.0)), "contained");
        assert!(Aabb::new(2.0, 2.0, 3.0, 3.0).overlaps(&a), "contains");
        assert!(a.overlaps(&Aabb::new(9.999, 9.999, 5.0, 5.0)));
        assert!(!a.overlaps(&Aabb::new(-5.0, -5.0, 4.0, 4.0)));
    }

    #[test]
    fn overlap_is_symmetric() {
        let boxes = [
            Aabb::new(0.0, 0.0, 4.0, 4.0),
            Aabb::new(3.0, 3.0, 4.0, 4.0),
            Aabb::new(4.0, 0.0, 2.0, 2.0),
            Aabb::new(-1.0, 1.0, 20.0, 1.0),
        ];
        for a in &boxes {
            for b in &boxes {
                assert_eq!(a.overlaps(b), b.overlaps(a));
            }
        }
    }

    #[test]
    fn center_and_distance() {
        let a = Aabb::from_center(10.0, 20.0, 4.0, 6.0);
        assert_eq!(a.center(), (10.0, 20.0));
        assert_eq!(a.distance_to(10.0, 20.0), 0.0);
        assert_eq!(a.distance_to(12.0, 20.0), 0.0, "on the border");
        assert!((a.distance_to(15.0, 20.0) - 3.0).abs() < 1e-5);
        assert!((a.distance_to(13.0, 27.0) - 1.0_f32.hypot(4.0)).abs() < 1e-5);
    }
}
