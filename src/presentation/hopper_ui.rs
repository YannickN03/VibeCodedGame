//! Drawing of VibeCoded Hopper: a top-down 2.5D world of 19 tiles across
//! (15 playable plus 2 decor columns on each side), scrolling camera,
//! vehicles with head-light cones, trains with signal lights, flowing water,
//! four-layer trees, the hopper with shadow and hop arc, HUD and the menu
//! preview.

use macroquad::prelude::*;

use crate::core::hopper::game::{HopperGame, Status, VIEW_ROWS};
use crate::core::hopper::world::{
    ItemKind, LaneKind, LaserPhase, RowSpec, TRAIN_PASS, TrainPhase, VehicleKind, WIDTH,
    laser_phase, train_phase, train_span,
};

use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::{Theme, darken, lighten, mix, with_alpha};
use super::widgets::shapes::fill_rounded;
use super::widgets::{FlowMeterBar, Orientation, RoundedPanel, StatBox};

pub const TILE: f32 = 40.0;
/// Decor columns on each side of the playfield.
pub const DECOR: f32 = 2.0;
pub const FIELD_X: f32 = 260.0;
pub const FIELD_Y: f32 = 80.0;
pub const FIELD_W: f32 = (WIDTH + 2.0 * DECOR) * TILE;
pub const FIELD_H: f32 = VIEW_ROWS * TILE;

