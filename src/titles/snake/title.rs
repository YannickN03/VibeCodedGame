//! `ArcadeTitle` adapter of VibeCoded Snake.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use crate::core::FlowTier;
use crate::core::snake::bot;
use crate::core::snake::game::{
    Collision, MAX_SPEED, SnakeEvent, SnakeGame, SnakeMode, Status, steps_per_second,
};
use crate::core::snake::grid::Direction;
use crate::core::snake::interp::{RenderPoint, render_points};
use crate::presentation::digits;
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, mix, with_alpha};

use super::render::{
    self, BOARD_X, BOARD_Y, CELL, SnakeStyle, cell_center, draw_apple, draw_snake,
};

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new(
        "classic",
        "CLASSIC",
        "Deadly walls. The snake gets faster with every apple.",
    ),
    GameModeInfo::new(
        "portal",
        "PORTAL WRAP",
        "Walls wrap around the field. Only your own body can stop you.",
    ),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "Symmetric obstacles and apples: the same course for everyone today.",
    )
    .daily(),
    GameModeInfo::new(
        "zen",
        "ZEN",
        "Cannot die. Walls wrap and a bite only trims your tail.",
    ),
];

const LEGEND: [(&str, &str); 5] = [
    ("ARROWS / WASD", "Steer"),
    ("SPACE / SHIFT", "Sprint boost (Settings)"),
    ("2 QUICK TURNS", "Buffered, no 180 degree bites"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

const TURN_KEYS: [([Key; 2], Direction); 4] = [
    ([Key::Up, Key::W], Direction::Up),
    ([Key::Down, Key::S], Direction::Down),
    ([Key::Left, Key::A], Direction::Left),
    ([Key::Right, Key::D], Direction::Right),
];

pub struct SnakeTitle {
    game: SnakeGame,
    ctx: TitleContext,
    time: f32,
    reported: bool,
    last_tier: FlowTier,
    smoke_seed: u64,
}

impl Default for SnakeTitle {
    fn default() -> Self {
        Self::new()
    }
}

impl SnakeTitle {
    pub fn new() -> Self {
        SnakeTitle {
            game: SnakeGame::new(SnakeMode::Classic, 1),
            ctx: TitleContext::default(),
            time: 0.0,
            reported: false,
            last_tier: FlowTier::Chill,
            smoke_seed: 0,
        }
    }

    pub fn game(&self) -> &SnakeGame {
        &self.game
    }

    fn advance(&mut self, dt: f32, boost: bool) -> FrameEvents {
        self.time += dt;
        self.game.update(dt, boost);
        let mut ev = self.collect_events();
        if !self.game.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    /// Converts the game's events plus the per-frame state into `FrameEvents`.
    fn collect_events(&mut self) -> FrameEvents {
        let theme = crate::presentation::theme::themes()
            [self.ctx.settings.theme % crate::presentation::theme::themes().len()]
        .clone();
        let mut ev = FrameEvents::default();
        let tier = self.game.tier();
        let apple_col = theme.pieces[4];
        let gold = theme.pieces[1];
        for e in self.game.drain_events() {
            match e {
                SnakeEvent::Turned(_) => ev.sound(cue::ROTATE_OR_TURN, 0.3),
                SnakeEvent::Wrapped(_) => ev.sound(cue::ACTION_TICK, 0.18),
                SnakeEvent::Ate { pos, pulse, points } => {
                    let c = cell_center(pos.x as f32, pos.y as f32);
                    if pulse {
                        ev.burst(c, gold, 26, 340.0);
                        ev.callout(format!("+{points}"), c, gold, Size::X2);
                        ev.sound(cue::CLEAR_OR_EAT, 0.7);
                        ev.flash(gold, 0.45);
                        ev.note(tier.index(), 7, 0.6);
                    } else {
                        ev.burst(c, apple_col, 12, 240.0);
                        ev.callout(format!("+{points}"), c, theme.text, Size::X1);
                        let idx = (self.game.apples_eaten as usize * 3) % 8;
                        ev.note(tier.index(), idx, 0.55);
                    }
                }
                SnakeEvent::PulseSpawned(pos) => {
                    let c = cell_center(pos.x as f32, pos.y as f32);
                    ev.callout("PULSE!", c - vec2(0.0, 26.0), gold, Size::X1);
                    ev.sound(cue::ACTION_TICK, 0.5);
                }
                SnakeEvent::PulseExpired(_) => ev.sound(cue::ACTION_TICK, 0.3),
                SnakeEvent::Trimmed { removed } => {
                    for p in &removed {
                        ev.burst(
                            cell_center(p.x as f32, p.y as f32),
                            theme.pieces[3],
                            3,
                            160.0,
                        );
                    }
                    let h = self.game.head();
                    ev.callout(
                        "TRIMMED",
                        cell_center(h.x as f32, h.y as f32),
                        theme.accent,
                        Size::X1,
                    );
                    ev.sound(cue::ACTION_TICK, 0.4);
                }
                SnakeEvent::Died { at, .. } => {
                    let c = cell_center(
                        at.x.clamp(0, self.game.width - 1) as f32,
                        at.y.clamp(0, self.game.height - 1) as f32,
                    );
                    ev.burst(c, theme.danger, 40, 380.0);
                    ev.shake = 14.0;
                    ev.flash(theme.danger, 0.9);
                    ev.sound(cue::GAME_OVER, 0.8);
                }
                SnakeEvent::Victory => {
                    ev.callout("PERFECT!", vec2(640.0, 400.0), theme.accent2, Size::X3);
                    ev.sound("finish", 0.8);
                    ev.flash(theme.accent2, 0.8);
                }
            }
        }
        if tier > self.last_tier {
            let c = theme.tiers[tier.index()];
            if tier == FlowTier::Overdrive {
                ev.callout("OVERDRIVE 2.0x", vec2(640.0, 300.0), c, Size::X3);
                ev.flash(c, 1.0);
                ev.sound(cue::OVERDRIVE_UP, 0.8);
            } else {
                ev.callout(
                    format!("{} {}x", tier.name(), tier.multiplier()),
                    vec2(640.0, 300.0),
                    c,
                    Size::X2,
                );
            }
        }
        self.last_tier = tier;
        ev.flow = self.game.flow.flow;
        ev.music_stage = render::music_stage(&self.game);
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.game;
        let (title, won) = match g.status {
            Status::Victory => ("PERFECT!", true),
            Status::Dead(Collision::WallCollision) => ("HIT THE WALL", false),
            Status::Dead(Collision::ObstacleCollision) => ("HIT AN OBSTACLE", false),
            Status::Dead(Collision::SelfCollision) => ("BIT YOURSELF", false),
            Status::Playing => ("GAME OVER", false),
        };
        SessionEnd {
            title,
            won,
            headline: ("SCORE", g.score.to_string()),
            value: g.score,
            lower_is_better: false,
            counts_for_record: true,
            summary: vec![
                ("LENGTH".into(), g.segments.len().to_string()),
                ("APPLES".into(), g.apples_eaten.to_string()),
                ("TIME".into(), digits::format_time(g.elapsed)),
                (
                    "TOP SPEED".into(),
                    format!("{:.1} STEPS/S", steps_per_second(g.apples_eaten)),
                ),
            ],
        }
    }

    /// Invariants checked on every headless bot step.
    fn check_invariants(&self) {
        let g = &self.game;
        let mut seen = std::collections::HashSet::new();
        for s in &g.segments {
            assert!(g.in_bounds(*s), "segment outside the field");
            assert!(!g.is_obstacle(*s), "segment inside an obstacle");
            assert!(seen.insert(*s), "overlapping segments");
        }
        assert_eq!(
            g.segments.len() as u32 + g.pending_growth() + g.trimmed_total,
            g.start_len() + g.total_growth,
            "length bookkeeping"
        );
        if let Some(a) = g.apple {
            assert!(!seen.contains(&a.pos) && !g.is_obstacle(a.pos));
        }
        assert!((0.0..=100.0).contains(&g.flow.flow));
        assert!(steps_per_second(g.apples_eaten) <= MAX_SPEED);
    }
}

impl ArcadeTitle for SnakeTitle {
    fn id(&self) -> &'static str {
        "snake"
    }

    fn title(&self) -> &'static str {
        "VIBECODED SNAKE"
    }

    fn tagline(&self) -> &'static str {
        "eat, grow, find your flow"
    }

    fn accent_color(&self) -> Color {
        hex(0x5CF08A)
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.game = SnakeGame::new(SnakeMode::from_id(mode_id), seed);
        self.time = 0.0;
        self.reported = false;
        self.last_tier = FlowTier::Chill;
        self.smoke_seed = seed;
    }

    fn set_context(&mut self, ctx: &TitleContext) {
        self.ctx = ctx.clone();
    }

    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        for (keys, dir) in TURN_KEYS {
            if input.any_pressed(&keys) {
                self.game.queue_turn(dir);
            }
        }
        let boost = self.ctx.settings.snake_boost && input.any_down(&[Key::Space, Key::Shift]);
        self.advance(dt, boost)
    }

    fn demo_update(&mut self, dt: f32) -> Option<FrameEvents> {
        bot::steer(&mut self.game);
        Some(self.advance(dt, false))
    }

    fn draw(&self, canvas: &VirtualCanvas, theme: &Theme, colorblind: bool) {
        let g = &self.game;
        render::draw_board(g, theme, self.ctx.pulse, self.time);
        render::draw_apples(g, theme, self.time);
        let pts = render_points(&g.prev_segments, &g.segments, g.alpha());
        let dead = matches!(g.status, Status::Dead(_));
        let style = SnakeStyle::from_theme(theme, colorblind, dead);
        let head = g.head();
        let look = [g.pulse, g.apple]
            .into_iter()
            .flatten()
            .min_by_key(|a| a.pos.manhattan(head))
            .map(|a| (a.pos.x as f32, a.pos.y as f32));
        let (dx, dy) = g.dir.delta();
        draw_snake(
            &pts,
            vec2(BOARD_X, BOARD_Y),
            CELL,
            &style,
            look,
            (dx as f32, dy as f32),
            self.time,
        );
        render::draw_death_tint(g, theme);
        let mode = MODES
            .iter()
            .find(|m| m.id == g.mode.id())
            .map(|m| m.label)
            .unwrap_or("");
        render::draw_hud(
            g,
            &render::HudCtx {
                stage: canvas,
                theme,
                mode_label: mode,
                best: self.ctx.best,
                today_iso: &self.ctx.today_iso,
                pulse: self.ctx.pulse,
                legend: "ARROWS/WASD steer    SPACE/SHIFT boost    P pause",
            },
        );
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        draw_snake_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        if self.game.status == Status::Victory {
            let id = self.game.mode.id();
            self.start(id, self.smoke_seed.wrapping_add(tick as u64 + 1));
        }
        // A death of the bot is a regression and must fail loudly.
        assert!(
            self.game.is_playing(),
            "snake bot died at step {tick}: {:?}",
            self.game.status
        );
        bot::steer(&mut self.game);
        self.game.step();
        let ev = self.collect_events();
        assert!(ev.flow.is_finite());
        self.check_invariants();
    }
}

