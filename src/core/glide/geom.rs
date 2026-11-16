//! Axis-aligned boxes and the exact circle-against-rectangle test.
//! `(x, y)` is the top-left corner, `+y` points down.

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

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Distance from a point to the box (`0` inside or on the border).
    pub fn distance_to(&self, px: f32, py: f32) -> f32 {
        let dx = (self.x - px).max(0.0).max(px - self.right());
        let dy = (self.y - py).max(0.0).max(py - self.bottom());
        (dx * dx + dy * dy).sqrt()
    }
}

/// `true` if the circle overlaps the rectangle. Merely touching (distance
/// exactly `r`) is not a hit; a centre inside the rectangle always is.
pub fn circle_vs_aabb(cx: f32, cy: f32, r: f32, b: &Aabb) -> bool {
    let nx = cx.clamp(b.x, b.right());
    let ny = cy.clamp(b.y, b.bottom());
    let (dx, dy) = (cx - nx, cy - ny);
    dx * dx + dy * dy < r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Aabb = Aabb::new(10.0, 10.0, 20.0, 10.0);

    #[test]
    fn tangent_contact_is_not_a_hit_but_a_hair_deeper_is() {
        // Circle to the left of the box: distance from centre to the edge == r.
        assert!(!circle_vs_aabb(0.0, 15.0, 10.0, &B));
        assert!(circle_vs_aabb(0.001, 15.0, 10.0, &B));
        // Above the top edge.
        assert!(!circle_vs_aabb(20.0, 0.0, 10.0, &B));
        assert!(circle_vs_aabb(20.0, 0.01, 10.0, &B));
    }

    #[test]
    fn corners_use_the_diagonal_distance() {
        // Corner at (10, 10); the centre sits on the 45 degree diagonal.
        let d = 10.0 / 2.0_f32.sqrt(); // axis offset giving distance exactly 10
        assert!(!circle_vs_aabb(10.0 - d, 10.0 - d, 10.0, &B), "tangent");
        assert!(circle_vs_aabb(10.0 - d + 0.1, 10.0 - d + 0.1, 10.0, &B));
        // The bounding boxes overlap but the corner is out of reach.
        assert!(!circle_vs_aabb(5.0, 5.0, 7.0, &B), "dist 7.07 > 7");
        assert!(circle_vs_aabb(5.0, 5.0, 7.2, &B));
    }

    #[test]
    fn centre_inside_and_containing_circles_hit() {
        assert!(circle_vs_aabb(20.0, 15.0, 1.0, &B));
        assert!(
            circle_vs_aabb(20.0, 15.0, 100.0, &B),
            "box inside the circle"
        );
        assert!(circle_vs_aabb(10.0, 10.0, 0.5, &B), "centre on the corner");
    }

    #[test]
    fn distance_and_lip_overhang() {
        assert_eq!(B.distance_to(20.0, 15.0), 0.0);
        assert!((B.distance_to(40.0, 15.0) - 10.0).abs() < 1e-5);
        // A 76-wide pipe body with an 8-unit lip overhang on each side: a
        // circle that only touches the lip misses the body.
        let body = Aabb::new(100.0, 0.0, 76.0, 300.0);
        let lip = Aabb::new(92.0, 274.0, 92.0, 26.0);
        let (cx, cy) = (92.0 - 10.0, 285.0);
        assert!(!circle_vs_aabb(cx, cy, 15.0, &body), "body alone is missed");
        assert!(circle_vs_aabb(cx, cy, 15.0, &lip), "the lip overhang hits");
    }
}
