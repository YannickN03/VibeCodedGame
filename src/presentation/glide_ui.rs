//! Drawing of VibeCoded Glide: neon glider, layered pipes, Vibe rings,
//! parallax skyline, contrail, HUD panels and the menu preview.
//!
//! The 960x640 arena sits in the middle of the 1280x720 stage at
//! [`ARENA_X`], [`ARENA_Y`] with a 140-unit HUD panel on each side.

use macroquad::prelude::*;

use crate::core::glide::course::{LIP_OVERHANG, PIPE_W, RING_HALF_H};
use crate::core::glide::game::{
    ARENA_H, ARENA_W, CEIL_Y, DASH_COOLDOWN, FLOOR_Y, GLIDER_R, GLIDER_X, GlideGame, Pipe, Status,
    pitch_angle,
};

use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::{Theme, darken, lighten, mix, with_alpha};
use super::widgets::{FlowMeterBar, Orientation, RoundedPanel, StatBox};

pub const ARENA_X: f32 = 160.0;
pub const ARENA_Y: f32 = 40.0;

/// Arena position -> stage position.
pub fn to_stage(x: f32, y: f32) -> Vec2 {
    vec2(ARENA_X + x, ARENA_Y + y)
}

/// Music-intensity stage `0..5` from the scroll speed.
pub fn music_stage(g: &GlideGame) -> usize {
    (((g.speed() - 220.0) / 120.0) * 5.0)
        .floor()
        .clamp(0.0, 4.0) as usize
}

/// One point of the contrail: how old it is and where the glider was.
pub type TrailPoint = (f32, f32);

pub struct UiCtx<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub colorblind: bool,
    pub time: f32,
    pub pulse: f32,
    pub best: Option<u64>,
    pub mode_label: &'a str,
}

