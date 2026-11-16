//! Drawing of VibeCoded Armada: arena, layered vector ships and enemies,
//! boss, bunkers, projectiles, HUD panels and the menu preview.
//!
//! Arena coordinates (`900 x 640`) are placed in the middle of the
//! 1280x720 stage at [`ARENA_X`], [`ARENA_Y`] with a HUD panel on each side.

use macroquad::prelude::*;

use crate::core::armada::game::{
    ARENA_H, ARENA_W, ArmadaGame, Boss, DEFENSE_Y, Laser, SHIP_H, SHIP_W, SHIP_Y, Status,
    TELEGRAPH_SECS, UFO_Y,
};
use crate::core::armada::waves::{EnemyKind, PowerKind, is_boss_wave};

use super::digits;
use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::{Theme, darken, lighten, mix, with_alpha};
use super::widgets::shapes::{fill_rounded, outline_rounded};
use super::widgets::{FlowMeterBar, Orientation, RoundedPanel, StatBox};

pub const ARENA_X: f32 = 190.0;
pub const ARENA_Y: f32 = 40.0;

/// Arena position -> stage position.
pub fn to_stage(x: f32, y: f32) -> Vec2 {
    vec2(ARENA_X + x, ARENA_Y + y)
}

/// Music-intensity stage `0..5` from the wave: the Dreadnought is the peak.
pub fn music_stage(g: &ArmadaGame) -> usize {
    if is_boss_wave(g.wave) {
        4
    } else {
        (g.wave.saturating_sub(1) as usize).min(3)
    }
}

/// Everything the HUD needs besides the game.
pub struct UiCtx<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub colorblind: bool,
    pub time: f32,
    pub pulse: f32,
    pub best: Option<u64>,
    pub mode_label: &'a str,
}

fn enemy_color(theme: &Theme, kind: EnemyKind) -> Color {
    match kind {
        EnemyKind::Scout => theme.pieces[3],
        EnemyKind::Interceptor => theme.pieces[4],
        EnemyKind::Aegis => theme.pieces[5],
        EnemyKind::Weaver => theme.pieces[2],
    }
}

pub fn power_color(theme: &Theme, kind: PowerKind) -> Color {
    match kind {
        PowerKind::TriSpread => theme.pieces[0],
        PowerKind::RailBeam => theme.pieces[1],
        PowerKind::Overclock => theme.pieces[6],
        PowerKind::AegisRepair => theme.pieces[3],
        PowerKind::VibeSurge => theme.pieces[2],
    }
}