/// Position `u` cells along the closed rectangle loop `(x0,y0)-(x1,y1)`.
fn loop_pos(u: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> (f32, f32) {
    let (w, h) = (x1 - x0, y1 - y0);
    let l = 2.0 * (w + h);
    let mut d = u.rem_euclid(l);
    if d < w {
        return (x0 + d, y0);
    }
    d -= w;
    if d < h {
        return (x1, y0 + d);
    }
    d -= h;
    if d < w {
        return (x1 - d, y1);
    }
    d -= w;
    (x0, y1 - d)
}

/// Scripted mini snake looping around a small board. Pure function of `time`.
pub fn draw_snake_preview(rect: Rect, time: f32, theme: &Theme) {
    const COLS: f32 = 16.0;
    const ROWS: f32 = 9.0;
    let cell = (rect.w / COLS).min(rect.h / ROWS).floor().max(4.0);
    let (w, h) = (cell * COLS, cell * ROWS);
    let origin = vec2(rect.x + (rect.w - w) * 0.5, rect.y + (rect.h - h) * 0.5);
    draw_rectangle(origin.x, origin.y, w, h, theme.well);
    draw_rectangle_lines(
        origin.x,
        origin.y,
        w,
        h,
        2.0,
        with_alpha(theme.accent2, 0.4),
    );
    for y in 0..ROWS as i32 {
        for x in 0..COLS as i32 {
            if (x + y) % 2 == 0 {
                draw_rectangle(
                    origin.x + x as f32 * cell,
                    origin.y + y as f32 * cell,
                    cell,
                    cell,
                    with_alpha(WHITE, 0.025),
                );
            }
        }
    }
    // A few static obstacles in the middle of the loop.
    for x in 6..10 {
        crate::titles::blocks::mino::mino(
            origin.x + x as f32 * cell,
            origin.y + 4.0 * cell,
            cell,
            mix(theme.text_dim, theme.panel_border, 0.4),
            None,
            false,
        );
    }
    let (x0, y0, x1, y1) = (1.0, 1.0, COLS - 2.0, ROWS - 2.0);
    let head_u = time * 6.5;
    let m = (head_u / 12.0).floor() + 1.0;
    let apple_u = m * 12.0;
    let (ax, ay) = loop_pos(apple_u, x0, y0, x1, y1);
    let ac = origin + vec2(ax + 0.5, ay + 0.5) * cell;
    draw_apple(ac, cell, theme.pieces[4], time);
    // A ring where the previous apple was just eaten.
    let since = (head_u - (m - 1.0) * 12.0) / 6.5;
    if (m - 1.0) > 0.0 && since < 0.4 {
        let (px, py) = loop_pos((m - 1.0) * 12.0, x0, y0, x1, y1);
        let pc = origin + vec2(px + 0.5, py + 0.5) * cell;
        render::draw_ring(
            pc,
            cell * (0.6 + 2.0 * since),
            1.0,
            2.0,
            with_alpha(theme.accent2, 1.0 - since / 0.4),
        );
    }
    let n = 10;
    let pts: Vec<RenderPoint> = (0..n)
        .map(|k| {
            let (x, y) = loop_pos(head_u - k as f32, x0, y0, x1, y1);
            RenderPoint {
                x,
                y,
                linked: k > 0,
            }
        })
        .collect();
    let style = SnakeStyle::from_theme(theme, false, false);
    draw_snake(&pts, origin, cell, &style, Some((ax, ay)), (1.0, 0.0), time);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(keys: &[Key]) -> ArcadeInput {
        ArcadeInput::tap(keys)
    }

    #[test]
    fn keys_steer_with_arrows_and_wasd() {
        let mut t = SnakeTitle::new();
        t.start("portal", 5);
        t.update(0.0, &tap(&[Key::W]));
        assert_eq!(t.game().planned_turns(), 1);
        // Fast double input: Up then Down must not queue a reversal.
        t.update(0.0, &tap(&[Key::Down]));
        assert_eq!(t.game().planned_turns(), 1);
        t.update(0.0, &tap(&[Key::A]));
        assert_eq!(t.game().planned_turns(), 2);
    }

    #[test]
    fn held_keys_do_not_retrigger_turns() {
        let mut t = SnakeTitle::new();
        t.start("portal", 5);
        t.update(0.0, &ArcadeInput::new([Key::Up], []));
        assert_eq!(t.game().planned_turns(), 0, "only fresh presses count");
    }

    #[test]
    fn boost_needs_the_setting_and_the_key() {
        let run = |boost_setting: bool, keys: &[Key]| {
            let mut t = SnakeTitle::new();
            let mut ctx = TitleContext::default();
            ctx.settings.snake_boost = boost_setting;
            t.set_context(&ctx);
            t.start("portal", 5);
            t.update(0.016, &ArcadeInput::new(keys.iter().copied(), []));
            t.game().speed()
        };
        let base = steps_per_second(0);
        assert!((run(true, &[]) - base).abs() < 1e-4);
        assert!(run(true, &[Key::Space]) > base * 1.5);
        assert!(run(true, &[Key::Shift]) > base * 1.5);
        assert!(
            (run(false, &[Key::Space]) - base).abs() < 1e-4,
            "setting off"
        );
    }

    #[test]
    fn crash_reports_session_end_once_with_stats() {
        let mut t = SnakeTitle::new();
        t.start("classic", 5);
        let mut ends = Vec::new();
        for _ in 0..600 {
            let ev = t.update(1.0 / 30.0, &ArcadeInput::EMPTY);
            if let Some(e) = ev.session_end {
                ends.push(e);
            }
        }
        assert_eq!(
            ends.len(),
            1,
            "driving straight into the wall ends the run once"
        );
        let e = &ends[0];
        assert_eq!(e.title, "HIT THE WALL");
        assert!(!e.won && e.counts_for_record && !e.lower_is_better);
        assert!(e.summary.iter().any(|(k, _)| k == "LENGTH"));
    }

    #[test]
    fn zen_never_ends_the_session() {
        let mut t = SnakeTitle::new();
        t.start("zen", 5);
        for _ in 0..900 {
            let ev = t.update(1.0 / 30.0, &ArcadeInput::EMPTY);
            assert!(ev.session_end.is_none());
        }
        assert!(t.game().is_playing());
    }

    #[test]
    fn demo_bot_plays_without_input() {
        let mut t = SnakeTitle::new();
        t.start("classic", 11);
        for _ in 0..1200 {
            let ev = t.demo_update(1.0 / 60.0).expect("snake supports a demo");
            assert!(ev.session_end.is_none(), "the bot must not crash");
        }
        assert!(t.game().apples_eaten >= 3);
    }

    #[test]
    fn smoke_steps_hold_invariants_in_every_mode() {
        for m in MODES {
            let mut t = SnakeTitle::new();
            t.start(m.id, 20261116);
            for tick in 0..600 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().apples_eaten >= 5, "{}: bot should eat", m.id);
        }
    }

    #[test]
    fn loop_path_is_closed_and_unit_speed() {
        let (x0, y0, x1, y1) = (1.0, 1.0, 14.0, 7.0);
        let l = 2.0 * ((x1 - x0) + (y1 - y0));
        assert_eq!(loop_pos(0.0, x0, y0, x1, y1), loop_pos(l, x0, y0, x1, y1));
        let (ax, ay) = loop_pos(5.0, x0, y0, x1, y1);
        let (bx, by) = loop_pos(5.1, x0, y0, x1, y1);
        assert!(((bx - ax).abs() + (by - ay).abs() - 0.1).abs() < 1e-4);
    }

    #[test]
    fn metadata_is_complete() {
        let t = SnakeTitle::new();
        assert_eq!(t.id(), "snake");
        assert_eq!(t.modes().len(), 4);
        assert!(t.modes().iter().any(|m| m.is_daily));
        assert!(!t.controls_legend().is_empty());
    }
}
