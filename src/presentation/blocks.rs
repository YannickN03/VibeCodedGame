//! 4-layer mino rendering, ghost pieces, colour-blind symbols and previews.

use macroquad::prelude::*;

use crate::core::{PieceKind, Rotation};

use super::theme::{darken, lighten, with_alpha};

/// Draws one mino: dark 1.5 px frame, main colour, 3D bevel (light top/left,
/// shadow bottom/right) and an inner gloss highlight.
pub fn mino(x: f32, y: f32, s: f32, base: Color, kind: Option<PieceKind>, colorblind: bool) {
    let a = base.a;
    // 1) dark outer frame
    draw_rectangle(
        x,
        y,
        s,
        s,
        Color::new(base.r * 0.18, base.g * 0.18, base.b * 0.22, a),
    );
    let f = 1.5;
    let (x0, y0, x1, y1) = (x + f, y + f, x + s - f, y + s - f);
    // 2) main colour
    draw_rectangle(x0, y0, x1 - x0, y1 - y0, base);
    // 3) bevel
    let b = (s * 0.13).max(2.0);
    let light = with_alpha(lighten(base, 0.45), a);
    let shade = with_alpha(darken(base, 0.42), a);
    let (p00, p10, p11, p01) = (vec2(x0, y0), vec2(x1, y0), vec2(x1, y1), vec2(x0, y1));
    let (i00, i10, i11, i01) = (
        vec2(x0 + b, y0 + b),
        vec2(x1 - b, y0 + b),
        vec2(x1 - b, y1 - b),
        vec2(x0 + b, y1 - b),
    );
    // top
    draw_triangle(p00, p10, i10, light);
    draw_triangle(p00, i10, i00, light);
    // left
    draw_triangle(p00, i00, i01, with_alpha(lighten(base, 0.25), a));
    draw_triangle(p00, i01, p01, with_alpha(lighten(base, 0.25), a));
    // bottom
    draw_triangle(p01, i01, i11, shade);
    draw_triangle(p01, i11, p11, shade);
    // right
    draw_triangle(p10, p11, i11, with_alpha(darken(base, 0.28), a));
    draw_triangle(p10, i11, i10, with_alpha(darken(base, 0.28), a));
    // 4) inner gloss
    let gw = (x1 - x0 - 2.0 * b) * 0.55;
    let gh = (y1 - y0 - 2.0 * b) * 0.22;
    draw_rectangle(
        i00.x + 1.0,
        i00.y + 1.0,
        gw,
        gh,
        Color::new(1.0, 1.0, 1.0, 0.22 * a),
    );
    draw_rectangle(
        i00.x + 1.0,
        i00.y + 1.0,
        gh * 0.8,
        gh * 0.8,
        Color::new(1.0, 1.0, 1.0, 0.28 * a),
    );

    if colorblind && let Some(k) = kind {
        let ink = Color::new(0.0, 0.0, 0.0, 0.62 * a);
        symbol(x + s * 0.5, y + s * 0.5, s * 0.22, k, ink);
    }
}

/// Unique geometric symbol per piece kind (colour-blind mode).
pub fn symbol(cx: f32, cy: f32, r: f32, kind: PieceKind, col: Color) {
    let t = (r * 0.38).max(1.5);
    match kind {
        PieceKind::I => draw_rectangle(cx - r, cy - t * 0.5, 2.0 * r, t, col),
        PieceKind::O => draw_poly_lines(cx, cy, 24, r * 0.9, 0.0, t, col),
        PieceKind::T => draw_poly(cx, cy + r * 0.15, 3, r * 1.05, -90.0, col),
        PieceKind::S => draw_poly(cx, cy, 4, r, 0.0, col),
        PieceKind::Z => {
            draw_line(cx - r, cy - r, cx + r, cy + r, t, col);
            draw_line(cx - r, cy + r, cx + r, cy - r, t, col);
        }
        PieceKind::J => {
            draw_rectangle(cx - r, cy - t * 0.5, 2.0 * r, t, col);
            draw_rectangle(cx - t * 0.5, cy - r, t, 2.0 * r, col);
        }
        PieceKind::L => draw_circle(cx, cy, r * 0.55, col),
    }
}

/// Ghost mino: coloured 2 px frame with 15 % fill.
pub fn ghost(x: f32, y: f32, s: f32, col: Color) {
    draw_rectangle(x + 1.0, y + 1.0, s - 2.0, s - 2.0, with_alpha(col, 0.15));
    draw_rectangle_lines(
        x + 1.0,
        y + 1.0,
        s - 2.0,
        s - 2.0,
        2.0,
        with_alpha(col, 0.75),
    );
}

/// Draws a whole piece centred in a box (hold / next previews).
pub fn preview(kind: PieceKind, cx: f32, cy: f32, s: f32, col: Color, colorblind: bool) {
    let cells = kind.cells(Rotation::Spawn);
    let (mut minx, mut maxx, mut miny, mut maxy) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for &(x, y) in &cells {
        minx = minx.min(x);
        maxx = maxx.max(x);
        miny = miny.min(y);
        maxy = maxy.max(y);
    }
    let w = (maxx - minx + 1) as f32 * s;
    let h = (maxy - miny + 1) as f32 * s;
    for &(x, y) in &cells {
        mino(
            cx - w * 0.5 + (x - minx) as f32 * s,
            cy - h * 0.5 + (y - miny) as f32 * s,
            s,
            col,
            Some(kind),
            colorblind,
        );
    }
}