fn hash(n: i64) -> u32 {
    (n as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(23) as u32
}

/// Hue `0..1` to an opaque colour.
pub fn rainbow(h: f32) -> Color {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = h.fract();
    let (r, g, b) = match h as u32 {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    Color::new(r, g, b, 1.0)
}

fn rot(p: (f32, f32), a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

/// Diagonal hatch lines clipped to `r` (colour-blind contrast pattern).
fn hatch(r: Rect, spacing: f32, thickness: f32, col: Color) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let mut k = -r.h;
    while k < r.w {
        // Line x = r.x + k + t, y = r.y + t for t in 0..h, clipped to the box.
        let t0 = (-k).max(0.0);
        let t1 = (r.w - k).min(r.h);
        if t1 > t0 {
            draw_line(
                r.x + k + t0,
                r.y + t0,
                r.x + k + t1,
                r.y + t1,
                thickness,
                col,
            );
        }
        k += spacing;
    }
}

/// The glider: 4 layers, nose tilted by `pitch`, pulsing wing tips.
pub fn draw_glider(c: Vec2, pitch: f32, s: f32, theme: &Theme, time: f32, overdrive: bool) {
    let body = theme.accent2;
    let dark = darken(body, 0.5);
    let light = lighten(body, 0.6);
    let pulse = 0.5 + 0.5 * (time * 9.0).sin();
    let pt = |x: f32, y: f32| c + rot((x * s, y * s), pitch);
    // Glow.
    for (r, a) in [(26.0, 0.025), (19.0, 0.035), (13.0, 0.05)] {
        draw_circle(c.x, c.y, r * s, with_alpha(body, a + 0.015 * pulse));
    }
    // Wings (rear, darker).
    let (w1, w2) = (pt(-6.0, -4.0), pt(-6.0, 4.0));
    draw_triangle(w1, pt(-20.0, -17.0), pt(2.0, -2.0), dark);
    draw_triangle(w2, pt(-20.0, 17.0), pt(2.0, 2.0), dark);
    // Hull.
    draw_triangle(pt(24.0, 0.0), pt(-16.0, -9.0), pt(-16.0, 9.0), dark);
    draw_triangle(pt(22.0, 0.0), pt(-12.0, -7.0), pt(-12.0, 7.0), body);
    draw_triangle(pt(18.0, 0.0), pt(-4.0, -3.5), pt(-4.0, 3.5), light);
    let cp = pt(4.0, 0.0);
    draw_circle(cp.x, cp.y, 3.0 * s, with_alpha(BLACK, 0.55));
    // Pulsing wing tips.
    let tip = if overdrive {
        rainbow(time * 0.8)
    } else {
        theme.accent
    };
    for sy in [-1.0f32, 1.0] {
        let p = pt(-20.0, 17.0 * sy);
        draw_circle(p.x, p.y, (2.4 + 1.6 * pulse) * s, tip);
        draw_circle(p.x, p.y, (5.0 + 3.0 * pulse) * s, with_alpha(tip, 0.25));
    }
    // Engine.
    let e = pt(-17.0, 0.0);
    draw_circle(e.x, e.y, 3.5 * s, with_alpha(theme.tiers[3], 0.9));
}

fn draw_pipe(p: &Pipe, t: f32, kinetic: bool, theme: &Theme, colorblind: bool, time: f32) {
    let rects = p.rects(t, kinetic);
    let col = theme.pieces[3];
    let dark = darken(col, 0.55);
    let mid = darken(col, 0.2);
    let hi = lighten(col, 0.45);
    for (i, r) in rects.iter().enumerate() {
        if r.h <= 0.0 {
            continue;
        }
        let is_lip = i == 1 || i == 2;
        // Clip horizontally to the arena so pipes never spill over the HUD.
        let (cx0, cx1) = (r.x.max(0.0), r.right().min(ARENA_W));
        if cx1 <= cx0 {
            continue;
        }
        let p0 = to_stage(cx0, r.y);
        let rr = Rect::new(p0.x, p0.y, cx1 - cx0, r.h);
        // Layer 1: shadow. Layer 2: dark body. Layer 3: lit body.
        draw_rectangle(
            rr.x + 4.0,
            rr.y + 4.0,
            rr.w,
            rr.h,
            Color::new(0.0, 0.0, 0.0, 0.25),
        );
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, dark);
        draw_rectangle(rr.x + 3.0, rr.y + 3.0, rr.w - 6.0, rr.h - 6.0, mid);
        // Layer 4: highlight stripe and glowing lip edge.
        draw_rectangle(
            rr.x + rr.w * 0.18,
            rr.y + 3.0,
            rr.w * 0.14,
            rr.h - 6.0,
            with_alpha(hi, 0.45),
        );
        if is_lip {
            // The edge facing the gap glows.
            let ey = if i == 1 {
                rr.y + rr.h - 2.0
            } else {
                rr.y + 1.0
            };
            let g = 0.55 + 0.25 * (time * 5.0 + p.spec.index as f32).sin();
            draw_rectangle(
                rr.x,
                ey - 3.0,
                rr.w,
                6.0,
                with_alpha(theme.accent2, 0.18 * g),
            );
            draw_rectangle(
                rr.x,
                ey - 1.0,
                rr.w,
                2.0,
                with_alpha(lighten(theme.accent2, 0.4), g),
            );
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 2.0, with_alpha(hi, 0.7));
        }
        if colorblind {
            // Top pipes get "/" hatching, bottom pipes "\" style (dots) so
            // they stay distinguishable without colour.
            if i < 2 {
                hatch(rr, 12.0, 2.0, with_alpha(WHITE, 0.35));
            } else {
                let mut y = rr.y + 6.0;
                while y < rr.bottom() - 3.0 {
                    let mut x = rr.x + 6.0;
                    while x < rr.right() - 3.0 {
                        draw_circle(x, y, 1.6, with_alpha(WHITE, 0.4));
                        x += 12.0;
                    }
                    y += 12.0;
                }
            }
        }
    }
}

fn draw_ring(c: Vec2, theme: &Theme, colorblind: bool, time: f32) {
    let pulse = 0.5 + 0.5 * (time * 6.0).sin();
    let col = theme.pieces[1];
    draw_ellipse(
        c.x,
        c.y,
        16.0,
        RING_HALF_H + 6.0,
        0.0,
        with_alpha(col, 0.10 + 0.06 * pulse),
    );
    if colorblind {
        // Dashed outline: a clear pattern instead of a colour cue.
        let n = 18;
        for k in 0..n {
            if k % 2 == 0 {
                let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
                let a1 = (k as f32 + 1.0) / n as f32 * std::f32::consts::TAU;
                draw_line(
                    c.x + 9.0 * a0.cos(),
                    c.y + RING_HALF_H * a0.sin(),
                    c.x + 9.0 * a1.cos(),
                    c.y + RING_HALF_H * a1.sin(),
                    4.0,
                    col,
                );
            }
        }
    } else {
        draw_ellipse_lines(c.x, c.y, 9.0, RING_HALF_H, 0.0, 4.0, col);
    }
    draw_ellipse_lines(c.x, c.y, 9.0, RING_HALF_H, 0.0, 1.5, lighten(col, 0.6));
}