/// One enemy as a layered vector silhouette. `s` scales the whole drawing
/// (`1.0` = in-game size). In colour-blind mode every archetype also gets an
/// unmistakable inner symbol.
#[allow(clippy::too_many_arguments)]
pub fn draw_enemy(
    kind: EnemyKind,
    c: Vec2,
    hp: u8,
    s: f32,
    theme: &Theme,
    colorblind: bool,
    time: f32,
) {
    let col = enemy_color(theme, kind);
    let dark = darken(col, 0.45);
    let light = lighten(col, 0.55);
    let (w, h) = kind.size();
    let (w, h) = (w * s, h * s);
    // Soft glow + shadow.
    draw_circle(c.x, c.y, w * 0.75, with_alpha(col, 0.10));
    match kind {
        EnemyKind::Scout => {
            // Downward-pointing dart.
            draw_triangle(
                c + vec2(-w * 0.5, -h * 0.5),
                c + vec2(w * 0.5, -h * 0.5),
                c + vec2(0.0, h * 0.55),
                dark,
            );
            draw_triangle(
                c + vec2(-w * 0.38, -h * 0.4),
                c + vec2(w * 0.38, -h * 0.4),
                c + vec2(0.0, h * 0.4),
                col,
            );
            draw_circle(c.x, c.y - h * 0.1, w * 0.12, light);
            if colorblind {
                draw_circle(c.x, c.y - h * 0.1, w * 0.06, BLACK);
            }
        }
        EnemyKind::Interceptor => {
            // Diamond with twin cannons.
            draw_poly(c.x, c.y, 4, w * 0.62, 0.0, dark);
            draw_poly(c.x, c.y, 4, w * 0.5, 0.0, col);
            draw_rectangle(c.x - w * 0.34, c.y + h * 0.1, w * 0.1, h * 0.5, light);
            draw_rectangle(c.x + w * 0.24, c.y + h * 0.1, w * 0.1, h * 0.5, light);
            draw_circle(c.x, c.y, w * 0.1, light);
            if colorblind {
                let k = w * 0.14;
                draw_line(c.x - k, c.y - k, c.x + k, c.y + k, 2.5, BLACK);
                draw_line(c.x - k, c.y + k, c.x + k, c.y - k, 2.5, BLACK);
            }
        }
        EnemyKind::Aegis => {
            // Armoured hexagon; the shield ring cracks after the first hit.
            draw_poly(c.x, c.y, 6, w * 0.55, 0.0, dark);
            draw_poly(c.x, c.y, 6, w * 0.44, 0.0, col);
            draw_poly(c.x, c.y, 6, w * 0.22, 0.0, light);
            if hp >= 2 {
                draw_poly_lines(c.x, c.y, 6, w * 0.62, 0.0, 2.5, with_alpha(light, 0.9));
            } else {
                // Cracked: broken ring segments and fracture lines.
                draw_poly_lines(c.x, c.y, 6, w * 0.62, 0.0, 1.5, with_alpha(light, 0.35));
                draw_line(
                    c.x - w * 0.3,
                    c.y - h * 0.3,
                    c.x + w * 0.05,
                    c.y + h * 0.05,
                    2.0,
                    BLACK,
                );
                draw_line(
                    c.x + w * 0.05,
                    c.y + h * 0.05,
                    c.x + w * 0.3,
                    c.y - h * 0.1,
                    2.0,
                    BLACK,
                );
                draw_line(
                    c.x + w * 0.05,
                    c.y + h * 0.05,
                    c.x - w * 0.05,
                    c.y + h * 0.4,
                    2.0,
                    BLACK,
                );
            }
            if colorblind {
                draw_circle_lines(c.x, c.y, w * 0.14, 2.5, BLACK);
            }
        }
        EnemyKind::Weaver => {
            // Two phase-shifted discs.
            let ph = (time * 3.0 + c.x * 0.05).sin();
            draw_ellipse(
                c.x - w * 0.12 * ph,
                c.y,
                w * 0.5,
                h * 0.42,
                0.0,
                with_alpha(dark, 0.9),
            );
            draw_ellipse(
                c.x + w * 0.12 * ph,
                c.y,
                w * 0.5,
                h * 0.42,
                0.0,
                with_alpha(col, 0.85),
            );
            draw_circle(c.x, c.y, w * 0.14, light);
            if hp < 2 {
                draw_line(c.x - w * 0.3, c.y, c.x + w * 0.3, c.y, 2.0, BLACK);
            }
            if colorblind {
                let mut prev = c + vec2(-w * 0.3, 0.0);
                for i in 1..=6 {
                    let t = i as f32 / 6.0;
                    let p = c + vec2(-w * 0.3 + w * 0.6 * t, (t * 12.0).sin() * h * 0.14);
                    draw_line(prev.x, prev.y, p.x, p.y, 2.0, BLACK);
                    prev = p;
                }
            }
        }
    }
}

pub fn draw_ufo(c: Vec2, s: f32, theme: &Theme, colorblind: bool, time: f32) {
    let col = theme.pieces[1];
    draw_circle(c.x, c.y, 34.0 * s, with_alpha(col, 0.12));
    draw_ellipse(
        c.x,
        c.y + 2.0 * s,
        22.0 * s,
        8.0 * s,
        0.0,
        darken(col, 0.45),
    );
    draw_ellipse(c.x, c.y, 22.0 * s, 7.0 * s, 0.0, col);
    draw_ellipse(
        c.x,
        c.y - 5.0 * s,
        10.0 * s,
        7.0 * s,
        0.0,
        lighten(col, 0.5),
    );
    for i in -2..=2 {
        let on = ((time * 6.0) as i32 + i).rem_euclid(2) == 0;
        draw_circle(
            c.x + i as f32 * 8.0 * s,
            c.y + 1.0 * s,
            1.8 * s,
            if on { WHITE } else { darken(col, 0.6) },
        );
    }
    if colorblind {
        draw_line(
            c.x - 14.0 * s,
            c.y + 6.0 * s,
            c.x + 14.0 * s,
            c.y + 6.0 * s,
            2.0,
            BLACK,
        );
    }
}

