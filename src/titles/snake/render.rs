//! Snake drawing: tapered body, colour-blind back pattern, animated eyes,
//! apples with countdown ring, board and HUD. All coordinates are in stage
//! units; cell coordinates may be fractional (interpolated positions).

use macroquad::prelude::*;

use crate::core::snake::game::{
    GRID_H, GRID_W, MAX_SPEED, MIN_SPEED, PULSE_SECS, SnakeGame, SnakeMode, Status,
};
use crate::core::snake::interp::{RenderPoint, look_vector};
use crate::presentation::digits;
use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::{Theme, darken, lighten, mix, with_alpha};
use crate::presentation::widgets::shapes::outline_rounded;
use crate::presentation::widgets::{FlowMeterBar, Orientation, StatBox};
use crate::titles::blocks::mino;

pub const CELL: f32 = 32.0;
pub const BOARD_X: f32 = 128.0;
pub const BOARD_Y: f32 = 112.0;
pub const BOARD_W: f32 = CELL * GRID_W as f32;
pub const BOARD_H: f32 = CELL * GRID_H as f32;

/// Top-left stage position of cell `(x, y)` (fractional allowed).
pub fn cell_pos(x: f32, y: f32) -> Vec2 {
    vec2(BOARD_X + x * CELL, BOARD_Y + y * CELL)
}

pub fn cell_center(x: f32, y: f32) -> Vec2 {
    cell_pos(x, y) + vec2(CELL * 0.5, CELL * 0.5)
}

/// Colours of one snake.
#[derive(Clone, Copy, Debug)]
pub struct SnakeStyle {
    pub body: Color,
    pub head: Color,
    pub colorblind: bool,
    pub dead: bool,
}

impl SnakeStyle {
    pub fn from_theme(theme: &Theme, colorblind: bool, dead: bool) -> Self {
        let mut body = theme.pieces[3];
        if dead {
            body = mix(body, Color::new(0.35, 0.35, 0.38, 1.0), 0.65);
        }
        SnakeStyle {
            body,
            head: lighten(body, 0.25),
            colorblind,
            dead,
        }
    }
}

/// Width of segment `i` of `n` in cells: tapers from head to tail.
pub fn segment_width(i: usize, n: usize) -> f32 {
    let t = if n <= 1 {
        0.0
    } else {
        i as f32 / (n - 1) as f32
    };
    0.86 - 0.44 * t.powf(0.85)
}

fn heading_of(points: &[RenderPoint], fallback: (f32, f32)) -> (f32, f32) {
    if points.len() >= 2 && points[1].linked {
        let (dx, dy) = (points[0].x - points[1].x, points[0].y - points[1].y);
        let l = (dx * dx + dy * dy).sqrt();
        if l > 1e-3 {
            return (dx / l, dy / l);
        }
    }
    fallback
}

/// Draws the snake. `look_at` (cell coordinates) is what the eyes follow.
pub fn draw_snake(
    points: &[RenderPoint],
    origin: Vec2,
    cell: f32,
    style: &SnakeStyle,
    look_at: Option<(f32, f32)>,
    fallback_heading: (f32, f32),
    time: f32,
) {
    let n = points.len();
    if n == 0 {
        return;
    }
    let center = |p: &RenderPoint| origin + vec2(p.x + 0.5, p.y + 0.5) * cell;
    let color_at = |i: usize| {
        let t = if n <= 1 {
            0.0
        } else {
            i as f32 / (n - 1) as f32
        };
        mix(style.head, darken(style.body, 0.35), t.powf(0.8))
    };

    // Soft shadow under the body.
    for (i, p) in points.iter().enumerate() {
        let c = center(p) + vec2(2.0, 3.0);
        draw_circle(
            c.x,
            c.y,
            segment_width(i, n) * cell * 0.5,
            Color::new(0.0, 0.0, 0.0, 0.25),
        );
    }
    // Body: connected thick lines and round joints, tail first so the head
    // ends up on top. Links across a wrap jump are skipped.
    for i in (0..n).rev() {
        let c = center(&points[i]);
        let w = segment_width(i, n) * cell;
        if points[i].linked {
            let prev = center(&points[i - 1]);
            let w_prev = segment_width(i - 1, n) * cell;
            draw_line(c.x, c.y, prev.x, prev.y, (w + w_prev) * 0.5, color_at(i));
        }
        draw_circle(c.x, c.y, w * 0.5, color_at(i));
    }
    // Highlight stripe along the back.
    for (i, p) in points.iter().enumerate() {
        let c = center(p);
        let w = segment_width(i, n) * cell;
        draw_circle(
            c.x - w * 0.12,
            c.y - w * 0.14,
            w * 0.18,
            with_alpha(lighten(color_at(i), 0.6), 0.28),
        );
    }
    // Colour-blind pattern: alternating dark bars and light dots on the back.
    if style.colorblind {
        for i in 1..n {
            let p = &points[i];
            let c = center(p);
            let w = segment_width(i, n) * cell;
            let (dx, dy) = if p.linked {
                let q = &points[i - 1];
                (q.x - p.x, q.y - p.y)
            } else {
                (1.0, 0.0)
            };
            let l = (dx * dx + dy * dy).sqrt().max(1e-3);
            let (nx, ny) = (-dy / l, dx / l);
            if i % 2 == 0 {
                let h = w * 0.34;
                draw_line(
                    c.x - nx * h,
                    c.y - ny * h,
                    c.x + nx * h,
                    c.y + ny * h,
                    3.0,
                    with_alpha(BLACK, 0.55),
                );
            } else {
                draw_circle(c.x, c.y, w * 0.1, with_alpha(WHITE, 0.75));
            }
        }
    }
    draw_eyes(
        points,
        center(&points[0]),
        cell,
        style,
        look_at,
        fallback_heading,
        time,
    );
}