fn draw_skyline(distance: f32, theme: &Theme) {
    let layers = [(0.06, 0.35, 110.0), (0.12, 0.5, 80.0), (0.22, 0.7, 60.0)];
    for (li, (factor, alpha, max_h)) in layers.iter().enumerate() {
        let cell = 70.0;
        let shift = distance * factor;
        let first = (shift / cell).floor() as i64;
        for i in 0..16i64 {
            let idx = first + i;
            let h = hash(idx * 7 + li as i64 * 1000);
            let x = i as f32 * cell - shift.rem_euclid(cell);
            let bh = 30.0 + (h % 100) as f32 / 100.0 * max_h;
            let w = cell - 6.0;
            let col = mix(theme.bg_light, theme.accent, 0.12 * (li as f32 + 1.0));
            // Clip to the arena so nothing spills over the HUD panels.
            let (cx0, cx1) = (x.max(0.0), (x + w).min(ARENA_W));
            if cx1 <= cx0 {
                continue;
            }
            let p = to_stage(cx0, FLOOR_Y - bh);
            draw_rectangle(p.x, p.y, cx1 - cx0, bh, with_alpha(col, *alpha));
            if li == 2 && x >= 0.0 && x + w <= ARENA_W {
                // Lit windows.
                for wy in 0..(bh / 14.0) as i32 {
                    if (h >> (wy % 20)) & 1 == 1 {
                        draw_rectangle(
                            p.x + 8.0 + (wy % 3) as f32 * 16.0,
                            p.y + 6.0 + wy as f32 * 14.0,
                            5.0,
                            5.0,
                            with_alpha(theme.accent2, 0.35),
                        );
                    }
                }
            }
        }
    }
}