/// The player's ship (layered). `overdrive` adds wing blasters and the
/// chromatic neon trail.
pub fn draw_ship(c: Vec2, s: f32, theme: &Theme, overdrive: bool, thrust: f32, alpha: f32) {
    let body = with_alpha(theme.accent2, alpha);
    let dark = with_alpha(darken(theme.accent2, 0.5), alpha);
    let light = with_alpha(lighten(theme.accent2, 0.6), alpha);
    if overdrive {
        // Chromatic trail: three offset, tinted copies of the exhaust.
        for (i, tint) in [theme.tiers[3], theme.tiers[1], theme.tiers[2]]
            .iter()
            .enumerate()
        {
            let k = i as f32 - 1.0;
            for j in 0..6 {
                let t = j as f32 / 6.0;
                draw_circle(
                    c.x + k * 6.0 * s,
                    c.y + 10.0 * s + t * 34.0 * s,
                    (4.0 - 3.0 * t) * s,
                    with_alpha(*tint, 0.5 * (1.0 - t) * alpha),
                );
            }
        }
    }
    // Engine flame.
    let flame = 8.0 + 6.0 * thrust.abs().min(1.0) + 2.0 * (c.x * 0.3).sin().abs();
    draw_triangle(
        c + vec2(-5.0 * s, 8.0 * s),
        c + vec2(5.0 * s, 8.0 * s),
        c + vec2(0.0, (8.0 + flame) * s),
        with_alpha(theme.tiers[3], 0.85 * alpha),
    );
    // Wings.
    draw_triangle(
        c + vec2(-17.0 * s, 9.0 * s),
        c + vec2(-4.0 * s, -6.0 * s),
        c + vec2(-2.0 * s, 9.0 * s),
        dark,
    );
    draw_triangle(
        c + vec2(17.0 * s, 9.0 * s),
        c + vec2(4.0 * s, -6.0 * s),
        c + vec2(2.0 * s, 9.0 * s),
        dark,
    );
    // Hull.
    draw_triangle(
        c + vec2(-9.0 * s, 9.0 * s),
        c + vec2(9.0 * s, 9.0 * s),
        c + vec2(0.0, -12.0 * s),
        body,
    );
    draw_triangle(
        c + vec2(-4.0 * s, 6.0 * s),
        c + vec2(4.0 * s, 6.0 * s),
        c + vec2(0.0, -8.0 * s),
        light,
    );
    draw_circle(c.x, c.y - 1.0 * s, 2.6 * s, with_alpha(BLACK, 0.55 * alpha));
    if overdrive {
        for sx in [-1.0f32, 1.0] {
            draw_rectangle(
                c.x + sx * 16.0 * s - 1.5 * s,
                c.y - 10.0 * s,
                3.0 * s,
                12.0 * s,
                light,
            );
        }
    }
}

fn draw_capsule(c: Vec2, kind: PowerKind, theme: &Theme, stage: &Stage, time: f32) {
    let col = power_color(theme, kind);
    let r = Rect::new(c.x - 13.0, c.y - 10.0, 26.0, 20.0);
    draw_circle(
        c.x,
        c.y,
        22.0,
        with_alpha(col, 0.12 + 0.06 * (time * 6.0).sin().abs()),
    );
    fill_rounded(r, 8.0, darken(col, 0.4));
    fill_rounded(
        Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0),
        6.0,
        col,
    );
    text::label(
        stage,
        kind.symbol(),
        c.x,
        c.y - 7.0,
        Size::X1,
        BLACK,
        Align::Center,
    );
}

fn draw_bunkers(g: &ArmadaGame, theme: &Theme) {
    let col = theme.pieces[3];
    for b in &g.bunkers {
        for r in 0..crate::core::armada::bunker::ROWS {
            for c in 0..crate::core::armada::bunker::COLS {
                if !b.cells[r][c] {
                    continue;
                }
                let cb = b.cell_box(r, c);
                let p = to_stage(cb.x, cb.y);
                // Cells get darker towards the bottom; a bevel highlight on top.
                let shade = darken(col, 0.12 * r as f32);
                draw_rectangle(p.x, p.y, cb.w, cb.h, darken(shade, 0.35));
                draw_rectangle(p.x + 0.5, p.y + 0.5, cb.w - 1.0, cb.h - 1.0, shade);
                draw_rectangle(
                    p.x + 0.5,
                    p.y + 0.5,
                    cb.w - 1.0,
                    1.5,
                    with_alpha(WHITE, 0.3),
                );
            }
        }
    }
}