fn draw_eyes(
    points: &[RenderPoint],
    c: Vec2,
    cell: f32,
    style: &SnakeStyle,
    look_at: Option<(f32, f32)>,
    fallback_heading: (f32, f32),
    time: f32,
) {
    let (hx, hy) = heading_of(points, fallback_heading);
    let (nx, ny) = (-hy, hx);
    let eye_r = cell * 0.17;
    let fwd = cell * 0.10;
    let side = cell * 0.2;
    // Where do the pupils look? Towards the apple, else straight ahead.
    let (lx, ly) = match look_at {
        Some(t) => look_vector((points[0].x, points[0].y), t),
        None => (hx, hy),
    };
    let blink = (time % 4.3) < 0.12;
    for s in [-1.0f32, 1.0] {
        let e = c + vec2(hx * fwd + nx * side * s, hy * fwd + ny * side * s);
        if style.dead {
            let k = eye_r * 0.8;
            for (a, b) in [((-k, -k), (k, k)), ((-k, k), (k, -k))] {
                draw_line(
                    e.x + a.0,
                    e.y + a.1,
                    e.x + b.0,
                    e.y + b.1,
                    2.0,
                    Color::new(0.1, 0.05, 0.08, 1.0),
                );
            }
            continue;
        }
        let ry = if blink { eye_r * 0.2 } else { eye_r };
        draw_ellipse(e.x, e.y, eye_r, ry, 0.0, Color::new(0.97, 0.98, 1.0, 1.0));
        if !blink {
            let pr = eye_r * 0.52;
            let off = eye_r * 0.42;
            draw_circle(
                e.x + lx * off,
                e.y + ly * off,
                pr,
                Color::new(0.06, 0.08, 0.14, 1.0),
            );
            draw_circle(
                e.x + lx * off - pr * 0.3,
                e.y + ly * off - pr * 0.3,
                pr * 0.3,
                with_alpha(WHITE, 0.9),
            );
        }
    }
}

/// Partial ring (clockwise from the top), `frac` in `0..=1`.
pub fn draw_ring(center: Vec2, radius: f32, frac: f32, thickness: f32, color: Color) {
    let f = frac.clamp(0.0, 1.0);
    if f <= 0.0 {
        return;
    }
    let steps = ((48.0 * f).ceil() as usize).max(2);
    let mut prev: Option<Vec2> = None;
    for i in 0..=steps {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * f * i as f32 / steps as f32;
        let p = center + vec2(a.cos(), a.sin()) * radius;
        if let Some(q) = prev {
            draw_line(q.x, q.y, p.x, p.y, thickness, color);
        }
        prev = Some(p);
    }
}

pub fn draw_apple(center: Vec2, cell: f32, color: Color, time: f32) {
    let r = cell * 0.36 * (1.0 + 0.06 * (time * 5.0).sin());
    draw_circle(center.x, center.y, r * 1.8, with_alpha(color, 0.12));
    draw_circle(center.x, center.y, r * 1.35, with_alpha(color, 0.18));
    draw_circle(
        center.x + 1.5,
        center.y + 2.0,
        r,
        Color::new(0.0, 0.0, 0.0, 0.3),
    );
    draw_circle(center.x, center.y, r, color);
    draw_circle(
        center.x - r * 0.35,
        center.y - r * 0.4,
        r * 0.28,
        with_alpha(WHITE, 0.55),
    );
    // Leaf.
    draw_triangle(
        center + vec2(0.0, -r * 0.9),
        center + vec2(r * 0.7, -r * 1.35),
        center + vec2(r * 0.25, -r * 0.7),
        Color::new(0.4, 0.9, 0.5, 1.0),
    );
}