/// Draws the arena and everything in it.
pub fn draw_arena(g: &GlideGame, ctx: &UiCtx, trail: &[TrailPoint]) {
    let th = ctx.theme;
    let tcol = th.tiers[g.tier().index()];
    let (ax, ay) = (ARENA_X, ARENA_Y);
    let kinetic = g.kinetic();
    let t = g.world_time();
    // Glow frame.
    let glow = 0.12 + 0.28 * ctx.pulse * (0.3 + g.flow.flow / 100.0);
    for i in 1..=4 {
        let k = i as f32 * 3.0;
        draw_rectangle_lines(
            ax - k,
            ay - k,
            ARENA_W + 2.0 * k,
            ARENA_H + 2.0 * k,
            2.0,
            with_alpha(tcol, glow / i as f32),
        );
    }
    // Sky: stacked bands from dark (top) to lighter (horizon).
    let bands = 8;
    for b in 0..bands {
        let k = b as f32 / (bands - 1) as f32;
        draw_rectangle(
            ax,
            ay + b as f32 * (ARENA_H / bands as f32),
            ARENA_W,
            ARENA_H / bands as f32 + 1.0,
            mix(th.bg_dark, th.bg_light, k * 0.9),
        );
    }
    draw_skyline(g.distance, th);
    // Floor and ceiling strips with a scrolling grid.
    let fp = to_stage(0.0, FLOOR_Y);
    draw_rectangle(fp.x, fp.y, ARENA_W, ARENA_H - FLOOR_Y, darken(th.well, 0.1));
    draw_line(fp.x, fp.y, fp.x + ARENA_W, fp.y, 3.0, with_alpha(tcol, 0.8));
    let off = g.distance.rem_euclid(48.0);
    let mut x = -off;
    while x < ARENA_W {
        // Slanted grid lines, clipped to the arena.
        if x >= 30.0 {
            draw_line(
                fp.x + x,
                fp.y,
                fp.x + x - 30.0,
                fp.y + (ARENA_H - FLOOR_Y),
                1.0,
                with_alpha(tcol, 0.25),
            );
        }
        x += 48.0;
    }
    let cp = to_stage(0.0, 0.0);
    draw_rectangle(cp.x, cp.y, ARENA_W, CEIL_Y, darken(th.well, 0.1));
    draw_line(
        cp.x,
        cp.y + CEIL_Y,
        cp.x + ARENA_W,
        cp.y + CEIL_Y,
        3.0,
        with_alpha(tcol, 0.8),
    );

    // Gust streaks.
    if let Some(left) = g.gust {
        let a = (left.min(0.3) / 0.3).clamp(0.0, 1.0);
        for i in 0..14 {
            let h = hash(i);
            let x = (h % 960) as f32;
            let y = ((h >> 10) % 560) as f32 + 30.0;
            let p = to_stage(x, (y + ctx.time * 300.0) % 560.0 + 30.0);
            draw_line(
                p.x,
                p.y,
                p.x - 6.0,
                p.y + 28.0,
                2.0,
                with_alpha(th.accent2, 0.35 * a),
            );
        }
    }

    // Pipes and rings.
    for p in &g.pipes {
        if p.x > ARENA_W + 20.0 || p.x + PIPE_W + LIP_OVERHANG < -10.0 {
            continue;
        }
        draw_pipe(p, t, kinetic, th, ctx.colorblind, ctx.time);
        if !p.ring_resolved
            && let Some(ry) = p.ring_y(t, kinetic)
        {
            draw_ring(to_stage(p.center_x(), ry), th, ctx.colorblind, ctx.time);
        }
    }

    // Contrail (rainbow in Overdrive).
    let speed = g.speed();
    for (i, (age, y)) in trail.iter().enumerate() {
        let k = 1.0 - (age / 0.9).clamp(0.0, 1.0);
        let p = to_stage(GLIDER_X - age * speed * 0.9 - 14.0, *y);
        let col = if g.overdrive() {
            rainbow(i as f32 * 0.06 + ctx.time * 0.5)
        } else {
            th.accent2
        };
        draw_circle(p.x, p.y, 2.0 + 6.0 * k, with_alpha(col, 0.45 * k));
    }

    // Glider.
    let gc = to_stage(GLIDER_X, g.y);
    if g.iframes > 0.0 {
        // Phase shield: ghost images and a ring.
        for k in 1..4 {
            let gp = gc - vec2(k as f32 * 16.0, 0.0);
            draw_glider(gp, g.pitch(), 1.0, th, ctx.time, false);
            draw_circle(gp.x, gp.y, 20.0, with_alpha(th.bg_dark, 0.45));
        }
        draw_circle_lines(gc.x, gc.y, 26.0, 2.5, with_alpha(th.accent, 0.8));
    }
    if g.is_playing() {
        draw_glider(gc, g.pitch(), 1.0, th, ctx.time, g.overdrive());
    } else {
        draw_circle(gc.x, gc.y, GLIDER_R + 4.0, with_alpha(th.danger, 0.6));
    }
    if !g.started {
        text::label(
            ctx.stage,
            "FLAP TO START",
            ax + ARENA_W * 0.5,
            ay + 140.0,
            Size::X2,
            with_alpha(th.text, 0.6 + 0.4 * ctx.pulse),
            Align::Center,
        );
        text::label(
            ctx.stage,
            "SPACE / W / UP / CLICK",
            ax + ARENA_W * 0.5,
            ay + 180.0,
            Size::X1,
            th.text_dim,
            Align::Center,
        );
    }
    if let Status::Dead(_) = g.status {
        draw_rectangle(ax, ay, ARENA_W, ARENA_H, with_alpha(th.danger, 0.10));
    }
    // Overdrive aura on the whole arena frame.
    if g.overdrive() {
        draw_rectangle_lines(
            ax,
            ay,
            ARENA_W,
            ARENA_H,
            3.0,
            with_alpha(rainbow(ctx.time * 0.4), 0.7),
        );
    }
}

