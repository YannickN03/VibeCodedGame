//! In-game 3-column HUD: hold + stats + flow meter | board | next + score.

use macroquad::prelude::*;

use crate::core::board::{Cell, HEIGHT, VISIBLE_TOP, WIDTH};
use crate::core::flow::FLOW_MAX;
use crate::core::game::LOCK_DELAY;
use crate::core::{FlowTier, GameMode, GameState};

use super::blocks;
use super::digits;
use super::fx::Fx;
use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::{Theme, lighten, mix, with_alpha};

/// Cell edge length in stage units.
pub const CELL: f32 = 32.0;
/// Board origin (top-left of the first visible row).
pub const BOARD_X: f32 = 480.0;
pub const BOARD_Y: f32 = 48.0;
pub const BOARD_W: f32 = CELL * WIDTH as f32;
pub const BOARD_H: f32 = CELL * (HEIGHT - VISIBLE_TOP) as f32;
/// Danger-zone warning line row.
pub const DANGER_ROW: usize = 23;

/// Everything the HUD needs besides the game.
pub struct HudCtx<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub colorblind: bool,
    pub pulse: f32,
    pub best: Option<u64>,
    pub daily_label: &'a str,
}

/// Screen position of matrix cell `(x, y)` (top-left).
pub fn cell_pos(x: i32, y: i32, bounce: f32) -> Vec2 {
    vec2(
        BOARD_X + x as f32 * CELL,
        BOARD_Y + (y - VISIBLE_TOP as i32) as f32 * CELL + bounce,
    )
}

/// Centre of matrix cell `(x, y)`.
pub fn cell_center(x: i32, y: i32) -> Vec2 {
    cell_pos(x, y, 0.0) + vec2(CELL * 0.5, CELL * 0.5)
}

pub fn panel(x: f32, y: f32, w: f32, h: f32, theme: &Theme, accent: Color) {
    draw_rectangle(x + 4.0, y + 6.0, w, h, Color::new(0.0, 0.0, 0.0, 0.28));
    draw_rectangle(x, y, w, h, theme.panel);
    draw_rectangle_lines(x, y, w, h, 1.5, theme.panel_border);
    draw_rectangle(x, y, w, 2.0, with_alpha(accent, 0.85));
}

fn tier_color(theme: &Theme, tier: FlowTier) -> Color {
    theme.tiers[tier.index()]
}

/// Draws the complete in-game HUD.
pub fn draw(game: &GameState, ctx: &HudCtx, fx: &Fx) {
    let tier = game.tier();
    let tcol = tier_color(ctx.theme, tier);
    draw_board(game, ctx, fx, tcol);
    draw_left(game, ctx, tcol);
    draw_right(game, ctx, tcol);
}

