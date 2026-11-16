//! Drawing of VibeCoded Maze: the 28x23 neon maze with double-contour walls,
//! the mouth-animated runner, four ghosts with waving hems and looking
//! pupils, frightened blinking, colour-blind forehead emblems, the HUD panels
//! and the menu preview.

use macroquad::prelude::*;

use crate::core::maze::game::{BLINK_SECS, MazeGame, MazeMode};
use crate::core::maze::ghost::{Dir, GhostKind, GhostMode};
use crate::core::maze::maze::{H, Maze, Tile, W};

use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::{Theme, hex, lighten, mix, with_alpha};
use super::widgets::{FlowMeterBar, Orientation, RoundedPanel, StatBox};

pub const TILE: f32 = 28.0;
pub const MAZE_W: f32 = W as f32 * TILE;
pub const MAZE_H: f32 = H as f32 * TILE;
pub const MX: f32 = (1280.0 - MAZE_W) * 0.5;
pub const MY: f32 = (720.0 - MAZE_H) * 0.5;

/// Music-intensity stage `0..5`: later stages and higher flow tiers.
pub fn music_stage(g: &MazeGame) -> usize {
    ((g.stage as usize - 1) + g.tier().index() / 2).min(4)
}

pub struct UiCtx<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub colorblind: bool,
    pub time: f32,
    pub pulse: f32,
    pub best: Option<u64>,
    pub mode_label: &'a str,
}

/// Screen position of the centre of the (fractional) tile position.
pub fn screen_pos(x: f32, y: f32) -> Vec2 {
    vec2(MX + x * TILE, MY + y * TILE)
}

pub fn runner_color() -> Color {
    hex(0xFFD84A)
}

pub fn ghost_color(theme: &Theme, kind: GhostKind) -> Color {
    match kind {
        GhostKind::Apex => theme.danger,
        GhostKind::Vektor => theme.pieces[5],
        GhostKind::Flank => theme.accent2,
        GhostKind::Drift => theme.pieces[2],
    }
}

fn dir_angle(d: Dir) -> f32 {
    match d {
        Dir::Right => 0.0,
        Dir::Down => std::f32::consts::FRAC_PI_2,
        Dir::Left => std::f32::consts::PI,
        Dir::Up => 3.0 * std::f32::consts::FRAC_PI_2,
    }
}

/// Calls `f` with `x` and, near the tunnel mouths, with the wrapped copy.
fn wrap_draw(x: f32, mut f: impl FnMut(f32)) {
    f(x);
    if x < 1.0 {
        f(x + W as f32);
    } else if x > W as f32 - 1.0 {
        f(x - W as f32);
    }
}

// ---------------------------------------------------------------- maze

/// Walls with a neon double contour (outer glow line + bright inner line).
pub fn draw_walls(maze: &Maze, ox: f32, oy: f32, tile: f32, theme: &Theme, glow: f32) {
    let wall = mix(theme.accent, theme.accent2, 0.35);
    let is_wall = |x: i32, y: i32| {
        if y < 0 || y >= H as i32 {
            return true;
        }
        if x < 0 || x >= W as i32 {
            return false;
        }
        maze.get(x, y) == Tile::Wall
    };
    let thick = (tile * 0.09).max(1.0);
    let inset = tile * 0.16;
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            if maze.get(x, y) != Tile::Wall {
                continue;
            }
            let (l, t) = (ox + x as f32 * tile, oy + y as f32 * tile);
            draw_rectangle(l, t, tile, tile, mix(theme.well, wall, 0.1));
            // Edges that face open space.
            let sides = [
                (!is_wall(x, y - 1), (l, t, l + tile, t), (0.0, 1.0)),
                (
                    !is_wall(x, y + 1),
                    (l, t + tile, l + tile, t + tile),
                    (0.0, -1.0),
                ),
                (!is_wall(x - 1, y), (l, t, l, t + tile), (1.0, 0.0)),
                (
                    !is_wall(x + 1, y),
                    (l + tile, t, l + tile, t + tile),
                    (-1.0, 0.0),
                ),
            ];
            for (open, (x0, y0, x1, y1), (nx, ny)) in sides {
                if !open {
                    continue;
                }
                draw_line(
                    x0,
                    y0,
                    x1,
                    y1,
                    thick * 3.0,
                    with_alpha(wall, 0.18 + 0.14 * glow),
                );
                draw_line(x0, y0, x1, y1, thick, with_alpha(lighten(wall, 0.2), 0.95));
                draw_line(
                    x0 + nx * inset,
                    y0 + ny * inset,
                    x1 + nx * inset,
                    y1 + ny * inset,
                    thick * 0.8,
                    with_alpha(wall, 0.55),
                );
            }
        }
    }
}