/// Music-intensity stage `0..5` from the distance and the camera speed.
pub fn music_stage(g: &HopperGame) -> usize {
    (g.max_row / 25).clamp(0, 4) as usize
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

/// Screen x of world x (tiles).
pub fn sx(x: f32) -> f32 {
    FIELD_X + (x + DECOR) * TILE
}

/// Screen y of the TOP edge of row `r` (rows grow upwards on screen).
pub fn sy(g: &HopperGame, row_f: f32) -> f32 {
    FIELD_Y + FIELD_H - (row_f + 1.0 - g.camera_row) * TILE
}

fn clip_rect(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
    let (l, r) = (x.max(FIELD_X), (x + w).min(FIELD_X + FIELD_W));
    let (t, b) = (y.max(FIELD_Y), (y + h).min(FIELD_Y + FIELD_H));
    (r > l && b > t).then(|| Rect::new(l, t, r - l, b - t))
}

fn rect(x: f32, y: f32, w: f32, h: f32, c: Color) {
    if let Some(r) = clip_rect(x, y, w, h) {
        draw_rectangle(r.x, r.y, r.w, r.h, c);
    }
}

fn vehicle_color(theme: &Theme, k: VehicleKind) -> Color {
    match k {
        VehicleKind::Racer => theme.accent2,
        VehicleKind::Truck => theme.pieces[6],
        VehicleKind::Convoy => theme.pieces[5],
    }
}

/// Diagonal hatching inside a (clipped) rectangle: colour-blind pattern.
fn hatch(r: Rect, spacing: f32, col: Color) {
    let mut k = -r.h;
    while k < r.w {
        let (t0, t1) = ((-k).max(0.0), (r.w - k).min(r.h));
        if t1 > t0 {
            draw_line(r.x + k + t0, r.y + t0, r.x + k + t1, r.y + t1, 1.5, col);
        }
        k += spacing;
    }
}

/// Is the tile whose top edge is at `y` completely inside the field?
fn inside(y: f32) -> bool {
    y >= FIELD_Y - 0.5 && y + TILE <= FIELD_Y + FIELD_H + 0.5
}

fn draw_tree(x: f32, y: f32, theme: &Theme, colorblind: bool) {
    let g = theme.pieces[3];
    let (cx, base) = (x + TILE * 0.5, y + TILE * 0.88);
    // Layer 1: shadow. Layer 2: trunk. Layers 3 + 4: glowing crown.
    draw_ellipse(
        cx + 4.0,
        base,
        12.0,
        5.0,
        0.0,
        Color::new(0.0, 0.0, 0.0, 0.3),
    );
    draw_rectangle(
        cx - 3.0,
        base - 12.0,
        6.0,
        12.0,
        darken(theme.pieces[6], 0.5),
    );
    draw_triangle(
        vec2(cx - 15.0, base - 8.0),
        vec2(cx + 15.0, base - 8.0),
        vec2(cx, base - 26.0),
        darken(g, 0.45),
    );
    draw_triangle(
        vec2(cx - 12.0, base - 17.0),
        vec2(cx + 12.0, base - 17.0),
        vec2(cx, base - 34.0),
        g,
    );
    draw_triangle(
        vec2(cx - 6.0, base - 25.0),
        vec2(cx + 6.0, base - 25.0),
        vec2(cx, base - 36.0),
        lighten(g, 0.5),
    );
    draw_circle(cx, base - 36.0, 2.5, with_alpha(theme.accent2, 0.9));
    if colorblind {
        draw_line(
            cx - 9.0,
            base - 14.0,
            cx + 9.0,
            base - 14.0,
            2.0,
            with_alpha(WHITE, 0.7),
        );
        draw_line(
            cx - 6.0,
            base - 22.0,
            cx + 6.0,
            base - 22.0,
            2.0,
            with_alpha(WHITE, 0.7),
        );
    }
}

fn draw_shard(x: f32, y: f32, theme: &Theme, time: f32) {
    let (cx, cy) = (x + TILE * 0.5, y + TILE * 0.5 + 2.0 * (time * 4.0).sin());
    let c = theme.pieces[0];
    draw_circle(cx, cy, 15.0, with_alpha(c, 0.15));
    draw_poly(cx, cy, 4, 10.0, 0.0, darken(c, 0.4));
    draw_poly(cx, cy, 4, 8.0, 0.0, c);
    draw_poly(cx - 1.5, cy - 2.0, 4, 3.0, 0.0, with_alpha(WHITE, 0.8));
}

fn draw_item(x: f32, y: f32, kind: ItemKind, theme: &Theme, time: f32) {
    let (cx, cy) = (x + TILE * 0.5, y + TILE * 0.5);
    let pulse = 0.5 + 0.5 * (time * 5.0).sin();
    match kind {
        ItemKind::Aegis => {
            let c = theme.accent2;
            draw_circle(cx, cy, 15.0 + 2.0 * pulse, with_alpha(c, 0.18));
            draw_circle_lines(cx, cy, 11.0, 3.0, c);
            draw_poly(cx, cy, 6, 6.0, 0.0, lighten(c, 0.4));
        }
        ItemKind::Boots => {
            let c = theme.pieces[1];
            draw_circle(cx, cy, 15.0 + 2.0 * pulse, with_alpha(c, 0.18));
            for k in 0..2 {
                let o = k as f32 * 7.0 - 3.0;
                draw_line(cx - 8.0, cy + 3.0 - o, cx, cy - 5.0 - o, 3.0, c);
                draw_line(cx + 8.0, cy + 3.0 - o, cx, cy - 5.0 - o, 3.0, c);
            }
        }
    }
}

fn draw_row(g: &HopperGame, spec: &RowSpec, ctx: &UiCtx) {
    let th = ctx.theme;
    let y = sy(g, spec.row as f32);
    let t = g.time;
    let full_x = FIELD_X;
    let w = FIELD_W;
    let play_x = sx(0.0);
    match &spec.lane {
        LaneKind::Grass { trees, .. } => {
            let base = if spec.row % 2 == 0 {
                darken(th.pieces[3], 0.72)
            } else {
                darken(th.pieces[3], 0.66)
            };
            rect(full_x, y, w, TILE, base);
            // Decor columns are darker.
            rect(full_x, y, DECOR * TILE, TILE, with_alpha(BLACK, 0.25));
            rect(
                play_x + WIDTH * TILE,
                y,
                DECOR * TILE,
                TILE,
                with_alpha(BLACK, 0.25),
            );
            // Fine grass ticks.
            for c in 0..(WIDTH as i32 + 4) {
                let h = ((spec.row * 31 + c as i64 * 17) % 7) as f32;
                rect(
                    full_x + c as f32 * TILE + 8.0 + h * 3.0,
                    y + 8.0 + h * 2.0,
                    2.0,
                    5.0,
                    with_alpha(th.pieces[3], 0.25),
                );
            }
            // Crowns are not clipped, so only draw trees on tiles that lie
            // completely inside the field.
            if inside(y) {
                for (c, tree) in trees.iter().enumerate() {
                    if *tree {
                        draw_tree(sx(c as f32), y, th, ctx.colorblind);
                    }
                }
                // Decor pylons at the edges.
                for k in [0.0, WIDTH + 1.0] {
                    if (spec.row + k as i64) % 3 == 0 {
                        draw_tree(FIELD_X + k * TILE, y, th, false);
                    }
                }
            }
        }
        LaneKind::Road { dir, kind, .. } => {
            rect(full_x, y, w, TILE, darken(th.bg_light, 0.4));
            rect(
                full_x,
                y + TILE - 3.0,
                w,
                3.0,
                with_alpha(th.text_dim, 0.25),
            );
            // Dashed lane marking.
            let mut x = full_x;
            while x < full_x + w {
                rect(
                    x,
                    y + TILE * 0.5 - 1.0,
                    18.0,
                    2.0,
                    with_alpha(th.text, 0.25),
                );
                x += 36.0;
            }
            for c in spec.vehicles(t) {
                let len = kind.len();
                let (vx, vw) = (sx(c - len * 0.5), len * TILE);
                let col = vehicle_color(th, *kind);
                let vy = y + TILE * 0.15;
                let vh = TILE * 0.7;
                // Head-light cone in the direction of travel.
                let (fx, sign) = if *dir > 0.0 {
                    (vx + vw, 1.0)
                } else {
                    (vx, -1.0)
                };
                if let Some(r) = clip_rect(fx.min(fx + sign * 46.0), vy, 46.0, vh) {
                    draw_rectangle(r.x, r.y, r.w, r.h, with_alpha(th.pieces[1], 0.10));
                }
                rect(vx + 3.0, vy + 4.0, vw, vh, Color::new(0.0, 0.0, 0.0, 0.3));
                rect(vx, vy, vw, vh, darken(col, 0.5));
                rect(vx + 2.0, vy + 2.0, vw - 4.0, vh - 4.0, col);
                rect(
                    vx + vw * 0.25,
                    vy + 4.0,
                    vw * 0.4,
                    vh - 8.0,
                    with_alpha(BLACK, 0.35),
                );
                rect(vx + 3.0, vy + 2.0, vw - 6.0, 3.0, with_alpha(WHITE, 0.35));
                let (hx, tx) = if *dir > 0.0 {
                    (vx + vw - 5.0, vx)
                } else {
                    (vx, vx + vw - 5.0)
                };
                rect(hx, vy + 4.0, 5.0, 5.0, th.pieces[1]);
                rect(hx, vy + vh - 9.0, 5.0, 5.0, th.pieces[1]);
                rect(tx, vy + 4.0, 5.0, 5.0, th.danger);
                rect(tx, vy + vh - 9.0, 5.0, 5.0, th.danger);
                if ctx.colorblind
                    && let Some(r) = clip_rect(vx + 2.0, vy + 2.0, vw - 4.0, vh - 4.0)
                {
                    match kind {
                        VehicleKind::Racer => {}
                        VehicleKind::Truck => hatch(r, 9.0, with_alpha(WHITE, 0.55)),
                        VehicleKind::Convoy => {
                            let mut yy = r.y + 4.0;
                            while yy < r.bottom() {
                                draw_line(r.x, yy, r.right(), yy, 2.0, with_alpha(WHITE, 0.5));
                                yy += 7.0;
                            }
                        }
                    }
                }
            }
        }
        LaneKind::Rail { dir, period, phase } => {
            rect(full_x, y, w, TILE, darken(th.bg_light, 0.25));
            // Sleepers and rails.
            let mut x = full_x;
            while x < full_x + w {
                rect(x, y + 6.0, 5.0, TILE - 12.0, darken(th.pieces[6], 0.6));
                x += 14.0;
            }
            rect(full_x, y + 11.0, w, 3.0, th.text_dim);
            rect(full_x, y + TILE - 14.0, w, 3.0, th.text_dim);
            let state = train_phase(*period, *phase, t);
            // Signal lights at both ends: blinking while the warning runs.
            let lit = match state {
                TrainPhase::Warning { .. } => ((t * 8.0) as i32) % 2 == 0,
                TrainPhase::Pass { .. } => true,
                TrainPhase::Idle => false,
            };
            for lx in [full_x + 10.0, full_x + w - 26.0] {
                rect(
                    lx,
                    y + 4.0,
                    16.0,
                    TILE - 8.0,
                    Color::new(0.05, 0.05, 0.08, 0.9),
                );
                let on = |c: Color, a: bool| if a { c } else { darken(c, 0.8) };
                draw_circle(lx + 8.0, y + 13.0, 5.0, on(th.danger, lit));
                draw_circle(
                    lx + 8.0,
                    y + TILE - 13.0,
                    5.0,
                    on(
                        th.pieces[1],
                        !lit && matches!(state, TrainPhase::Warning { .. }),
                    ),
                );
                if lit {
                    draw_circle(lx + 8.0, y + 13.0, 11.0, with_alpha(th.danger, 0.25));
                }
            }
            if matches!(state, TrainPhase::Warning { .. }) {
                rect(
                    full_x,
                    y,
                    w,
                    TILE,
                    with_alpha(th.danger, 0.07 + 0.05 * ((t * 8.0).sin() * 0.5 + 0.5)),
                );
            }
            if let TrainPhase::Pass { progress } = state {
                let (l, r) = train_span(*dir, progress);
                let (tx, tw) = (sx(l), (r - l) * TILE);
                let c = th.pieces[2];
                rect(tx, y + 2.0, tw, TILE - 4.0, darken(c, 0.5));
                rect(tx + 2.0, y + 5.0, tw - 4.0, TILE - 10.0, c);
                let mut wx = tx + 10.0;
                while wx < tx + tw - 10.0 {
                    rect(wx, y + 9.0, 14.0, 8.0, with_alpha(WHITE, 0.55));
                    wx += 26.0;
                }
                rect(
                    tx,
                    y + TILE * 0.5 - 1.0,
                    tw,
                    2.0,
                    with_alpha(th.accent2, 0.8),
                );
                if ctx.colorblind
                    && let Some(rr) = clip_rect(tx, y + 2.0, tw, TILE - 4.0)
                {
                    hatch(rr, 8.0, with_alpha(WHITE, 0.6));
                }
            }
        }
        LaneKind::River { dir, width, .. } => {
            rect(full_x, y, w, TILE, mix(th.tiers[0], th.bg_dark, 0.55));
            // Flowing wave lines (move with the stream).
            let shift = t * dir * 20.0;
            for row in 0..3 {
                let mut x = full_x - 40.0 + shift.rem_euclid(40.0);
                while x < full_x + w {
                    let wy = y + 8.0 + row as f32 * 12.0 + 2.0 * ((x + t * 3.0) * 0.1).sin();
                    rect(x, wy, 16.0, 2.0, with_alpha(th.text, 0.18));
                    x += 40.0;
                }
            }
            for c in spec.platforms(t) {
                let (lx, lw) = (sx(c - width * 0.5), width * TILE);
                let col = darken(th.pieces[6], 0.35);
                rect(
                    lx + 2.0,
                    y + 8.0,
                    lw,
                    TILE - 12.0,
                    Color::new(0.0, 0.0, 0.0, 0.25),
                );
                rect(lx, y + 5.0, lw, TILE - 10.0, darken(col, 0.4));
                rect(lx + 2.0, y + 7.0, lw - 4.0, TILE - 14.0, col);
                rect(lx + 4.0, y + 9.0, lw - 8.0, 4.0, with_alpha(WHITE, 0.2));
                let mut ringx = lx + 14.0;
                while ringx < lx + lw - 8.0 {
                    if let Some(r) = clip_rect(ringx - 5.0, y + 14.0, 10.0, 12.0) {
                        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, with_alpha(BLACK, 0.4));
                    }
                    ringx += 26.0;
                }
                if ctx.colorblind
                    && let Some(r) = clip_rect(lx, y + 5.0, lw, TILE - 10.0)
                {
                    let mut dx = r.x + 5.0;
                    while dx < r.right() {
                        draw_circle(dx, r.y + r.h * 0.5, 2.0, with_alpha(WHITE, 0.8));
                        dx += 12.0;
                    }
                }
            }
            if let Some(i) = g.ice
                && i.row == spec.row
                && i.timer > 0.0
            {
                let a = (i.timer / 0.5).min(1.0);
                rect(
                    sx(i.x - 0.5),
                    y + 3.0,
                    TILE,
                    TILE - 6.0,
                    with_alpha(th.accent2, 0.65 * a),
                );
                rect(
                    sx(i.x - 0.5) + 4.0,
                    y + 7.0,
                    TILE - 8.0,
                    4.0,
                    with_alpha(WHITE, 0.7 * a),
                );
            }
        }
        LaneKind::Laser {
            period,
            phase,
            active,
            ..
        } => {
            rect(full_x, y, w, TILE, darken(th.bg_light, 0.5));
            let ph = laser_phase(*period, *phase, t);
            for (c, a) in active.iter().enumerate() {
                let x = sx(c as f32);
                rect(
                    x + 2.0,
                    y + 2.0,
                    TILE - 4.0,
                    TILE - 4.0,
                    with_alpha(th.text_dim, 0.12),
                );
                if !*a {
                    continue;
                }
                match ph {
                    LaserPhase::Charging => {
                        let blink = ((t * 10.0) as i32) % 2 == 0;
                        if let Some(r) = clip_rect(x + 3.0, y + 3.0, TILE - 6.0, TILE - 6.0) {
                            draw_rectangle_lines(
                                r.x,
                                r.y,
                                r.w,
                                r.h,
                                2.0,
                                with_alpha(th.pieces[1], if blink { 0.9 } else { 0.4 }),
                            );
                        }
                    }
                    LaserPhase::Active => {
                        rect(x, y, TILE, TILE, with_alpha(th.danger, 0.6));
                        rect(x + 8.0, y, TILE - 16.0, TILE, with_alpha(WHITE, 0.85));
                    }
                    LaserPhase::Off => {
                        rect(x + 14.0, y + 14.0, 12.0, 12.0, with_alpha(th.danger, 0.25));
                    }
                }
                if ctx.colorblind && ph != LaserPhase::Active {
                    rect(
                        x + 6.0,
                        y + TILE * 0.5 - 1.0,
                        TILE - 12.0,
                        2.0,
                        with_alpha(WHITE, 0.6),
                    );
                }
            }
        }
        LaneKind::Bridge { solid, .. } => {
            rect(full_x, y, w, TILE, Color::new(0.02, 0.02, 0.05, 1.0));
            for (c, s) in solid.iter().enumerate() {
                let x = sx(c as f32);
                if !*s {
                    continue;
                }
                let key = (spec.row, c);
                let sunk = g.sinking.get(&key).map(|st| (t - st).max(0.0));
                let (down, gone) = match sunk {
                    Some(d) if d >= 0.9 => (1.0, true),
                    Some(d) => ((d / 0.9).powi(2), false),
                    None => (0.0, false),
                };
                if gone {
                    continue;
                }
                let shake = if down > 0.3 {
                    (t * 60.0).sin() * 1.5 * down
                } else {
                    0.0
                };
                let tile = darken(th.pieces[6], 0.3 + 0.4 * down);
                rect(
                    x + 1.0 + shake,
                    y + 2.0 + down * 6.0,
                    TILE - 2.0,
                    TILE - 4.0,
                    tile,
                );
                rect(
                    x + 3.0 + shake,
                    y + 4.0 + down * 6.0,
                    TILE - 6.0,
                    4.0,
                    with_alpha(WHITE, 0.2 * (1.0 - down)),
                );
                if ctx.colorblind {
                    rect(
                        x + 6.0 + shake,
                        y + TILE - 12.0,
                        TILE - 12.0,
                        2.0,
                        with_alpha(WHITE, 0.6),
                    );
                }
            }
        }
    }
    // Collectibles sit on top of the lane.
    if let Some(c) = spec.shard
        && inside(y)
        && !g.shard_taken(spec.row)
    {
        draw_shard(sx(c as f32), y, th, ctx.time);
    }
    if let Some((k, c)) = spec.item
        && inside(y)
        && !g.item_taken(spec.row)
    {
        draw_item(sx(c as f32), y, k, th, ctx.time);
    }
}