fn draw_board(game: &GameState, ctx: &HudCtx, fx: &Fx, tcol: Color) {
    let th = ctx.theme;
    let bounce = fx.spring.offset;
    let (bx, by) = (BOARD_X, BOARD_Y + bounce);

    // Outer glow frame, breathing with the beat and flow tier.
    let glow = 0.18 + 0.32 * ctx.pulse * (0.3 + game.flow.flow / FLOW_MAX);
    for i in 1..=4 {
        let g = i as f32 * 2.5;
        draw_rectangle_lines(
            bx - g,
            by - g,
            BOARD_W + 2.0 * g,
            BOARD_H + 2.0 * g,
            2.0,
            with_alpha(tcol, glow / i as f32),
        );
    }
    draw_rectangle(bx, by, BOARD_W, BOARD_H, th.well);

    // Helper grid
    let gc = with_alpha(th.grid, 0.07);
    for x in 1..WIDTH {
        let lx = bx + x as f32 * CELL;
        draw_line(lx, by, lx, by + BOARD_H, 1.0, gc);
    }
    for y in 1..(HEIGHT - VISIBLE_TOP) {
        let ly = by + y as f32 * CELL;
        draw_line(bx, ly, bx + BOARD_W, ly, 1.0, gc);
    }

    // Danger zone
    let danger_y = by + (DANGER_ROW - VISIBLE_TOP) as f32 * CELL;
    let stack_top = game.board.highest_occupied_row().unwrap_or(HEIGHT);
    let in_danger = stack_top <= DANGER_ROW + 2;
    let da = if in_danger {
        0.45 + 0.45 * ctx.pulse
    } else {
        0.28
    };
    if in_danger {
        draw_rectangle(
            bx,
            by,
            BOARD_W,
            danger_y - by,
            with_alpha(th.danger, 0.06 + 0.08 * ctx.pulse),
        );
    }
    let mut dx = bx;
    while dx < bx + BOARD_W {
        draw_line(
            dx,
            danger_y,
            (dx + 10.0).min(bx + BOARD_W),
            danger_y,
            2.0,
            with_alpha(th.danger, da),
        );
        dx += 16.0;
    }

    // Locked blocks (row 19 peeks out above the board).
    for y in (VISIBLE_TOP - 1)..HEIGHT {
        for x in 0..WIDTH {
            if let Cell::Filled(kind) = game.board[y][x] {
                let p = cell_pos(x as i32, y as i32, bounce);
                let mut c = th.piece(kind);
                if y < VISIBLE_TOP {
                    c = with_alpha(c, 0.55);
                }
                blocks::mino(p.x, p.y, CELL, c, Some(kind), ctx.colorblind);
            }
        }
    }

    fx.draw_board_layer();

    if game.is_playing() {
        // Ghost
        let ghost = game.ghost();
        let gcol = th.piece(ghost.kind);
        if ghost.y != game.active.y {
            for (x, y) in ghost.minos() {
                if y >= VISIBLE_TOP as i32 {
                    let p = cell_pos(x, y, bounce);
                    blocks::ghost(p.x, p.y, CELL, gcol);
                }
            }
        }
        // Active piece, brightening as the lock timer runs out.
        let lock_frac = if game.is_grounded() {
            (game.lock_timer / LOCK_DELAY).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let acol = lighten(th.piece(game.active.kind), 0.08 + 0.3 * lock_frac);
        for (x, y) in game.active.minos() {
            if y >= VISIBLE_TOP as i32 - 1 {
                let p = cell_pos(x, y, bounce);
                let c = if y < VISIBLE_TOP as i32 {
                    with_alpha(acol, 0.6)
                } else {
                    acol
                };
                blocks::mino(p.x, p.y, CELL, c, Some(game.active.kind), ctx.colorblind);
            }
        }
    }

    draw_rectangle_lines(
        bx,
        by,
        BOARD_W,
        BOARD_H,
        2.0,
        with_alpha(lighten(th.panel_border, 0.2), 0.9),
    );
}

fn stat(ctx: &HudCtx, label: &str, value: &str, x: f32, y: f32, col: Color) {
    text::label(
        ctx.stage,
        label,
        x,
        y,
        Size::X1,
        ctx.theme.text_dim,
        Align::Left,
    );
    digits::draw(value, x, y + 22.0, 30.0, col);
}

fn draw_left(game: &GameState, ctx: &HudCtx, tcol: Color) {
    let th = ctx.theme;
    // HOLD
    panel(250.0, 48.0, 200.0, 140.0, th, th.accent);
    text::label(
        ctx.stage,
        "HOLD",
        266.0,
        60.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    if let Some(k) = game.hold {
        let c = if game.can_hold {
            th.piece(k)
        } else {
            mix(th.piece(k), Color::new(0.35, 0.37, 0.42, 1.0), 0.75)
        };
        blocks::preview(k, 350.0, 128.0, 26.0, c, ctx.colorblind);
    }

    // Stats
    panel(250.0, 204.0, 150.0, 484.0, th, th.accent);
    let x = 266.0;
    stat(ctx, "LEVEL", &game.level.to_string(), x, 218.0, th.text);
    let lines_label = match game.mode.line_goal() {
        Some(g) => format!("LINES / {g}"),
        None => "LINES".to_string(),
    };
    stat(
        ctx,
        &lines_label,
        &game.lines.to_string(),
        x,
        296.0,
        th.text,
    );
    let combo = game.score.combo.max(0);
    stat(
        ctx,
        "COMBO",
        &format!("x{combo}"),
        x,
        374.0,
        if combo > 0 { th.accent2 } else { th.text },
    );
    let b2b = game.score.b2b_active;
    stat(
        ctx,
        "B2B",
        &game.score.b2b_streak.to_string(),
        x,
        452.0,
        if b2b { th.accent2 } else { th.text },
    );
    if b2b {
        let a = 0.5 + 0.5 * ctx.pulse;
        text::label(
            ctx.stage,
            "READY",
            384.0,
            452.0,
            Size::X1,
            with_alpha(th.accent2, a),
            Align::Right,
        );
    }
    // Vibe tier
    let tier = game.tier();
    text::label(
        ctx.stage,
        "VIBE",
        x,
        530.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    text::label(
        ctx.stage,
        tier.name(),
        x,
        552.0,
        Size::X1,
        tcol,
        Align::Left,
    );
    let mult = match tier {
        FlowTier::Chill => "1.0x",
        FlowTier::Groove => "1.25x",
        FlowTier::Sync => "1.5x",
        FlowTier::Overdrive => "2.0x",
    };
    digits::draw(mult, x, 580.0, 34.0, tcol);
    if game.flow.overdrive_shield > 0.0 {
        let a = 0.6 + 0.4 * ctx.pulse;
        text::label(
            ctx.stage,
            "SHIELD",
            x,
            630.0,
            Size::X1,
            with_alpha(th.accent2, a),
            Align::Left,
        );
        let w = 116.0 * game.flow.overdrive_shield / crate::core::flow::OVERDRIVE_SHIELD_SECS;
        draw_rectangle(x, 652.0, w, 4.0, with_alpha(th.accent2, a));
    }

    // Flow meter
    panel(410.0, 204.0, 40.0, 484.0, th, tcol);
    let (mx, my, mw, mh) = (419.0, 236.0, 22.0, 440.0);
    text::label(
        ctx.stage,
        "FLOW",
        430.0,
        212.0,
        Size::X1,
        th.text_dim,
        Align::Center,
    );
    draw_rectangle(mx, my, mw, mh, Color::new(0.0, 0.0, 0.0, 0.45));
    let frac = game.flow.flow / FLOW_MAX;
    let fh = mh * frac;
    // gradient fill from tier colours
    let segs = 24;
    for i in 0..segs {
        let t0 = i as f32 / segs as f32;
        if t0 >= frac {
            break;
        }
        let t1 = ((i + 1) as f32 / segs as f32).min(frac);
        let c = th.tiers[FlowTier::from_flow(t0 * FLOW_MAX).index()];
        let y1 = my + mh - t1 * mh;
        draw_rectangle(mx, y1, mw, (t1 - t0) * mh + 0.5, with_alpha(c, 0.9));
    }
    if fh > 0.0 {
        let gy = my + mh - fh;
        draw_rectangle(
            mx - 3.0,
            gy - 2.0,
            mw + 6.0,
            4.0,
            with_alpha(lighten(tcol, 0.5), 0.6 + 0.4 * ctx.pulse),
        );
        // glow halo
        draw_rectangle(
            mx - 5.0,
            gy,
            mw + 10.0,
            fh,
            with_alpha(tcol, 0.08 + 0.1 * ctx.pulse),
        );
    }
    for m in [25.0, 60.0, 90.0] {
        let y = my + mh - m / FLOW_MAX * mh;
        draw_line(
            mx - 4.0,
            y,
            mx + mw + 4.0,
            y,
            1.5,
            with_alpha(th.text, 0.45),
        );
    }
    draw_rectangle_lines(mx, my, mw, mh, 1.0, with_alpha(th.text_dim, 0.5));
}

fn draw_right(game: &GameState, ctx: &HudCtx, tcol: Color) {
    let th = ctx.theme;
    let x = 830.0;
    // NEXT queue (4)
    panel(x, 48.0, 200.0, 312.0, th, th.accent);
    text::label(
        ctx.stage,
        "NEXT",
        x + 16.0,
        60.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    for (i, k) in game.next_queue(4).into_iter().enumerate() {
        let (cy, s) = if i == 0 {
            (122.0, 28.0)
        } else {
            (122.0 + 30.0 + i as f32 * 62.0, 20.0)
        };
        let mut c = th.piece(k);
        if i > 0 {
            c = with_alpha(c, 0.85);
        }
        blocks::preview(k, x + 100.0, cy, s, c, ctx.colorblind);
    }

    // Score block
    panel(x, 376.0, 200.0, 312.0, th, tcol);
    let lx = x + 16.0;
    text::label(
        ctx.stage,
        game.mode.title(),
        lx,
        386.0,
        Size::X1,
        th.accent2,
        Align::Left,
    );
    text::label(
        ctx.stage,
        "SCORE",
        lx,
        412.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    digits::draw(&game.score.score.to_string(), lx, 432.0, 32.0, th.text);

    let best_txt = match (game.mode, ctx.best) {
        (GameMode::Sprint, Some(ms)) => digits::format_time(ms as f64 / 1000.0),
        (GameMode::Sprint, None) => "-:--.--".to_string(),
        (_, Some(s)) => s.to_string(),
        (_, None) => "0".to_string(),
    };
    let best_label = if game.mode == GameMode::Sprint {
        "BEST TIME"
    } else {
        "HIGHSCORE"
    };
    text::label(
        ctx.stage,
        best_label,
        lx,
        478.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    digits::draw(&best_txt, lx, 498.0, 22.0, th.text_dim);

    if game.mode == GameMode::DailyVibe {
        text::label(
            ctx.stage,
            "DAILY",
            lx,
            532.0,
            Size::X1,
            th.text_dim,
            Align::Left,
        );
        digits::draw(ctx.daily_label, lx, 552.0, 20.0, th.accent2);
    } else {
        text::label(
            ctx.stage,
            "TIME",
            lx,
            532.0,
            Size::X1,
            th.text_dim,
            Align::Left,
        );
        digits::draw(&digits::format_time(game.elapsed), lx, 552.0, 22.0, th.text);
    }

    let legend = [
        ("T", "THEME"),
        ("B", "COLORBLIND"),
        ("M", "MUTE"),
        ("P", "PAUSE"),
    ];
    for (i, (k, v)) in legend.iter().enumerate() {
        let y = 592.0 + i as f32 * 22.0;
        draw_rectangle_lines(lx, y - 2.0, 20.0, 20.0, 1.0, with_alpha(th.text_dim, 0.7));
        text::label(ctx.stage, k, lx + 10.0, y, Size::X1, th.text, Align::Center);
        text::label(
            ctx.stage,
            v,
            lx + 30.0,
            y,
            Size::X1,
            th.text_dim,
            Align::Left,
        );
    }
}
