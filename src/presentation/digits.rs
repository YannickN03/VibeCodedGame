//! Geometric vector digit renderer for big numbers (score, level, timer …).
//!
//! Glyphs are built from bevelled hexagonal segments (7-segment style), so
//! they stay razor sharp at any scale. Supported: `0-9 : . x % # -` and space.

use macroquad::prelude::*;

use super::theme::with_alpha;

/// Segment bits: a b c d e f g.
const A: u8 = 1;
const B: u8 = 2;
const C: u8 = 4;
const D: u8 = 8;
const E: u8 = 16;
const F: u8 = 32;
const G: u8 = 64;

fn segments(ch: char) -> Option<u8> {
    Some(match ch {
        '0' => A | B | C | D | E | F,
        '1' => B | C,
        '2' => A | B | G | E | D,
        '3' => A | B | G | C | D,
        '4' => F | G | B | C,
        '5' => A | F | G | C | D,
        '6' => A | F | G | E | D | C,
        '7' => A | B | C,
        '8' => A | B | C | D | E | F | G,
        '9' => A | B | C | D | F | G,
        '-' => G,
        _ => return None,
    })
}

/// Relative advance width of a glyph (in units of glyph height).
fn advance(ch: char) -> f32 {
    match ch {
        ':' | '.' => 0.30,
        ' ' => 0.35,
        'x' => 0.50,
        '%' | '#' => 0.66,
        _ => 0.58,
    }
}

const SPACING: f32 = 0.14;

/// Total width of `text` at glyph height `h`.
pub fn width(text: &str, h: f32) -> f32 {
    let n = text.chars().count();
    if n == 0 {
        return 0.0;
    }
    text.chars().map(|c| advance(c) * h).sum::<f32>() + SPACING * h * (n - 1) as f32
}

fn hex_h(x1: f32, x2: f32, yc: f32, ht: f32, col: Color) {
    if x2 - x1 < 2.0 * ht {
        draw_rectangle(x1, yc - ht, x2 - x1, 2.0 * ht, col);
        return;
    }
    let p = [
        vec2(x1, yc),
        vec2(x1 + ht, yc - ht),
        vec2(x2 - ht, yc - ht),
        vec2(x2, yc),
        vec2(x2 - ht, yc + ht),
        vec2(x1 + ht, yc + ht),
    ];
    for i in 1..5 {
        draw_triangle(p[0], p[i], p[i + 1], col);
    }
}

fn hex_v(xc: f32, y1: f32, y2: f32, ht: f32, col: Color) {
    if y2 - y1 < 2.0 * ht {
        draw_rectangle(xc - ht, y1, 2.0 * ht, y2 - y1, col);
        return;
    }
    let p = [
        vec2(xc, y1),
        vec2(xc + ht, y1 + ht),
        vec2(xc + ht, y2 - ht),
        vec2(xc, y2),
        vec2(xc - ht, y2 - ht),
        vec2(xc - ht, y1 + ht),
    ];
    for i in 1..5 {
        draw_triangle(p[0], p[i], p[i + 1], col);
    }
}

fn diamond(cx: f32, cy: f32, r: f32, col: Color) {
    let (t, rr, b, l) = (
        vec2(cx, cy - r),
        vec2(cx + r, cy),
        vec2(cx, cy + r),
        vec2(cx - r, cy),
    );
    draw_triangle(t, rr, b, col);
    draw_triangle(t, b, l, col);
}