/// The hopper: shadow on the ground, body lifted by the hop arc.
pub fn draw_hopper(
    x: f32,
    y_top: f32,
    z: f32,
    progress: f32,
    theme: &Theme,
    time: f32,
    alpha: f32,
) {
    let (cx, base) = (x, y_top + TILE * 0.8);
    let body = with_alpha(theme.accent2, alpha);
    let dark = with_alpha(darken(theme.accent2, 0.5), alpha);
    draw_ellipse(
        cx,
        base,
        12.0 - 4.0 * z,
        5.0 - 2.0 * z,
        0.0,
        Color::new(0.0, 0.0, 0.0, 0.35),
    );
    let lift = z * TILE * 0.6;
    let squash = 1.0 + 0.18 * z - 0.12 * (1.0 - progress.min(1.0)).powi(6);
    let (w, h) = (22.0 / squash, 24.0 * squash);
    let by = base - lift - h;
    draw_circle(
        cx,
        by + h * 0.5,
        22.0,
        with_alpha(theme.accent2, 0.10 * alpha),
    );
    fill_rounded(Rect::new(cx - w * 0.5, by, w, h), 8.0, dark);
    fill_rounded(
        Rect::new(cx - w * 0.5 + 2.0, by + 2.0, w - 4.0, h - 4.0),
        6.0,
        body,
    );
    fill_rounded(
        Rect::new(cx - w * 0.5 + 4.0, by + 4.0, w - 8.0, 5.0),
        2.0,
        with_alpha(WHITE, 0.4 * alpha),
    );
    // Eyes and a pulsing antenna.
    draw_circle(cx - 4.5, by + 12.0, 2.6, with_alpha(WHITE, alpha));
    draw_circle(cx + 4.5, by + 12.0, 2.6, with_alpha(WHITE, alpha));
    draw_circle(cx - 4.5, by + 11.5, 1.2, with_alpha(BLACK, alpha));
    draw_circle(cx + 4.5, by + 11.5, 1.2, with_alpha(BLACK, alpha));
    draw_line(cx, by, cx, by - 6.0, 2.0, dark);
    draw_circle(
        cx,
        by - 7.0,
        2.4 + (time * 8.0).sin().abs(),
        with_alpha(theme.accent, alpha),
    );
}