fn draw_boss(b: &Boss, theme: &Theme, colorblind: bool, time: f32) {
    let col = theme.pieces[6];
    let body = b.body();
    let p = to_stage(body.x, body.y);
    let r = Rect::new(p.x, p.y, body.w, body.h);
    fill_rounded(
        Rect::new(r.x + 4.0, r.y + 6.0, r.w, r.h),
        14.0,
        Color::new(0.0, 0.0, 0.0, 0.3),
    );
    fill_rounded(r, 14.0, darken(col, 0.55));
    fill_rounded(
        Rect::new(r.x + 5.0, r.y + 5.0, r.w - 10.0, r.h - 10.0),
        11.0,
        darken(col, 0.25),
    );
    fill_rounded(
        Rect::new(r.x + 12.0, r.y + 10.0, r.w - 24.0, 10.0),
        5.0,
        with_alpha(lighten(col, 0.5), 0.35),
    );
    outline_rounded(r, 14.0, 2.0, col);
    // Hull plating lines.
    for i in 1..4 {
        let x = r.x + r.w * i as f32 / 4.0;
        draw_line(
            x,
            r.y + 8.0,
            x,
            r.y + r.h - 8.0,
            1.0,
            with_alpha(BLACK, 0.3),
        );
    }
    // Turrets.
    for i in 0..2 {
        let tb = b.turret_box(i);
        let tp = to_stage(tb.x, tb.y);
        let alive = b.turrets[i].alive;
        let tc = if alive {
            lighten(col, 0.1)
        } else {
            Color::new(0.2, 0.2, 0.22, 1.0)
        };
        fill_rounded(Rect::new(tp.x, tp.y, tb.w, tb.h), 6.0, darken(tc, 0.4));
        fill_rounded(
            Rect::new(tp.x + 2.0, tp.y + 2.0, tb.w - 4.0, tb.h - 4.0),
            4.0,
            tc,
        );
        if alive {
            draw_rectangle(
                tp.x + tb.w * 0.5 - 2.0,
                tp.y + tb.h - 2.0,
                4.0,
                10.0,
                lighten(col, 0.4),
            );
            let frac = b.turrets[i].hp as f32 / 6.0;
            draw_rectangle(tp.x, tp.y - 6.0, tb.w * frac, 3.0, theme.accent2);
        } else if colorblind {
            draw_line(
                tp.x + 4.0,
                tp.y + 4.0,
                tp.x + tb.w - 4.0,
                tp.y + tb.h - 4.0,
                2.0,
                BLACK,
            );
        }
    }
    // Core with its health ring.
    let cb = b.core();
    let cc = to_stage(cb.center().0, cb.center().1);
    let frac = b.core_hp as f32 / b.core_max as f32;
    let pulse = 0.5 + 0.5 * (time * 4.0).sin();
    draw_circle(
        cc.x,
        cc.y,
        30.0,
        with_alpha(theme.danger, 0.12 + 0.1 * pulse),
    );
    draw_circle(cc.x, cc.y, 19.0, darken(theme.danger, 0.4));
    draw_circle(
        cc.x,
        cc.y,
        15.0 * (0.55 + 0.45 * frac),
        mix(theme.danger, WHITE, 0.3 + 0.3 * pulse),
    );
    if colorblind {
        draw_circle_lines(cc.x, cc.y, 22.0, 2.0, BLACK);
    }
    // Health bar above.
    let bw = 160.0;
    draw_rectangle(cc.x - bw * 0.5, r.y - 14.0, bw, 6.0, with_alpha(BLACK, 0.5));
    draw_rectangle(cc.x - bw * 0.5, r.y - 14.0, bw * frac, 6.0, theme.danger);
    // Laser: telegraph line, then the sweeping beam.
    match b.laser {
        Laser::Telegraph { t, x } => {
            let k = 1.0 - t / TELEGRAPH_SECS;
            let blink = if (time * 14.0).sin() > 0.0 { 1.0 } else { 0.55 };
            let top = r.y + r.h;
            let sx = ARENA_X + x;
            draw_line(
                sx,
                top,
                sx,
                ARENA_Y + ARENA_H,
                1.5,
                with_alpha(theme.danger, (0.3 + 0.6 * k) * blink),
            );
            draw_circle(
                sx,
                top + 6.0,
                4.0 + 6.0 * k,
                with_alpha(theme.danger, 0.5 * blink),
            );
        }
        Laser::Fire { x, .. } => {
            if let Some(beam) = b.beam() {
                let bp = to_stage(beam.x, beam.y);
                draw_rectangle(
                    bp.x - 8.0,
                    bp.y,
                    beam.w + 16.0,
                    beam.h,
                    with_alpha(theme.danger, 0.18),
                );
                draw_rectangle(bp.x, bp.y, beam.w, beam.h, with_alpha(theme.danger, 0.65));
                draw_rectangle(
                    bp.x + beam.w * 0.3,
                    bp.y,
                    beam.w * 0.4,
                    beam.h,
                    with_alpha(WHITE, 0.9),
                );
            }
            let _ = x;
        }
        Laser::Idle(_) => {}
    }
}