/// Golden pulse apple with a countdown ring (`frac` = time left / total).
pub fn draw_pulse_apple(
    center: Vec2,
    cell: f32,
    color: Color,
    danger: Color,
    frac: f32,
    time: f32,
) {
    let beat = 0.5 + 0.5 * (time * 9.0).sin();
    let r = cell * (0.34 + 0.05 * beat);
    draw_circle(
        center.x,
        center.y,
        r * 2.2,
        with_alpha(color, 0.10 + 0.08 * beat),
    );
    // Rotating star.
    let rot = time * 90.0;
    draw_poly(center.x, center.y, 4, r * 1.35, rot, color);
    draw_poly(
        center.x,
        center.y,
        4,
        r * 1.35,
        rot + 45.0,
        lighten(color, 0.25),
    );
    draw_circle(center.x, center.y, r * 0.75, lighten(color, 0.55));
    let ring = if frac < 0.3 {
        mix(danger, color, 0.3 + 0.7 * (time * 12.0).sin().abs())
    } else {
        color
    };
    draw_ring(center, cell * 0.62, 1.0, 1.5, with_alpha(ring, 0.25));
    draw_ring(center, cell * 0.62, frac, 3.0, ring);
}

/// Current flow-driven colour of the board frame.
fn frame_color(theme: &Theme, game: &SnakeGame) -> Color {
    theme.tiers[game.tier().index()]
}

pub fn draw_board(game: &SnakeGame, theme: &Theme, pulse: f32, time: f32) {
    let tcol = frame_color(theme, game);
    let (bx, by) = (BOARD_X, BOARD_Y);
    // Glow frame breathing with the beat.
    let glow = 0.14 + 0.28 * pulse * (0.3 + game.flow.flow / 100.0);
    for i in 1..=4 {
        let g = i as f32 * 3.0;
        draw_rectangle_lines(
            bx - g,
            by - g,
            BOARD_W + 2.0 * g,
            BOARD_H + 2.0 * g,
            2.0,
            with_alpha(tcol, glow / i as f32),
        );
    }
    draw_rectangle(bx, by, BOARD_W, BOARD_H, theme.well);
    // Checkerboard + grid dots.
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            if (x + y) % 2 == 0 {
                let p = cell_pos(x as f32, y as f32);
                draw_rectangle(p.x, p.y, CELL, CELL, with_alpha(WHITE, 0.022));
            }
        }
    }
    // Border: solid in walled modes, animated portal dashes when wrapping.
    if game.mode.wraps() {
        let dash = 24.0;
        let off = (time * 40.0) % (dash * 2.0);
        let col = with_alpha(theme.accent2, 0.55 + 0.3 * pulse);
        let mut t = -off;
        while t < BOARD_W {
            let (a, b) = (t.max(0.0), (t + dash).min(BOARD_W));
            if b > a {
                draw_line(bx + a, by, bx + b, by, 4.0, col);
                draw_line(
                    bx + BOARD_W - b,
                    by + BOARD_H,
                    bx + BOARD_W - a,
                    by + BOARD_H,
                    4.0,
                    col,
                );
            }
            t += dash * 2.0;
        }
        let mut t = -off;
        while t < BOARD_H {
            let (a, b) = (t.max(0.0), (t + dash).min(BOARD_H));
            if b > a {
                draw_line(bx, by + BOARD_H - b, bx, by + BOARD_H - a, 4.0, col);
                draw_line(bx + BOARD_W, by + a, bx + BOARD_W, by + b, 4.0, col);
            }
            t += dash * 2.0;
        }
    } else {
        draw_rectangle_lines(bx, by, BOARD_W, BOARD_H, 4.0, with_alpha(tcol, 0.8));
    }
    // Obstacles as bevelled blocks.
    for o in &game.obstacles {
        let p = cell_pos(o.x as f32, o.y as f32);
        mino::mino(
            p.x,
            p.y,
            CELL,
            mix(theme.text_dim, theme.panel_border, 0.4),
            None,
            false,
        );
    }
}

pub fn draw_apples(game: &SnakeGame, theme: &Theme, time: f32) {
    if let Some(a) = game.apple {
        draw_apple(
            cell_center(a.pos.x as f32, a.pos.y as f32),
            CELL,
            theme.pieces[4],
            time,
        );
    }
    if let Some(a) = game.pulse {
        draw_pulse_apple(
            cell_center(a.pos.x as f32, a.pos.y as f32),
            CELL,
            theme.pieces[1],
            theme.danger,
            a.time_left / PULSE_SECS,
            time,
        );
    }
}