/// Draws the playfield and everything on it.
pub fn draw_world(g: &HopperGame, ctx: &UiCtx) {
    let th = ctx.theme;
    // Glow frame breathing with the beat.
    let tcol = th.tiers[g.tier().index()];
    let glow = 0.12 + 0.28 * ctx.pulse * (0.3 + g.flow.flow / 100.0);
    for i in 1..=3 {
        let k = i as f32 * 3.0;
        draw_rectangle_lines(
            FIELD_X - k,
            FIELD_Y - k,
            FIELD_W + 2.0 * k,
            FIELD_H + 2.0 * k,
            2.0,
            with_alpha(tcol, glow / i as f32),
        );
    }
    draw_rectangle(FIELD_X, FIELD_Y, FIELD_W, FIELD_H, th.well);
    let lo = g.camera_row.floor() as i64 - 1;
    let hi = (g.camera_row + VIEW_ROWS).ceil() as i64 + 1;
    // Draw far rows first so nearer rows overlap properly.
    for r in (lo.max(0)..=hi).rev() {
        if let Some(spec) = g.world.peek(r) {
            draw_row(g, spec, ctx);
        }
    }
    // The player.
    let px = sx(g.pos_x());
    let py = sy(g, g.pos_row());
    let alpha = if g.inv > 0.0 && ((ctx.time * 14.0) as i32) % 2 == 0 {
        0.4
    } else {
        1.0
    };
    if g.is_playing() {
        draw_hopper(px, py, g.hop_z(), g.hop_progress(), th, ctx.time, alpha);
        if g.shield {
            draw_circle_lines(
                px,
                py + TILE * 0.5,
                22.0,
                2.5,
                with_alpha(th.accent2, 0.5 + 0.3 * (ctx.time * 6.0).sin().abs()),
            );
        }
        if g.boots > 0.0 {
            draw_circle_lines(
                px,
                py + TILE * 0.8,
                14.0,
                2.0,
                with_alpha(th.pieces[1], 0.8),
            );
        }
    } else {
        draw_circle(px, py + TILE * 0.5, 16.0, with_alpha(th.danger, 0.6));
    }
    // The sweeper: a red glow at the bottom edge that grows when the
    // hopper falls behind.
    let behind = (g.pos_row() - g.camera_row).clamp(0.0, 4.0);
    if g.mode != crate::core::hopper::game::HopperMode::Zen && behind < 2.5 {
        let k = 1.0 - behind / 2.5;
        for i in 0..8 {
            let a = k * 0.35 * (1.0 - i as f32 / 8.0);
            draw_rectangle(
                FIELD_X,
                FIELD_Y + FIELD_H - (i as f32 + 1.0) * 8.0,
                FIELD_W,
                8.0,
                with_alpha(th.danger, a),
            );
        }
    }
    // Hide anything that spilled over the field edges: cover with the
    // background colour before the HUD is drawn.
    if let Status::Dead(_) = g.status {
        draw_rectangle(
            FIELD_X,
            FIELD_Y,
            FIELD_W,
            FIELD_H,
            with_alpha(th.danger, 0.10),
        );
    }
    let _ = TRAIN_PASS;
}