/// Draws the arena and its contents.
pub fn draw_arena(g: &ArmadaGame, ctx: &UiCtx) {
    let th = ctx.theme;
    let tcol = th.tiers[g.tier().index()];
    let (ax, ay) = (ARENA_X, ARENA_Y);
    // Frame glow breathing with the beat.
    let glow = 0.12 + 0.3 * ctx.pulse * (0.3 + g.flow.flow / 100.0);
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
    draw_rectangle(ax, ay, ARENA_W, ARENA_H, th.well);
    // Parallax star field (pure function of time).
    for i in 0..70u32 {
        let h = i.wrapping_mul(2654435761);
        let sx = (h % 900) as f32;
        let speed = 10.0 + (h >> 8) as f32 % 30.0;
        let sy = ((h >> 16) as f32 % 640.0 + ctx.time * speed) % ARENA_H;
        let a = 0.15 + 0.25 * ((h >> 4) % 4) as f32 / 4.0;
        draw_rectangle(ax + sx, ay + sy, 1.5, 1.5, with_alpha(WHITE, a));
    }
    // Defense line.
    let mut x = 0.0;
    while x < ARENA_W {
        draw_line(
            ax + x,
            ay + DEFENSE_Y,
            ax + (x + 10.0).min(ARENA_W),
            ay + DEFENSE_Y,
            1.5,
            with_alpha(th.danger, 0.4),
        );
        x += 20.0;
    }
    draw_bunkers(g, th);

    for e in g.enemies.iter().filter(|e| e.alive) {
        let (x, y) = g.enemy_pos(e);
        draw_enemy(
            e.kind,
            to_stage(x, y),
            e.hp,
            1.0,
            th,
            ctx.colorblind,
            ctx.time,
        );
        if e.drop.is_some() {
            // Carriers pulse softly so players can plan for the capsule.
            let p = to_stage(x, y);
            draw_circle_lines(
                p.x,
                p.y,
                22.0 + 2.0 * (ctx.time * 5.0).sin(),
                1.0,
                with_alpha(WHITE, 0.35),
            );
        }
    }
    if let Some(u) = &g.ufo {
        draw_ufo(to_stage(u.x, UFO_Y), 1.0, th, ctx.colorblind, ctx.time);
    }
    if let Some(b) = &g.boss {
        draw_boss(b, th, ctx.colorblind, ctx.time);
    }
    for c in &g.capsules {
        draw_capsule(to_stage(c.x, c.y), c.kind, th, ctx.stage, ctx.time);
    }
    // Bolts with light trails.
    for b in &g.player_bolts {
        let p = to_stage(b.x, b.y);
        for t in 1..5 {
            let k = t as f32 / 5.0;
            draw_rectangle(
                p.x - 1.5,
                p.y + 7.0 + 14.0 * k,
                3.0,
                8.0,
                with_alpha(th.accent2, 0.4 * (1.0 - k)),
            );
        }
        draw_rectangle(
            p.x - 3.0,
            p.y - 8.0,
            6.0,
            16.0,
            with_alpha(th.accent2, 0.25),
        );
        draw_rectangle(p.x - 1.5, p.y - 7.0, 3.0, 14.0, WHITE);
    }
    for b in &g.enemy_bolts {
        let p = to_stage(b.x, b.y);
        for t in 1..5 {
            let k = t as f32 / 5.0;
            draw_rectangle(
                p.x - 2.0,
                p.y - 6.0 - 14.0 * k,
                4.0,
                8.0,
                with_alpha(th.danger, 0.4 * (1.0 - k)),
            );
        }
        draw_poly(p.x, p.y, 4, b.w * 0.8, 0.0, with_alpha(th.danger, 0.3));
        draw_rectangle(p.x - b.w * 0.5, p.y - b.h * 0.5, b.w, b.h, th.danger);
        draw_rectangle(
            p.x - 1.0,
            p.y - b.h * 0.5,
            2.0,
            b.h,
            lighten(th.danger, 0.7),
        );
    }
    // Rail beam flash.
    if g.rail_flash > 0.0 {
        let a = g.rail_flash / 0.12;
        let sx = ax + g.rail_x;
        draw_rectangle(
            sx - 7.0,
            ay,
            14.0,
            SHIP_Y - 10.0,
            with_alpha(th.pieces[1], 0.25 * a),
        );
        draw_rectangle(sx - 2.0, ay, 4.0, SHIP_Y - 10.0, with_alpha(WHITE, 0.9 * a));
    }
    // Ship (blinks while invulnerable).
    if g.is_playing() {
        let c = to_stage(g.ship.x, SHIP_Y);
        let blink = if g.ship.shield > 0.0 && ((ctx.time * 16.0) as i32) % 2 == 0 {
            0.45
        } else {
            1.0
        };
        let alpha = if g.ship.iframes > 0.0 { 0.55 } else { blink };
        let thrust = if g.ship.dash_t > 0.0 {
            g.ship.dash_dir
        } else {
            0.0
        };
        draw_ship(c, 1.0, th, g.overdrive(), thrust, alpha);
        if g.ship.shield > 0.0 {
            let a = (g.ship.shield / 1.5).min(1.0);
            draw_circle_lines(c.x, c.y, 24.0, 2.0, with_alpha(th.accent2, 0.35 + 0.4 * a));
        }
    } else {
        let c = to_stage(g.ship.x, SHIP_Y);
        draw_circle(c.x, c.y, 18.0, with_alpha(th.danger, 0.5));
    }
    let _ = (SHIP_W, SHIP_H);
    if let Status::Dead(_) = g.status {
        draw_rectangle(ax, ay, ARENA_W, ARENA_H, with_alpha(th.danger, 0.10));
    }
}