/// Dots, orbs and the gate; `skip` hides individual dots (menu preview).
pub fn draw_pellets(
    maze: &Maze,
    ox: f32,
    oy: f32,
    tile: f32,
    theme: &Theme,
    time: f32,
    skip: &dyn Fn(i32, i32) -> bool,
) {
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let c = vec2(ox + (x as f32 + 0.5) * tile, oy + (y as f32 + 0.5) * tile);
            match maze.get(x, y) {
                Tile::Dot if !skip(x, y) => {
                    draw_circle(c.x, c.y, tile * 0.1, theme.text);
                }
                Tile::PowerOrb if !skip(x, y) => {
                    let k = 0.75 + 0.25 * (time * 6.0).sin();
                    draw_circle(c.x, c.y, tile * 0.42 * k, with_alpha(theme.accent2, 0.22));
                    draw_circle(c.x, c.y, tile * 0.27 * k, theme.accent2);
                    draw_circle(c.x, c.y, tile * 0.12 * k, WHITE);
                }
                Tile::GhostGate => {
                    draw_rectangle(
                        c.x - tile * 0.5,
                        c.y - tile * 0.08,
                        tile,
                        tile * 0.16,
                        with_alpha(theme.pieces[5], 0.85),
                    );
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------- actors

/// The runner: a pie with an animated mouth facing `dir`.
pub fn draw_runner(c: Vec2, r: f32, dir: Dir, mouth: f32, color: Color) {
    draw_circle(c.x, c.y, r * 1.5, with_alpha(color, 0.12));
    let a0 = dir_angle(dir);
    let (from, to) = (a0 + mouth, a0 + std::f32::consts::TAU - mouth);
    let n = 28;
    for i in 0..n {
        let (s, e) = (
            from + (to - from) * i as f32 / n as f32,
            from + (to - from) * (i + 1) as f32 / n as f32,
        );
        draw_triangle(
            c,
            vec2(c.x + r * s.cos(), c.y + r * s.sin()),
            vec2(c.x + r * e.cos(), c.y + r * e.sin()),
            color,
        );
    }
    // The eye.
    let ea = a0 - 1.1;
    draw_circle(
        c.x + r * 0.35 * ea.cos() + r * 0.05,
        c.y + r * 0.35 * ea.sin() - r * 0.1,
        r * 0.1,
        BLACK,
    );
}

pub fn mouth_angle(time: f32, moving: bool) -> f32 {
    if moving {
        0.08 + 0.55 * (0.5 + 0.5 * (time * 18.0).sin())
    } else {
        0.2
    }
}

fn draw_emblem(kind: GhostKind, c: Vec2, s: f32, col: Color) {
    match kind {
        GhostKind::Apex => draw_triangle(
            vec2(c.x, c.y - s),
            vec2(c.x - s, c.y + s * 0.8),
            vec2(c.x + s, c.y + s * 0.8),
            col,
        ),
        GhostKind::Vektor => draw_rectangle(c.x - s * 0.8, c.y - s * 0.8, s * 1.6, s * 1.6, col),
        GhostKind::Flank => {
            draw_triangle(
                vec2(c.x, c.y - s),
                vec2(c.x - s, c.y),
                vec2(c.x + s, c.y),
                col,
            );
            draw_triangle(
                vec2(c.x, c.y + s),
                vec2(c.x - s, c.y),
                vec2(c.x + s, c.y),
                col,
            );
        }
        GhostKind::Drift => draw_circle(c.x, c.y, s * 0.9, col),
    }
}

/// A ghost; `look` is its movement direction for the pupils.
#[allow(clippy::too_many_arguments)]
pub fn draw_ghost(
    c: Vec2,
    r: f32,
    kind: GhostKind,
    mode: GhostMode,
    blink: bool,
    look: Option<Dir>,
    time: f32,
    theme: &Theme,
    colorblind: bool,
) {
    let (lx, ly) = look.map_or((0, 0), |d| d.vec());
    let eyes = |c: Vec2| {
        for s in [-1.0f32, 1.0] {
            let e = vec2(c.x + s * r * 0.36, c.y - r * 0.2);
            draw_circle(e.x, e.y, r * 0.3, WHITE);
            draw_circle(
                e.x + lx as f32 * r * 0.13,
                e.y + ly as f32 * r * 0.13,
                r * 0.15,
                hex(0x1B2A8A),
            );
        }
    };
    if mode == GhostMode::Eaten {
        eyes(c);
        return;
    }
    let frightened = mode == GhostMode::Frightened;
    let base = ghost_color(theme, kind);
    let body = if frightened {
        if blink && (time * 8.0) as i32 % 2 == 0 {
            hex(0xF4F6FF)
        } else {
            mix(theme.tiers[0], hex(0x2036D0), 0.5)
        }
    } else {
        base
    };
    draw_circle(c.x, c.y, r * 1.6, with_alpha(body, 0.13));
    let top = c.y - r * 0.15;
    draw_circle(c.x, top, r, body);
    draw_rectangle(c.x - r, top, r * 2.0, r * 0.95, body);
    // Waving hem: three teeth that alternate with the phase.
    let phase = ((time * 8.0) as i32 % 2) as f32;
    let bottom = top + r * 0.95;
    let w = r * 2.0 / 3.0;
    for i in 0..3 {
        let x0 = c.x - r + i as f32 * w;
        let tip = r * (0.30 + 0.12 * (((i as f32 + phase) % 2.0) - 0.5).abs() * 2.0);
        draw_triangle(
            vec2(x0, bottom - 1.0),
            vec2(x0 + w, bottom - 1.0),
            vec2(x0 + w * 0.5, bottom + tip),
            body,
        );
    }
    if frightened {
        let col = if blink && (time * 8.0) as i32 % 2 == 0 {
            hex(0xE0455C)
        } else {
            hex(0xFFE9B0)
        };
        for s in [-1.0f32, 1.0] {
            draw_circle(c.x + s * r * 0.32, c.y - r * 0.25, r * 0.13, col);
        }
        for i in 0..4 {
            let x0 = c.x - r * 0.6 + i as f32 * r * 0.4;
            let (y0, y1) = if i % 2 == 0 {
                (c.y + r * 0.25, c.y + r * 0.05)
            } else {
                (c.y + r * 0.05, c.y + r * 0.25)
            };
            draw_line(x0, y0, x0 + r * 0.4, y1, 1.6, col);
        }
    } else {
        eyes(c);
        if colorblind {
            draw_emblem(
                kind,
                vec2(c.x, c.y - r * 0.7),
                r * 0.3,
                with_alpha(hex(0x10131C), 0.9),
            );
        }
    }
}

fn draw_core(c: Vec2, timer: f32, theme: &Theme, time: f32) {
    let k = 1.0 + 0.12 * (time * 7.0).sin();
    let s = TILE * 0.38 * k;
    let col = theme.pieces[1];
    draw_circle(c.x, c.y, s * 1.9, with_alpha(col, 0.15));
    for (a, c2) in [(s, col), (s * 0.55, WHITE)] {
        draw_triangle(
            vec2(c.x, c.y - a),
            vec2(c.x - a, c.y),
            vec2(c.x + a, c.y),
            c2,
        );
        draw_triangle(
            vec2(c.x, c.y + a),
            vec2(c.x - a, c.y),
            vec2(c.x + a, c.y),
            c2,
        );
    }
    // Remaining time as a ring of segments.
    let frac = (timer / crate::core::maze::game::CORE_SECS).clamp(0.0, 1.0);
    let n = 24;
    for i in 0..(frac * n as f32).ceil() as i32 {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
        draw_circle(
            c.x + a.cos() * TILE * 0.78,
            c.y + a.sin() * TILE * 0.78,
            1.8,
            with_alpha(col, 0.9),
        );
    }
}

// ---------------------------------------------------------------- world

pub fn draw_world(g: &MazeGame, ctx: &UiCtx) {
    let th = ctx.theme;
    let tcol = th.tiers[g.tier().index()];
    let glow = ctx.pulse * (0.3 + g.flow.flow / 100.0);
    for i in 1..=3 {
        let k = i as f32 * 3.0;
        draw_rectangle_lines(
            MX - k,
            MY - k,
            MAZE_W + 2.0 * k,
            MAZE_H + 2.0 * k,
            2.0,
            with_alpha(tcol, (0.1 + 0.22 * glow) / i as f32),
        );
    }
    draw_rectangle(MX, MY, MAZE_W, MAZE_H, th.well);
    draw_walls(&g.maze, MX, MY, TILE, th, ctx.pulse);
    draw_pellets(&g.maze, MX, MY, TILE, th, ctx.time, &|_, _| false);
    if let Some(c) = g.core {
        let p = screen_pos(c.tile.0 as f32 + 0.5, c.tile.1 as f32 + 0.5);
        draw_core(p, c.timer, th, ctx.time);
    }
    // The runner (and the Overdrive magnet trail).
    if g.is_playing() {
        let (px, py, dir) = (g.player.x, g.player.y, g.player.dir);
        let facing = dir.unwrap_or(Dir::Left);
        let moving = dir.is_some();
        let flicker = g.zen_inv > 0.0 && (ctx.time * 14.0) as i32 % 2 == 0;
        let col = if flicker {
            with_alpha(runner_color(), 0.4)
        } else {
            runner_color()
        };
        if g.overdrive() {
            let (dx, dy) = facing.opposite().vec();
            for i in 1..=6 {
                let d = i as f32 * 0.35;
                let p = screen_pos(px + dx as f32 * d, py + dy as f32 * d);
                draw_circle(
                    p.x,
                    p.y,
                    TILE * 0.3 * (1.0 - i as f32 / 8.0),
                    with_alpha(th.tiers[3], 0.5 - i as f32 * 0.07),
                );
            }
            let p = screen_pos(px, py);
            draw_circle_lines(
                p.x,
                p.y,
                TILE * (1.3 + 0.2 * (ctx.time * 8.0).sin()),
                1.5,
                with_alpha(th.tiers[3], 0.6),
            );
        }
        wrap_draw(px, |x| {
            draw_runner(
                screen_pos(x, py),
                TILE * 0.42,
                facing,
                mouth_angle(ctx.time, moving),
                col,
            );
        });
    }
    let blink = g.fright_t > 0.0 && g.fright_t <= BLINK_SECS;
    for gh in &g.ghosts {
        let mode = g.mode_of(gh);
        let (gx, gy) = gh.pos();
        let look = gh.dir;
        wrap_draw(gx, |x| {
            draw_ghost(
                screen_pos(x, gy),
                TILE * 0.44,
                gh.kind,
                mode,
                blink,
                look,
                ctx.time,
                th,
                ctx.colorblind,
            );
        });
    }
    if g.ready > 0 && g.is_playing() {
        let p = screen_pos(14.0, 14.0);
        text::label(
            ctx.stage,
            "READY!",
            p.x,
            p.y - 6.0,
            Size::X2,
            runner_color(),
            Align::Center,
        );
    }
}

// ---------------------------------------------------------------- HUD

fn mmss(secs: f32) -> String {
    let s = secs.max(0.0).ceil() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Both HUD panels.
pub fn draw_hud(g: &MazeGame, ctx: &UiCtx) {
    let (th, st) = (ctx.theme, ctx.stage);
    let left = Rect::new(10.0, MY, MX - 22.0, MAZE_H);
    let right = Rect::new(MX + MAZE_W + 12.0, MY, MX - 22.0, MAZE_H);
    RoundedPanel::new(left, th.accent2).draw(th);
    RoundedPanel::new(right, th.accent).draw(th);

    let lx = left.x + 16.0;
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
        left.right() - 16.0,
        left.y + 14.0,
        Size::X1,
        th.tiers[tier.index()],
        Align::Right,
    );
    FlowMeterBar {
        rect: Rect::new(lx + 44.0, left.y + 44.0, 24.0, 190.0),
        orientation: Orientation::Vertical,
        flow: g.flow.flow,
        pulse: ctx.pulse,
    }
    .draw(st, th);
    StatBox {
        label: "STAGE",
        value: &g.stage.to_string(),
        pos: vec2(lx + 84.0, left.y + 52.0),
        width: 84.0,
        digit_h: 26.0,
        color: None,
    }
    .draw(st, th);
    if g.mode == MazeMode::BlitzFrenzy {
        StatBox {
            label: "TIME",
            value: &mmss(g.blitz_left),
            pos: vec2(lx + 84.0, left.y + 134.0),
            width: 84.0,
            digit_h: 26.0,
            color: Some(if g.blitz_left < 15.0 {
                th.danger
            } else {
                th.text
            }),
        }
        .draw(st, th);
    }
    // Frightened bar.
    text::label(
        st,
        "FRIGHTENED",
        lx,
        left.y + 262.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let frac = if g.fright_t > 0.0 {
        (g.fright_t / g.frightened_secs()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let bw = left.w - 32.0;
    draw_rectangle(lx, left.y + 286.0, bw, 10.0, with_alpha(BLACK, 0.4));
    draw_rectangle(lx, left.y + 286.0, bw * frac, 10.0, th.tiers[0]);
    // Ghost combo: the value of the next ghost.
    text::label(
        st,
        "COMBO",
        lx,
        left.y + 322.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let (txt, col) = if g.fright_t > 0.0 {
        (format!("NEXT {}", 200u32 << g.chain.min(3)), th.pieces[0])
    } else {
        ("-".to_string(), th.text_dim)
    };
    text::label(st, &txt, lx, left.y + 346.0, Size::X1, col, Align::Left);
    text::label(
        st,
        &format!("GHOSTS x{}", g.ghosts_eaten),
        lx,
        left.y + 372.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    text::label(
        st,
        ctx.mode_label,
        lx,
        left.y + MAZE_H - 34.0,
        Size::X1,
        th.accent,
        Align::Left,
    );

    let rx = right.x + 16.0;
    let w = right.w - 32.0;
    StatBox {
        label: "SCORE",
        value: &g.score.to_string(),
        pos: vec2(rx, right.y + 14.0),
        width: w,
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
        width: w,
        digit_h: 20.0,
        color: Some(th.text_dim),
    }
    .draw(st, th);
    text::label(
        st,
        "LIVES",
        rx,
        right.y + 164.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    for i in 0..g.lives {
        draw_runner(
            vec2(rx + 12.0 + i as f32 * 30.0, right.y + 198.0),
            10.0,
            Dir::Right,
            0.45,
            runner_color(),
        );
    }
    StatBox {
        label: "DOTS LEFT",
        value: &g.dots_left.to_string(),
        pos: vec2(rx, right.y + 226.0),
        width: w,
        digit_h: 22.0,
        color: None,
    }
    .draw(st, th);
    // Ghost legend (with the colour-blind emblems).
    let mut y = right.y + 318.0;
    for kind in GhostKind::ALL {
        let c = vec2(rx + 10.0, y + 8.0);
        draw_ghost(
            c,
            9.0,
            kind,
            GhostMode::Chase,
            false,
            Some(Dir::Right),
            ctx.time,
            th,
            ctx.colorblind,
        );
        text::label(
            st,
            kind.name(),
            rx + 28.0,
            y,
            Size::X1,
            th.text,
            Align::Left,
        );
        y += 26.0;
    }
    let legend = [
        ("ARROWS/WASD", "MOVE"),
        ("P", "PAUSE"),
        ("T/B/M", "THEME/CB/MUTE"),
    ];
    let mut y = right.y + MAZE_H - 90.0;
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
        y += 24.0;
    }
}

// ---------------------------------------------------------------- preview

/// Animated menu card: the runner eats along the top corridor with a ghost on
/// its tail, then the orb turns the tables. A pure function of `time`.
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
    let tile = (rect.w / W as f32).min(rect.h / H as f32).floor().max(3.0);
    let ox = rect.x + (rect.w - W as f32 * tile) * 0.5;
    let oy = rect.y + (rect.h - H as f32 * tile) * 0.5;
    let maze = Maze::classic();
    let cycle = 9.0;
    let p = (time % cycle) / cycle;
    // First half: runner right along row 1 with a chaser; second half back
    // with the ghost frightened and running ahead.
    let (rx, dir, gx, frightened) = if p < 0.5 {
        let k = p / 0.5;
        let x = 1.5 + 25.0 * k;
        (x, Dir::Right, x - 5.0, false)
    } else {
        let k = (p - 0.5) / 0.5;
        let x = 26.5 - 25.0 * k;
        (x, Dir::Left, x - 5.0, true)
    };
    let eaten_to = if p < 0.5 { rx } else { 0.0 };
    draw_walls(&maze, ox, oy, tile, theme, 0.5);
    draw_pellets(&maze, ox, oy, tile, theme, time, &|x, y| {
        y == 1 && p < 0.5 && (x as f32 + 0.5) < eaten_to
    });
    let at = |x: f32, y: f32| vec2(ox + x * tile, oy + y * tile);
    let r = tile * 0.45;
    let mouth = mouth_angle(time, true);
    draw_runner(at(rx, 1.5), r, dir, mouth, runner_color());
    let (gk, gmode) = if frightened {
        (GhostKind::Apex, GhostMode::Frightened)
    } else {
        (GhostKind::Apex, GhostMode::Chase)
    };
    let gpos = if frightened { rx - 5.0 } else { gx };
    draw_ghost(
        at(gpos.clamp(1.5, 26.5), 1.5),
        r,
        gk,
        gmode,
        false,
        Some(if frightened { Dir::Left } else { Dir::Right }),
        time,
        theme,
        false,
    );
    // Two more ghosts patrolling the lower corridor.
    let t2 = (time * 0.12).fract();
    let (x2, d2) = if t2 < 0.5 {
        (1.5 + 25.0 * t2 * 2.0, Dir::Right)
    } else {
        (26.5 - 25.0 * (t2 - 0.5) * 2.0, Dir::Left)
    };
    draw_ghost(
        at(x2, 21.5),
        r,
        GhostKind::Flank,
        GhostMode::Scatter,
        false,
        Some(d2),
        time,
        theme,
        false,
    );
    draw_ghost(
        at(27.0 - x2, 17.5),
        r,
        GhostKind::Vektor,
        GhostMode::Scatter,
        false,
        Some(d2.opposite()),
        time,
        theme,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_maze_is_centred_and_fits_between_the_hud_panels() {
        assert_eq!(MAZE_W, 784.0);
        assert_eq!(MAZE_H, 644.0);
        const {
            assert!(MX > 230.0 && MX + MAZE_W + 230.0 <= 1280.0);
            assert!(MY >= 0.0 && MY + MAZE_H <= 720.0);
        }
        assert!((MX - (1280.0 - MX - MAZE_W)).abs() < 1.0, "centred");
    }

    #[test]
    fn music_stage_rises_with_stage_and_flow_and_stays_in_range() {
        let mut g = MazeGame::new(MazeMode::Classic, 1);
        let mut prev = 0;
        for stage in 1..12 {
            g.stage = stage;
            let s = music_stage(&g);
            assert!(s >= prev && s < 5);
            prev = s;
        }
        assert_eq!(prev, 4);
        g.stage = 1;
        g.flow.flow = 95.0;
        assert!(music_stage(&g) >= 1, "overdrive lifts the music");
    }

    #[test]
    fn screen_positions_map_tile_centres() {
        let p = screen_pos(0.5, 0.5);
        assert_eq!(p, vec2(MX + TILE * 0.5, MY + TILE * 0.5));
    }

    #[test]
    fn ghost_colours_are_distinct() {
        let th = &crate::presentation::theme::themes()[0];
        let cols: Vec<_> = GhostKind::ALL.iter().map(|&k| ghost_color(th, k)).collect();
        for i in 0..4 {
            for j in (i + 1)..4 {
                assert_ne!(cols[i], cols[j]);
            }
        }
    }
}