/// Both HUD panels.
pub fn draw_hud(g: &HopperGame, ctx: &UiCtx) {
    let (th, st) = (ctx.theme, ctx.stage);
    let left = Rect::new(10.0, FIELD_Y - 40.0, 230.0, 600.0);
    let right = Rect::new(FIELD_X + FIELD_W + 10.0, FIELD_Y - 40.0, 230.0, 600.0);
    RoundedPanel::new(left, th.accent2).draw(th);
    RoundedPanel::new(right, th.accent).draw(th);

    let lx = left.x + 18.0;
    text::label(
        st,
        "FLOW",
        lx,
        left.y + 16.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let tier = g.tier();
    text::label(
        st,
        &format!("{}x", tier.multiplier()),
        left.right() - 18.0,
        left.y + 16.0,
        Size::X1,
        th.tiers[tier.index()],
        Align::Right,
    );
    FlowMeterBar {
        rect: Rect::new(lx + 44.0, left.y + 48.0, 26.0, 200.0),
        orientation: Orientation::Vertical,
        flow: g.flow.flow,
        pulse: ctx.pulse,
    }
    .draw(st, th);
    StatBox {
        label: "COMBO",
        value: &format!("x{}", g.combo),
        pos: vec2(lx + 84.0, left.y + 60.0),
        width: 92.0,
        digit_h: 26.0,
        color: Some(if g.combo >= 4 { th.accent2 } else { th.text }),
    }
    .draw(st, th);
    text::label(
        st,
        "POWERUP",
        lx,
        left.y + 300.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    let (name, frac, col) = if g.boots > 0.0 {
        ("PHASE-BOOTS", g.boots / 8.0, th.pieces[1])
    } else if g.shield {
        ("AEGIS SHIELD", 1.0, th.accent2)
    } else {
        ("NONE", 0.0, th.text_dim)
    };
    text::label(st, name, lx, left.y + 324.0, Size::X1, col, Align::Left);
    draw_rectangle(lx, left.y + 348.0, 194.0, 8.0, with_alpha(BLACK, 0.4));
    draw_rectangle(lx, left.y + 348.0, 194.0 * frac, 8.0, col);
    text::label(
        st,
        "SHARDS",
        lx,
        left.y + 390.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    text::label(
        st,
        &g.shards.to_string(),
        lx,
        left.y + 412.0,
        Size::X2,
        th.pieces[0],
        Align::Left,
    );
    text::label(
        st,
        ctx.mode_label,
        lx,
        left.y + 560.0,
        Size::X1,
        th.accent,
        Align::Left,
    );

    let rx = right.x + 18.0;
    StatBox {
        label: "SCORE",
        value: &g.score.to_string(),
        pos: vec2(rx, right.y + 16.0),
        width: 194.0,
        digit_h: 30.0,
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
        pos: vec2(rx, right.y + 100.0),
        width: 194.0,
        digit_h: 20.0,
        color: Some(th.text_dim),
    }
    .draw(st, th);
    StatBox {
        label: "DISTANCE",
        value: &g.max_row.to_string(),
        pos: vec2(rx, right.y + 170.0),
        width: 194.0,
        digit_h: 30.0,
        color: None,
    }
    .draw(st, th);
    let legend = [
        ("ARROWS/WASD", "HOP"),
        ("P", "PAUSE"),
        ("T / B / M", "THEME/CB/MUTE"),
    ];
    let mut y = right.y + 470.0;
    for (k, a) in legend {
        text::label(st, k, rx, y, Size::X1, th.text, Align::Left);
        text::label(
            st,
            a,
            right.right() - 18.0,
            y,
            Size::X1,
            th.text_dim,
            Align::Right,
        );
        y += 24.0;
    }
}

/// Animated mini scene for the menu card: a hopper crossing a road and a
/// river, riding a log. A pure function of `time`.
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
    let rows = 6;
    let th_ = (rect.h / rows as f32).floor().max(8.0);
    let cols = (rect.w / th_).floor().max(6.0);
    let tile = th_;
    let ox = rect.x + (rect.w - cols * tile) * 0.5;
    let oy = rect.bottom() - rows as f32 * tile;
    // Rows from the bottom: grass, road, road, river, river, grass.
    #[derive(Clone, Copy, PartialEq)]
    enum K {
        Grass,
        Road,
        River,
    }
    let kinds = [K::Grass, K::Road, K::Road, K::River, K::River, K::Grass];
    let cycle = 7.0;
    let phase = (time % cycle) / cycle * 6.0; // row progress 0..6
    let step = phase.floor() as usize;
    let within = phase - phase.floor();
    // Hop arc inside each 1/6 of the cycle.
    let hop = (within / 0.35).min(1.0);
    let z = 4.0 * hop * (1.0 - hop);
    let hopper_row = step as f32 + hop;
    for (i, k) in kinds.iter().enumerate() {
        let y = oy + (rows - 1 - i) as f32 * tile;
        let col = match k {
            K::Grass => darken(theme.pieces[3], 0.7),
            K::Road => darken(theme.bg_light, 0.4),
            K::River => mix(theme.tiers[0], theme.bg_dark, 0.55),
        };
        draw_rectangle(ox, y, cols * tile, tile, col);
        match k {
            K::Road => {
                let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
                for v in 0..3 {
                    let len = tile * 1.6;
                    let x = ox
                        + ((time * 40.0 * dir + v as f32 * cols * tile / 3.0 + i as f32 * 30.0)
                            .rem_euclid(cols * tile + len))
                        - len;
                    let cx = x.max(ox);
                    let w = (x + len).min(ox + cols * tile) - cx;
                    if w > 0.0 {
                        draw_rectangle(
                            cx,
                            y + tile * 0.18,
                            w,
                            tile * 0.64,
                            if v % 2 == 0 {
                                theme.accent2
                            } else {
                                theme.pieces[6]
                            },
                        );
                    }
                }
            }
            K::River => {
                let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
                for v in 0..2 {
                    let len = tile * 3.0;
                    let x = ox
                        + ((time * 22.0 * dir + v as f32 * cols * tile / 2.0 + i as f32 * 17.0)
                            .rem_euclid(cols * tile + len))
                        - len;
                    let cx = x.max(ox);
                    let w = (x + len).min(ox + cols * tile) - cx;
                    if w > 0.0 {
                        draw_rectangle(
                            cx,
                            y + tile * 0.15,
                            w,
                            tile * 0.7,
                            darken(theme.pieces[6], 0.3),
                        );
                    }
                }
            }
            K::Grass => {
                if i == 5 {
                    draw_triangle(
                        vec2(ox + tile * 1.5, y + tile * 0.9),
                        vec2(ox + tile * 2.5, y + tile * 0.9),
                        vec2(ox + tile * 2.0, y + tile * 0.1),
                        theme.pieces[3],
                    );
                }
            }
        }
    }
    // The hopper (rides along on river rows).
    let on_river = (2.9..5.0).contains(&hopper_row);
    let drift = if on_river {
        (hopper_row - 2.9).max(0.0) * 0.0
    } else {
        0.0
    };
    let hx = ox + cols * tile * 0.5 + drift + (time * 0.0);
    let hy = oy + (rows as f32 - hopper_row - 1.0) * tile;
    draw_hopper_mini(hx, hy, tile, z, theme, time);
}

fn draw_hopper_mini(cx: f32, y_top: f32, tile: f32, z: f32, theme: &Theme, time: f32) {
    let s = tile / TILE;
    draw_ellipse(
        cx,
        y_top + tile * 0.85,
        9.0 * s,
        4.0 * s,
        0.0,
        Color::new(0.0, 0.0, 0.0, 0.35),
    );
    let lift = z * tile * 0.35;
    let (w, h) = (20.0 * s, 22.0 * s);
    let by = y_top + tile * 0.85 - lift - h;
    draw_rectangle(cx - w * 0.5, by, w, h, darken(theme.accent2, 0.5));
    draw_rectangle(
        cx - w * 0.5 + 1.5,
        by + 1.5,
        w - 3.0,
        h - 3.0,
        theme.accent2,
    );
    draw_circle(cx - 3.0 * s, by + h * 0.45, 2.0 * s.max(0.6), WHITE);
    draw_circle(cx + 3.0 * s, by + h * 0.45, 2.0 * s.max(0.6), WHITE);
    draw_circle(
        cx,
        by - 3.0 * s,
        2.0 + (time * 8.0).sin().abs(),
        theme.accent,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hopper::game::HopperMode;

    #[test]
    fn the_field_is_19_tiles_wide_and_fits_between_the_hud_panels() {
        assert_eq!(FIELD_W, 19.0 * TILE);
        const {
            assert!(FIELD_X > 250.0 && FIELD_X + FIELD_W + 250.0 <= 1280.0);
            assert!(FIELD_Y + FIELD_H <= 720.0);
        }
        assert!(
            (FIELD_X - (1280.0 - FIELD_X - FIELD_W)).abs() < 1.0,
            "centred"
        );
    }

    #[test]
    fn world_to_screen_mapping_is_consistent() {
        let g = HopperGame::new(HopperMode::Classic, 1);
        // Row 0 at camera 0 sits at the bottom of the field.
        assert_eq!(sy(&g, 0.0), FIELD_Y + FIELD_H - TILE);
        assert_eq!(sy(&g, 1.0), sy(&g, 0.0) - TILE, "rows grow upwards");
        assert_eq!(
            sx(0.0),
            FIELD_X + DECOR * TILE,
            "two decor columns on the left"
        );
        assert_eq!(sx(15.0), FIELD_X + (15.0 + DECOR) * TILE);
        assert_eq!(crate::core::hopper::world::COLS, 15);
    }

    #[test]
    fn music_stage_rises_with_distance() {
        let mut g = HopperGame::new(HopperMode::Classic, 1);
        let mut prev = 0;
        for r in 0..300 {
            g.max_row = r;
            let s = music_stage(&g);
            assert!(s >= prev && s < 5);
            prev = s;
        }
        assert_eq!(prev, 4);
    }
}