fn power_line(g: &ArmadaGame) -> (String, f32, PowerKind) {
    if let Some((k, t)) = g.weapon {
        return (k.name().to_string(), t / k.duration(), k);
    }
    if g.overclock > 0.0 {
        let k = PowerKind::Overclock;
        return (k.name().to_string(), g.overclock / k.duration(), k);
    }
    ("NONE".to_string(), 0.0, PowerKind::TriSpread)
}

/// Both HUD panels.
pub fn draw_hud(g: &ArmadaGame, ctx: &UiCtx) {
    let (th, st) = (ctx.theme, ctx.stage);
    let left = Rect::new(10.0, ARENA_Y, 170.0, ARENA_H);
    let right = Rect::new(ARENA_X + ARENA_W + 10.0, ARENA_Y, 170.0, ARENA_H);
    RoundedPanel::new(left, th.accent2).draw(th);
    RoundedPanel::new(right, th.accent).draw(th);

    // ---- left: flow, wave, power-up, dash
    let lx = left.x + 16.0;
    text::label(
        st,
        "FLOW",
        lx,
        left.y + 16.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    FlowMeterBar {
        rect: Rect::new(lx + 44.0, left.y + 44.0, 26.0, 250.0),
        orientation: Orientation::Vertical,
        flow: g.flow.flow,
        pulse: ctx.pulse,
    }
    .draw(st, th);
    let tier = g.tier();
    text::label(
        st,
        &format!("{}x", tier.multiplier()),
        lx + 84.0,
        left.y + 44.0,
        Size::X2,
        th.tiers[tier.index()],
        Align::Left,
    );
    StatBox {
        label: "WAVE",
        value: &g.wave.to_string(),
        pos: vec2(lx, left.y + 340.0),
        width: 136.0,
        digit_h: 30.0,
        color: None,
    }
    .draw(st, th);
    let (name, frac, kind) = power_line(g);
    text::label(
        st,
        "POWERUP",
        lx,
        left.y + 420.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let pc = if frac > 0.0 {
        power_color(th, kind)
    } else {
        th.text_dim
    };
    text::label(st, &name, lx, left.y + 442.0, Size::X1, pc, Align::Left);
    draw_rectangle(lx, left.y + 466.0, 136.0, 8.0, with_alpha(BLACK, 0.4));
    draw_rectangle(lx, left.y + 466.0, 136.0 * frac, 8.0, pc);
    text::label(
        st,
        "DASH",
        lx,
        left.y + 500.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let cd = g.mode.dash_cooldown();
    let ready = g.dash_ready();
    let fill = if ready {
        1.0
    } else {
        1.0 - g.ship.dash_cd / cd
    };
    draw_rectangle(lx, left.y + 524.0, 136.0, 10.0, with_alpha(BLACK, 0.4));
    let dcol = if ready {
        mix(th.accent2, WHITE, 0.3 * ctx.pulse)
    } else {
        th.text_dim
    };
    draw_rectangle(lx, left.y + 524.0, 136.0 * fill, 10.0, dcol);
    text::label(
        st,
        if ready { "READY" } else { "..." },
        lx,
        left.y + 542.0,
        Size::X1,
        if ready { th.accent2 } else { th.text_dim },
        Align::Left,
    );
    text::label(
        st,
        ctx.mode_label,
        lx,
        left.y + 596.0,
        Size::X1,
        th.accent,
        Align::Left,
    );

    // ---- right: score, best, hull, combo, legend
    let rx = right.x + 16.0;
    StatBox {
        label: "SCORE",
        value: &g.score.to_string(),
        pos: vec2(rx, right.y + 16.0),
        width: 138.0,
        digit_h: 28.0,
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
        pos: vec2(rx, right.y + 94.0),
        width: 138.0,
        digit_h: 20.0,
        color: Some(th.text_dim),
    }
    .draw(st, th);
    text::label(
        st,
        "HULL",
        rx,
        right.y + 160.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    for i in 0..g.max_hull() {
        let c = vec2(rx + 14.0 + i as f32 * 28.0, right.y + 196.0);
        if i < g.hull {
            draw_ship(c, 0.62, th, false, 0.0, 1.0);
        } else {
            draw_circle_lines(c.x, c.y, 8.0, 1.5, with_alpha(th.text_dim, 0.5));
        }
    }
    StatBox {
        label: "COMBO",
        value: &format!("x{}", g.combo),
        pos: vec2(rx, right.y + 232.0),
        width: 138.0,
        digit_h: 24.0,
        color: Some(if g.combo >= 3 { th.accent2 } else { th.text }),
    }
    .draw(st, th);
    let legend = [
        ("A/D", "MOVE"),
        ("SPACE", "FIRE"),
        ("SHIFT", "DASH"),
        ("P", "PAUSE"),
    ];
    let mut y = right.y + 520.0;
    for (k, a) in legend {
        text::label(st, k, rx, y, Size::X1, th.text, Align::Left);
        text::label(
            st,
            a,
            right.right() - 16.0,
            y,
            Size::X1,
            th.text_dim,
            Align::Right,
        );
        y += 22.0;
    }
    let _ = digits::width("0", 1.0);
}

/// Animated mini space battle for the menu card (a pure function of `time`).
pub fn draw_preview(rect: Rect, time: f32, theme: &Theme) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, theme.well);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.0,
        with_alpha(theme.accent2, 0.4),
    );
    // Stars.
    for i in 0..30u32 {
        let h = i.wrapping_mul(2654435761);
        let x = rect.x + (h % 1000) as f32 / 1000.0 * rect.w;
        let y = rect.y + ((h >> 10) % 1000) as f32 / 1000.0 * rect.h;
        let y = rect.y + (y - rect.y + time * (8.0 + (h >> 20) as f32 % 14.0)) % rect.h;
        draw_rectangle(x, y, 1.5, 1.5, with_alpha(WHITE, 0.35));
    }
    let s = (rect.h / 190.0).clamp(0.45, 1.0);
    // Patrolling formation: 7 x 3 with all four archetypes.
    let sway = (time * 0.7).sin() * rect.w * 0.07;
    let kinds = [EnemyKind::Weaver, EnemyKind::Aegis, EnemyKind::Interceptor];
    let cols = 7;
    let step = (rect.w * 0.78) / cols as f32;
    let x0 = rect.x + rect.w * 0.11 + step * 0.5 + sway;
    for (row, kind) in kinds.iter().enumerate() {
        for col in 0..cols {
            let c = vec2(
                x0 + col as f32 * step,
                rect.y + 24.0 * s + row as f32 * 30.0 * s,
            );
            // Some enemies are "shot" in a loop so the formation thins out.
            let cycle = (time * 0.35 + col as f32 * 0.13 + row as f32 * 0.31).fract();
            if cycle > 0.9 {
                continue;
            }
            draw_enemy(*kind, c, 2, 0.62 * s, theme, false, time);
        }
    }
    // Bunkers.
    for i in 0..3 {
        let bx = rect.x + rect.w * (0.2 + 0.3 * i as f32);
        for r in 0..2 {
            for c in 0..4 {
                draw_rectangle(
                    bx + c as f32 * 6.0 - 12.0,
                    rect.y + rect.h * 0.72 + r as f32 * 6.0,
                    5.0,
                    5.0,
                    with_alpha(theme.pieces[3], 0.8),
                );
            }
        }
    }
    // The player dodges left and right and shoots.
    let px = rect.x + rect.w * 0.5 + (time * 1.1).sin() * rect.w * 0.34;
    let py = rect.bottom() - 18.0 * s - 6.0;
    for k in 0..3 {
        let phase = (time * 1.6 + k as f32 / 3.0).fract();
        let by = py - 10.0 - phase * (py - rect.y - 20.0);
        let bx = px + ((time * 1.6 + k as f32 / 3.0).floor() * 7.0).sin() * 0.0;
        draw_rectangle(
            bx - 1.0,
            by,
            2.0,
            9.0 * s.max(0.7),
            with_alpha(theme.accent2, 1.0 - phase * 0.5),
        );
    }
    // Falling enemy bolts.
    for k in 0..3 {
        let phase = (time * 0.8 + k as f32 * 0.37).fract();
        let bx = rect.x + rect.w * (0.18 + 0.32 * k as f32) + (time + k as f32).sin() * 6.0;
        let by = rect.y + rect.h * 0.3 + phase * rect.h * 0.6;
        draw_rectangle(bx - 1.5, by, 3.0, 8.0, theme.danger);
    }
    draw_ship(
        vec2(px, py),
        0.8 * s.max(0.7),
        theme,
        false,
        (time * 1.1).cos(),
        1.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::armada::game::ArmadaMode;

    #[test]
    fn arena_is_centred_between_the_hud_panels() {
        const {
            assert!(ARENA_X + ARENA_W + 190.0 <= 1280.0);
            assert!(ARENA_Y + ARENA_H <= 720.0);
        }
        let left_gap = ARENA_X;
        let right_gap = 1280.0 - (ARENA_X + ARENA_W);
        assert!((left_gap - right_gap).abs() < 1.0, "centred horizontally");
        assert_eq!(to_stage(0.0, 0.0), vec2(ARENA_X, ARENA_Y));
    }

    #[test]
    fn music_stage_follows_the_waves_and_peaks_at_the_boss() {
        let mut g = ArmadaGame::new(ArmadaMode::Classic, 1);
        let stages: Vec<usize> = (1..=6)
            .map(|w| {
                g.wave = w;
                music_stage(&g)
            })
            .collect();
        assert_eq!(stages, vec![0, 1, 2, 3, 4, 3]);
    }

    #[test]
    fn every_power_up_has_a_distinct_colour_and_symbol() {
        let th = &crate::presentation::theme::themes()[0];
        let mut cols = Vec::new();
        let mut syms = Vec::new();
        for k in PowerKind::ALL {
            let c = power_color(th, k);
            cols.push((c.r.to_bits(), c.g.to_bits(), c.b.to_bits()));
            syms.push(k.symbol());
        }
        cols.sort();
        cols.dedup();
        syms.sort();
        syms.dedup();
        assert_eq!((cols.len(), syms.len()), (5, 5));
    }

    #[test]
    fn dread_and_enemy_names_exist() {
        assert_eq!(EnemyKind::Aegis.name(), "AEGIS STRIKER");
    }
}
