//! Rounded rectangle primitives (macroquad has none): rectangles for the
//! body plus triangle fans for the corners, so translucent fills never
//! overlap themselves.

use std::f32::consts::{FRAC_PI_2, PI};

use macroquad::prelude::*;

const ARC_SEGMENTS: usize = 6;

/// Points of the quarter arc around `center` starting at angle `a0`.
fn arc(center: Vec2, r: f32, a0: f32) -> [Vec2; ARC_SEGMENTS + 1] {
    let mut pts = [Vec2::ZERO; ARC_SEGMENTS + 1];
    for (i, p) in pts.iter_mut().enumerate() {
        let a = a0 + FRAC_PI_2 * i as f32 / ARC_SEGMENTS as f32;
        *p = center + vec2(a.cos(), a.sin()) * r;
    }
    pts
}

/// Corner radius clamped so it always fits.
pub fn clamp_radius(w: f32, h: f32, r: f32) -> f32 {
    r.clamp(0.0, w.min(h) * 0.5)
}

fn corners(rect: Rect, r: f32) -> [(Vec2, f32); 4] {
    [
        (vec2(rect.x + r, rect.y + r), PI),
        (vec2(rect.right() - r, rect.y + r), PI * 1.5),
        (vec2(rect.right() - r, rect.bottom() - r), 0.0),
        (vec2(rect.x + r, rect.bottom() - r), FRAC_PI_2),
    ]
}

pub fn fill_rounded(rect: Rect, r: f32, color: Color) {
    let r = clamp_radius(rect.w, rect.h, r);
    if r < 0.5 {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
        return;
    }
    draw_rectangle(rect.x + r, rect.y, rect.w - 2.0 * r, rect.h, color);
    draw_rectangle(rect.x, rect.y + r, r, rect.h - 2.0 * r, color);
    draw_rectangle(rect.right() - r, rect.y + r, r, rect.h - 2.0 * r, color);
    for (c, a0) in corners(rect, r) {
        let pts = arc(c, r, a0);
        for w in pts.windows(2) {
            draw_triangle(c, w[0], w[1], color);
        }
    }
}

pub fn outline_rounded(rect: Rect, r: f32, thickness: f32, color: Color) {
    let r = clamp_radius(rect.w, rect.h, r);
    if r < 0.5 {
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, thickness * 2.0, color);
        return;
    }
    let (l, t, rt, b) = (rect.x, rect.y, rect.right(), rect.bottom());
    draw_line(l + r, t, rt - r, t, thickness, color);
    draw_line(l + r, b, rt - r, b, thickness, color);
    draw_line(l, t + r, l, b - r, thickness, color);
    draw_line(rt, t + r, rt, b - r, thickness, color);
    for (c, a0) in corners(rect, r) {
        let pts = arc(c, r, a0);
        for w in pts.windows(2) {
            draw_line(w[0].x, w[0].y, w[1].x, w[1].y, thickness, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_is_clamped_to_half_the_short_side() {
        assert_eq!(clamp_radius(100.0, 20.0, 50.0), 10.0);
        assert_eq!(clamp_radius(100.0, 20.0, 4.0), 4.0);
        assert_eq!(clamp_radius(100.0, 20.0, -3.0), 0.0);
    }

    #[test]
    fn arc_ends_are_on_the_circle() {
        let pts = arc(vec2(10.0, 10.0), 5.0, 0.0);
        assert!((pts[0] - vec2(15.0, 10.0)).length() < 1e-4);
        assert!((pts[ARC_SEGMENTS] - vec2(10.0, 15.0)).length() < 1e-4);
    }
}