fn glyph(ch: char, x: f32, y: f32, h: f32, t: f32, col: Color) {
    let w = advance(ch) * h;
    let ht = t * 0.5;
    let gap = t * 0.18;
    if let Some(seg) = segments(ch) {
        // '1' is centred instead of hugging the right edge.
        let (l, r) = if ch == '1' {
            (x + w * 0.5, x + w * 0.5)
        } else {
            (x + ht, x + w - ht)
        };
        let (top, mid, bot) = (y + ht, y + h * 0.5, y + h - ht);
        let on = |s: u8| seg & s != 0;
        if on(A) {
            hex_h(l + gap, r - gap, top, ht, col);
        }
        if on(G) {
            hex_h(l + gap, r - gap, mid, ht, col);
        }
        if on(D) {
            hex_h(l + gap, r - gap, bot, ht, col);
        }
        if on(F) {
            hex_v(l, top + gap, mid - gap, ht, col);
        }
        if on(B) {
            hex_v(r, top + gap, mid - gap, ht, col);
        }
        if on(E) {
            hex_v(l, mid + gap, bot - gap, ht, col);
        }
        if on(C) {
            hex_v(r, mid + gap, bot - gap, ht, col);
        }
        return;
    }
    let cx = x + w * 0.5;
    match ch {
        ':' => {
            diamond(cx, y + h * 0.32, t * 0.75, col);
            diamond(cx, y + h * 0.72, t * 0.75, col);
        }
        '.' => diamond(cx, y + h - t * 0.8, t * 0.75, col),
        'x' => {
            let (x0, x1, y0, y1) = (x + t * 0.4, x + w - t * 0.4, y + h * 0.38, y + h - t * 0.4);
            draw_line(x0, y0, x1, y1, t * 0.9, col);
            draw_line(x0, y1, x1, y0, t * 0.9, col);
        }
        '%' => {
            draw_line(
                x + w - t * 0.5,
                y + t * 0.5,
                x + t * 0.5,
                y + h - t * 0.5,
                t * 0.8,
                col,
            );
            diamond(x + w * 0.22, y + h * 0.2, t * 0.85, col);
            diamond(x + w * 0.78, y + h * 0.8, t * 0.85, col);
        }
        '#' => {
            let tt = t * 0.75;
            draw_line(
                x + w * 0.38,
                y + t * 0.3,
                x + w * 0.28,
                y + h - t * 0.3,
                tt,
                col,
            );
            draw_line(
                x + w * 0.74,
                y + t * 0.3,
                x + w * 0.64,
                y + h - t * 0.3,
                tt,
                col,
            );
            draw_line(
                x + t * 0.3,
                y + h * 0.36,
                x + w - t * 0.3,
                y + h * 0.36,
                tt,
                col,
            );
            draw_line(
                x + t * 0.2,
                y + h * 0.66,
                x + w - t * 0.4,
                y + h * 0.66,
                tt,
                col,
            );
        }
        _ => {}
    }
}

/// Draws `text` with its top-left at `(x, y)` and glyph height `h`.
/// A soft glow layer sits underneath. Returns the drawn width.
pub fn draw(text: &str, x: f32, y: f32, h: f32, col: Color) -> f32 {
    let t = (h * 0.13).max(1.5);
    // Drop shadow + glow pass
    let mut cx = x;
    for ch in text.chars() {
        glyph(
            ch,
            cx + h * 0.04,
            y + h * 0.05,
            h,
            t,
            Color::new(0.0, 0.0, 0.0, 0.45 * col.a),
        );
        glyph(ch, cx, y, h, t * 1.9, with_alpha(col, 0.13 * col.a));
        cx += (advance(ch) + SPACING) * h;
    }
    let mut cx = x;
    for ch in text.chars() {
        glyph(ch, cx, y, h, t, col);
        cx += (advance(ch) + SPACING) * h;
    }
    width(text, h)
}

/// Right-aligned variant.
pub fn draw_right(text: &str, right: f32, y: f32, h: f32, col: Color) -> f32 {
    draw(text, right - width(text, h), y, h, col)
}

/// Centered variant.
pub fn draw_centered(text: &str, cx: f32, y: f32, h: f32, col: Color) -> f32 {
    draw(text, cx - width(text, h) * 0.5, y, h, col)
}

/// `mm:ss.cc` formatting for timers.
pub fn format_time(seconds: f64) -> String {
    let cs = (seconds.max(0.0) * 100.0).floor() as u64;
    format!("{}:{:02}.{:02}", cs / 6000, (cs / 100) % 60, cs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_supported_glyphs_known() {
        for ch in "0123456789-".chars() {
            assert!(segments(ch).is_some());
        }
        assert!(width("12:34.56x%#", 20.0) > 0.0);
    }

    #[test]
    fn time_format() {
        assert_eq!(format_time(0.0), "0:00.00");
        assert_eq!(format_time(83.456), "1:23.45");
        assert_eq!(format_time(600.0), "10:00.00");
    }
}