/// Music-intensity stage (`0..5`) from the current base speed.
pub fn music_stage(game: &SnakeGame) -> usize {
    let base = crate::core::snake::game::steps_per_second(game.apples_eaten);
    (((base - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)) * 5.0)
        .floor()
        .clamp(0.0, 4.0) as usize
}

pub struct HudCtx<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub mode_label: &'a str,
    pub best: Option<u64>,
    pub today_iso: &'a str,
    pub pulse: f32,
    pub legend: &'a str,
}

pub fn draw_hud(game: &SnakeGame, c: &HudCtx) {
    let th = c.theme;
    let st = c.stage;
    StatBox {
        label: "SCORE",
        value: &game.score.to_string(),
        pos: vec2(BOARD_X, 12.0),
        width: 230.0,
        digit_h: 34.0,
        color: None,
    }
    .draw(st, th);
    let best = c.best.map(|b| b.to_string()).unwrap_or_else(|| "-".into());
    StatBox {
        label: "BEST",
        value: &best,
        pos: vec2(BOARD_X + 262.0, 12.0),
        width: 140.0,
        digit_h: 34.0,
        color: Some(th.text_dim),
    }
    .draw(st, th);

    text::label(
        st,
        c.mode_label,
        640.0 + 60.0,
        8.0,
        Size::X1,
        th.accent2,
        Align::Left,
    );
    FlowMeterBar {
        rect: Rect::new(640.0 + 60.0, 52.0, 260.0, 14.0),
        orientation: Orientation::Horizontal,
        flow: game.flow.flow,
        pulse: c.pulse,
    }
    .draw(st, th);

    StatBox {
        label: "LENGTH",
        value: &game.segments.len().to_string(),
        pos: vec2(BOARD_X + BOARD_W - 190.0, 12.0),
        width: 80.0,
        digit_h: 34.0,
        color: None,
    }
    .draw(st, th);
    StatBox {
        label: "SPEED",
        value: &format!("{:.1}", game.speed()),
        pos: vec2(BOARD_X + BOARD_W - 90.0, 12.0),
        width: 90.0,
        digit_h: 34.0,
        color: Some(th.accent2),
    }
    .draw(st, th);

    text::label(
        st,
        c.legend,
        BOARD_X,
        BOARD_Y + BOARD_H + 14.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    if game.mode == SnakeMode::DailyVibe {
        digits::draw_right(
            &format!("#{}", c.today_iso),
            BOARD_X + BOARD_W,
            BOARD_Y + BOARD_H + 10.0,
            20.0,
            th.accent2,
        );
    }
}

/// Full-screen tint while the snake is dead.
pub fn draw_death_tint(game: &SnakeGame, theme: &Theme) {
    if let Status::Dead(_) = game.status {
        draw_rectangle(
            BOARD_X,
            BOARD_Y,
            BOARD_W,
            BOARD_H,
            with_alpha(theme.danger, 0.10),
        );
        outline_rounded(
            Rect::new(BOARD_X - 4.0, BOARD_Y - 4.0, BOARD_W + 8.0, BOARD_H + 8.0),
            4.0,
            3.0,
            with_alpha(theme.danger, 0.7),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_tapers_monotonically_from_head_to_tail() {
        let n = 12;
        let mut prev = f32::MAX;
        for i in 0..n {
            let w = segment_width(i, n);
            assert!(w <= prev + 1e-6 && w > 0.3 && w <= 0.86 + 1e-6);
            prev = w;
        }
        assert!(segment_width(0, 1) > 0.8);
    }

    #[test]
    fn board_fits_the_stage_below_the_hud() {
        const {
            assert!(BOARD_X + BOARD_W <= 1280.0 && BOARD_Y + BOARD_H <= 720.0);
            assert!(BOARD_Y >= 100.0);
        }
        assert_eq!(cell_pos(0.0, 0.0), vec2(BOARD_X, BOARD_Y));
    }

    #[test]
    fn music_stage_rises_with_speed() {
        let mut g = SnakeGame::new(SnakeMode::Classic, 1);
        assert_eq!(music_stage(&g), 0);
        g.apples_eaten = 200;
        assert_eq!(music_stage(&g), 4);
        let mut prev = 0;
        for a in 0..80 {
            g.apples_eaten = a;
            let s = music_stage(&g);
            assert!(s >= prev && s < 5);
            prev = s;
        }
    }
}