/// Both HUD panels.
pub fn draw_hud(g: &GlideGame, ctx: &UiCtx) {
    let (th, st) = (ctx.theme, ctx.stage);
    let left = Rect::new(10.0, ARENA_Y, 140.0, ARENA_H);
    let right = Rect::new(ARENA_X + ARENA_W + 10.0, ARENA_Y, 140.0, ARENA_H);
    RoundedPanel::new(left, th.accent2).draw(th);
    RoundedPanel::new(right, th.accent).draw(th);

    let lx = left.x + 14.0;
    text::label(
        st,
        "FLOW",
        lx,
        left.y + 14.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let tier = g.tier();
    text::label(
        st,
        &format!("{}x", tier.multiplier()),
        lx + 70.0,
        left.y + 14.0,
        Size::X1,
        th.tiers[tier.index()],
        Align::Left,
    );
    FlowMeterBar {
        rect: Rect::new(left.x + 57.0, left.y + 44.0, 26.0, 230.0),
        orientation: Orientation::Vertical,
        flow: g.flow.flow,
        pulse: ctx.pulse,
    }
    .draw(st, th);
    StatBox {
        label: "SPEED",
        value: &format!("{:.0}", g.speed()),
        pos: vec2(lx, left.y + 330.0),
        width: 112.0,
        digit_h: 22.0,
        color: Some(th.accent2),
    }
    .draw(st, th);
    text::label(
        st,
        "DASH",
        lx,
        left.y + 410.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let ready = g.dash_ready();
    let fill = if ready {
        1.0
    } else {
        1.0 - g.dash_cd / DASH_COOLDOWN
    };
    draw_rectangle(lx, left.y + 434.0, 112.0, 10.0, with_alpha(BLACK, 0.4));
    draw_rectangle(
        lx,
        left.y + 434.0,
        112.0 * fill,
        10.0,
        if ready {
            mix(th.accent2, WHITE, 0.3 * ctx.pulse)
        } else {
            th.text_dim
        },
    );
    text::label(
        st,
        if ready { "READY" } else { "..." },
        lx,
        left.y + 452.0,
        Size::X1,
        if ready { th.accent2 } else { th.text_dim },
        Align::Left,
    );
    text::label(
        st,
        ctx.mode_label,
        lx,
        left.y + 590.0,
        Size::X1,
        th.accent,
        Align::Left,
    );

    let rx = right.x + 14.0;
    StatBox {
        label: "SCORE",
        value: &g.score.to_string(),
        pos: vec2(rx, right.y + 14.0),
        width: 112.0,
        digit_h: 20.0,
        color: None,
    }
    .draw(st, th);
    let best = ctx
        .best
        .map(|b| b.to_string())
        .unwrap_or_else(|| "-".into());
    StatBox {
        label: "HIGHSCORE",
        value: &best,
        pos: vec2(rx, right.y + 82.0),
        width: 112.0,
        digit_h: 16.0,
        color: Some(th.text_dim),
    }
    .draw(st, th);
    StatBox {
        label: "PIPES",
        value: &g.passed.to_string(),
        pos: vec2(rx, right.y + 146.0),
        width: 112.0,
        digit_h: 20.0,
        color: None,
    }
    .draw(st, th);
    StatBox {
        label: "RING COMBO",
        value: &format!("x{}", g.combo),
        pos: vec2(rx, right.y + 214.0),
        width: 112.0,
        digit_h: 20.0,
        color: Some(if g.combo >= 3 { th.pieces[1] } else { th.text }),
    }
    .draw(st, th);
    let legend = [
        ("SPACE", "FLAP"),
        ("DOWN", "DIVE"),
        ("SHIFT", "DASH"),
        ("P", "PAUSE"),
    ];
    let mut y = right.y + 500.0;
    for (k, a) in legend {
        text::label(st, k, rx, y, Size::X1, th.text, Align::Left);
        text::label(
            st,
            a,
            right.right() - 14.0,
            y,
            Size::X1,
            th.text_dim,
            Align::Right,
        );
        y += 22.0;
    }
}

/// Animated mini flight for the menu card (a pure function of `time`).
pub fn draw_preview(rect: Rect, time: f32, theme: &Theme) {
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        mix(theme.bg_dark, theme.bg_light, 0.5),
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.0,
        with_alpha(theme.accent2, 0.4),
    );
    let s = (rect.h / 190.0).clamp(0.4, 1.0);
    // Skyline.
    let shift = time * 14.0;
    for i in 0..14i64 {
        let idx = (shift / 34.0).floor() as i64 + i;
        let h = hash(idx);
        let bh = (14.0 + (h % 50) as f32) * s;
        let x = rect.x + i as f32 * 34.0 - shift.rem_euclid(34.0);
        if x + 30.0 < rect.right() && x > rect.x - 30.0 {
            let w = 30.0f32.min(rect.right() - x);
            draw_rectangle(
                x.max(rect.x),
                rect.bottom() - 8.0 * s - bh,
                w,
                bh,
                with_alpha(theme.accent, 0.18),
            );
        }
    }
    draw_line(
        rect.x,
        rect.bottom() - 8.0 * s,
        rect.right(),
        rect.bottom() - 8.0 * s,
        2.0,
        with_alpha(theme.accent2, 0.7),
    );
    // Pipes scroll left; the glider follows their gaps.
    let speed = 90.0 * s.max(0.6);
    let spacing = 120.0 * s.max(0.6) + 40.0;
    let gx = rect.x + rect.w * 0.3;
    let gap_h = 62.0 * s.max(0.7);
    let gap_of = |k: i64| rect.y + rect.h * (0.35 + 0.3 * ((hash(k + 99) % 100) as f32 / 100.0));
    let travel = time * speed;
    let first = (travel / spacing).floor() as i64 - 1;
    for k in first..first + 6 {
        let px = rect.x + rect.w * 0.3 + k as f32 * spacing - travel + spacing;
        if px > rect.right() || px < rect.x - 30.0 {
            continue;
        }
        let gy = gap_of(k);
        let w = 22.0 * s.max(0.7);
        let col = theme.pieces[3];
        let clip_x = px.max(rect.x);
        let clip_w = (px + w).min(rect.right()) - clip_x;
        if clip_w > 0.0 {
            draw_rectangle(
                clip_x,
                rect.y + 1.0,
                clip_w,
                gy - gap_h * 0.5 - rect.y - 1.0,
                darken(col, 0.3),
            );
            draw_rectangle(
                clip_x,
                gy + gap_h * 0.5,
                clip_w,
                rect.bottom() - 8.0 * s - gy - gap_h * 0.5,
                darken(col, 0.3),
            );
            draw_rectangle(
                clip_x,
                gy - gap_h * 0.5 - 4.0,
                clip_w,
                4.0,
                lighten(col, 0.4),
            );
            draw_rectangle(clip_x, gy + gap_h * 0.5, clip_w, 4.0, lighten(col, 0.4));
        }
        if k % 2 == 1 && px + w * 0.5 < rect.right() && px + w * 0.5 > rect.x {
            draw_ellipse_lines(
                px + w * 0.5,
                gy,
                4.0 * s.max(0.7),
                11.0 * s.max(0.7),
                0.0,
                2.0,
                theme.pieces[1],
            );
        }
    }
    // Glider path: smooth interpolation through the gap centres.
    let pos_y = |tt: f32| -> f32 {
        let tr = tt * speed;
        let f = (tr / spacing).floor();
        let u = tr / spacing - f;
        let a = gap_of(f as i64);
        let b = gap_of(f as i64 + 1);
        let e = u * u * (3.0 - 2.0 * u);
        a + (b - a) * e
    };
    for k in 0..16 {
        let tt = time - k as f32 * 0.045;
        let y = pos_y(tt);
        let col = rainbow(time * 0.3 - k as f32 * 0.04);
        draw_circle(
            gx - k as f32 * 4.5 * s.max(0.7) - 8.0,
            y,
            (4.0 - k as f32 * 0.22) * s.max(0.7),
            with_alpha(col, 0.5 * (1.0 - k as f32 / 16.0)),
        );
    }
    let dy = pos_y(time + 0.05) - pos_y(time - 0.05);
    draw_glider(
        vec2(gx, pos_y(time)),
        pitch_angle(dy * 5.0),
        0.7 * s.max(0.7),
        theme,
        time,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::glide::game::{GlideGame, GlideMode};

    #[test]
    fn the_arena_is_centred_between_two_140_wide_panels() {
        const {
            assert!(ARENA_X + ARENA_W <= 1280.0);
            assert!(ARENA_Y + ARENA_H <= 720.0);
        }
        assert!(
            (ARENA_X - (1280.0 - ARENA_X - ARENA_W)).abs() < 1.0,
            "centred"
        );
        assert_eq!(to_stage(0.0, 0.0), vec2(ARENA_X, ARENA_Y));
        assert_eq!(crate::core::glide::course::LIP_H, 26.0);
    }

    #[test]
    fn music_stage_follows_the_speed() {
        let mut g = GlideGame::new(GlideMode::Classic, 1);
        assert_eq!(music_stage(&g), 0);
        let mut prev = 0;
        for n in 0..200 {
            g.passed = n;
            let s = music_stage(&g);
            assert!(s >= prev && s < 5);
            prev = s;
        }
        assert_eq!(prev, 4);
    }

    #[test]
    fn rainbow_covers_the_hue_circle_with_opaque_colours() {
        for i in 0..24 {
            let c = rainbow(i as f32 / 24.0);
            assert_eq!(c.a, 1.0);
            assert!(
                (c.r.max(c.g).max(c.b) - 1.0).abs() < 1e-5,
                "fully saturated"
            );
        }
        assert_eq!(rainbow(0.0), rainbow(1.0));
    }
}
