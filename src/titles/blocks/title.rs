//! `ArcadeTitle` adapter of VibeCoded Blocks: maps arcade input to the
//! engine's buttons, turns game events into `FrameEvents` and draws the HUD.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas,
};
use crate::core::board::{WIDTH, is_valid_position};
use crate::core::rng::SplitMix64;
use crate::core::{FlowTier, GameEvent, GameMode, GameStatus, PieceKind, Rotation, TopOut};
use crate::engine::autopilot::AutoPilot;
use crate::engine::{Button, ButtonSet, Simulation};
use crate::presentation::digits;
use crate::presentation::music;
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, themes, with_alpha};

use super::fx::{Fx, LOCK_FLASH_SECS, ROW_FLASH_SECS};
use super::hud::{self, BOARD_W, BOARD_X, BOARD_Y, CELL, HudCtx};
use super::mino;

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new("marathon", "MARATHON", "Endless. Level up every 10 lines."),
    GameModeInfo::new(
        "sprint",
        "40-LINE SPRINT",
        "Clear 40 lines as fast as you can.",
    )
    .timed(),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "200-line flow run. One seed for everyone.",
    )
    .daily(),
    GameModeInfo::new("zen", "ZEN", "No game over. Just vibes."),
];

const LEGEND: [(&str, &str); 9] = [
    ("LEFT / RIGHT", "Move"),
    ("DOWN", "Soft drop"),
    ("SPACE", "Hard drop"),
    ("UP / X", "Rotate right"),
    ("Y / Z / CTRL", "Rotate left"),
    ("A", "Rotate 180"),
    ("C / SHIFT", "Hold"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

fn mode_of(id: &str) -> GameMode {
    match id {
        "sprint" => GameMode::Sprint,
        "daily" => GameMode::DailyVibe,
        "zen" => GameMode::Zen,
        _ => GameMode::Marathon,
    }
}

/// Arcade keys -> engine buttons (QWERTZ and QWERTY: `Y` and `Z` both rotate left).
pub fn buttons_from(input: &ArcadeInput) -> ButtonSet {
    let mut set = ButtonSet::EMPTY;
    set.set(Button::Left, input.down(Key::Left));
    set.set(Button::Right, input.down(Key::Right));
    set.set(Button::SoftDrop, input.down(Key::Down));
    set.set(Button::HardDrop, input.down(Key::Space));
    set.set(Button::RotateCw, input.any_down(&[Key::Up, Key::X]));
    set.set(
        Button::RotateCcw,
        input.any_down(&[Key::Y, Key::Z, Key::Ctrl]),
    );
    set.set(Button::Rotate180, input.down(Key::A));
    set.set(Button::Hold, input.any_down(&[Key::C, Key::Shift]));
    set
}

pub struct BlocksTitle {
    sim: Simulation,
    mode: GameMode,
    fx: Fx,
    ctx: TitleContext,
    themes: [Theme; 2],
    melody: usize,
    reported: bool,
    demo_bot: Option<AutoPilot>,
    smoke_bot: AutoPilot,
    smoke_last: (u64, u32),
}

impl Default for BlocksTitle {
    fn default() -> Self {
        Self::new()
    }
}

impl BlocksTitle {
    pub fn new() -> Self {
        let ctx = TitleContext::default();
        BlocksTitle {
            sim: Simulation::new(GameMode::Marathon, 1, ctx.settings.handling()),
            mode: GameMode::Marathon,
            fx: Fx::new(),
            ctx,
            themes: themes(),
            melody: 0,
            reported: false,
            demo_bot: None,
            smoke_bot: AutoPilot::new(true),
            smoke_last: (0, 0),
        }
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.ctx.settings.theme % self.themes.len()]
    }

    /// Read-only access for tests and tooling.
    pub fn game(&self) -> &crate::core::GameState {
        &self.sim.game
    }

    fn step(&mut self, held: ButtonSet, dt: f32) -> FrameEvents {
        self.sim.handling = self.ctx.settings.handling();
        let out = self.sim.update(held, dt);
        self.fx.update(dt);
        let mut ev = FrameEvents::default();
        self.handle_events(&out.events, &mut ev);
        let g = &self.sim.game;
        ev.flow = g.flow.flow;
        ev.music_stage = music::stage_for(g.mode, g.level, g.lines);
        if !g.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.sim.game;
        let completed = g.status == GameStatus::Finished;
        let title = match g.status {
            GameStatus::Finished => "CLEAR!",
            GameStatus::GameOver(TopOut::BlockOut) => "BLOCK OUT",
            GameStatus::GameOver(TopOut::LockOut) => "LOCK OUT",
            GameStatus::Playing => "PAUSED",
        };
        let timed = g.mode.ranked_by_time();
        let time_ms = (g.elapsed * 1000.0) as u64;
        let mut summary = vec![
            ("LINES".to_string(), g.lines.to_string()),
            ("LEVEL".to_string(), g.level.to_string()),
            ("PIECES".to_string(), g.pieces_placed.to_string()),
        ];
        if timed {
            summary.push(("SCORE".to_string(), g.score.score.to_string()));
        } else {
            summary.push(("TIME".to_string(), digits::format_time(g.elapsed)));
        }
        SessionEnd {
            title,
            won: completed,
            headline: if timed {
                ("TIME", digits::format_time(g.elapsed))
            } else {
                ("SCORE", g.score.score.to_string())
            },
            value: if timed { time_ms } else { g.score.score },
            lower_is_better: timed,
            counts_for_record: !timed || completed,
            summary,
        }
    }

    /// Turns game events into juice and sound.
    fn handle_events(&mut self, events: &[GameEvent], ev: &mut FrameEvents) {
        let theme = self.theme().clone();
        let tier = self.sim.game.tier().index();
        let callout_pos = vec2(BOARD_X + BOARD_W * 0.5, BOARD_Y + 210.0);

        for e in events {
            match e {
                GameEvent::Moved => ev.sound("move", 0.35),
                GameEvent::Rotated { kick_index } => {
                    ev.sound("rotate", if *kick_index > 0 { 0.55 } else { 0.4 });
                }
                GameEvent::Held(_) => ev.sound("hold", 0.55),
                GameEvent::HardDropped { from, to, distance } => {
                    let col = theme.piece(from.kind);
                    for ((fx_, fy), (_, ty)) in from.minos().iter().zip(to.minos().iter()) {
                        let top = hud::cell_pos(*fx_, (*fy).max(19), 0.0).y;
                        let bottom = hud::cell_pos(*fx_, *ty, 0.0).y;
                        if bottom > top {
                            self.fx.trail(
                                BOARD_X + *fx_ as f32 * CELL + 2.0,
                                top,
                                bottom,
                                CELL - 4.0,
                                col,
                            );
                        }
                    }
                    self.fx.spring.impact(*distance);
                    ev.sound("harddrop", 0.75);
                }
                GameEvent::Locked { piece, .. } => {
                    for (x, y) in piece.minos() {
                        let p = hud::cell_pos(x, y, 0.0);
                        self.fx.flash(
                            Rect::new(p.x, p.y, CELL, CELL),
                            LOCK_FLASH_SECS,
                            Color::new(1.0, 1.0, 1.0, 0.75),
                        );
                    }
                    self.melody = (self.melody + 1 + piece.kind.index() % 3) % 5;
                    ev.note(tier, self.melody, 0.22);
                }
                GameEvent::LinesCleared {
                    rows,
                    result,
                    label,
                } => {
                    for &row in rows {
                        let y = row as i32;
                        let p = hud::cell_pos(0, y, 0.0);
                        self.fx.flash(
                            Rect::new(p.x, p.y, BOARD_W, CELL),
                            ROW_FLASH_SECS,
                            Color::new(1.0, 1.0, 1.0, 0.85),
                        );
                        for x in 0..10 {
                            let c = theme.pieces[(x as usize + row) % 7];
                            ev.burst(
                                hud::cell_center(x, y),
                                c,
                                5,
                                260.0 + 40.0 * result.lines as f32,
                            );
                        }
                    }
                    let big = result.lines >= 4 || result.tspin;
                    if let Some(l) = label {
                        let col = if big { theme.accent2 } else { theme.text };
                        let size = if big { Size::X3 } else { Size::X2 };
                        ev.callout(*l, callout_pos, col, size);
                    }
                    if result.back_to_back {
                        ev.callout("BACK-TO-BACK", callout_pos, theme.accent, Size::X1);
                    }
                    if result.combo > 0 {
                        ev.callout(
                            format!("COMBO x{}", result.combo),
                            callout_pos,
                            theme.accent2,
                            Size::X1,
                        );
                    }
                    let idx = (2 + result.lines as usize + result.combo.max(0) as usize).min(7);
                    ev.note(tier, idx, 0.8);
                    if big {
                        ev.sound("sparkle", 0.7);
                        ev.flash(theme.accent2, 0.6);
                    }
                }
                GameEvent::TSpinNoLines { .. } => {
                    ev.callout("T-SPIN!", callout_pos, theme.accent, Size::X2);
                    ev.sound("sparkle", 0.5);
                }
                GameEvent::LevelUp(l) => {
                    ev.callout(
                        format!("LEVEL {l}"),
                        callout_pos + vec2(0.0, 120.0),
                        theme.text,
                        Size::X2,
                    );
                    ev.sound("levelup", 0.6);
                }
                GameEvent::TierChanged { from, to } => {
                    if to > from {
                        let c = theme.tiers[to.index()];
                        if *to == FlowTier::Overdrive {
                            ev.callout(
                                "OVERDRIVE 2.0x",
                                callout_pos - vec2(0.0, 70.0),
                                c,
                                Size::X3,
                            );
                            ev.flash(c, 1.0);
                            ev.sound("overdrive", 0.8);
                        } else {
                            let txt = format!("{} {}x", to.name(), to.multiplier());
                            ev.callout(txt, callout_pos - vec2(0.0, 70.0), c, Size::X2);
                        }
                    }
                }
                GameEvent::TopOut(_) => ev.sound("gameover", 0.8),
                GameEvent::ZenReset { cleared } => {
                    for &(x, y, kind) in cleared {
                        if y >= 19 {
                            // Bottom rows dissolve first: a soft upward wave.
                            let delay = (39 - y) as f32 * 0.035;
                            self.fx.dissolve(
                                hud::cell_pos(x, y, 0.0),
                                theme.piece(kind),
                                kind,
                                delay,
                            );
                        }
                    }
                    ev.callout("ZEN RESET", callout_pos, theme.accent2, Size::X2);
                    ev.sound("zen", 0.7);
                }
                GameEvent::Finished => {
                    ev.callout("COMPLETE!", callout_pos, theme.accent2, Size::X3);
                    ev.sound("finish", 0.8);
                }
                GameEvent::Spawned(_) | GameEvent::SoftDropped(_) => {}
            }
        }
    }

    /// Invariants checked on every headless bot tick.
    fn check_invariants(&mut self) {
        let g = &self.sim.game;
        assert!(
            is_valid_position(&g.board, &g.active),
            "active piece overlaps the matrix"
        );
        assert!(
            (0.0..=crate::core::flow::FLOW_MAX).contains(&g.flow.flow),
            "flow out of range"
        );
        assert!(g.score.score >= self.smoke_last.0, "score decreased");
        assert!(g.lines >= self.smoke_last.1, "line count decreased");
        assert!(
            g.lock_reset_count <= crate::core::game::MAX_LOCK_RESETS,
            "lock reset budget exceeded"
        );
        assert!(g.board.full_rows().is_empty(), "uncleared full row");
        if g.mode != GameMode::Zen {
            assert_eq!(
                g.board.occupied_count() as u32,
                4 * g.pieces_placed - 10 * g.lines,
                "cell conservation violated"
            );
        }
        assert!(g.ghost().y >= g.active.y, "ghost above active piece");
        assert_eq!(g.next_queue(4).len(), 4, "preview queue too short");
        self.smoke_last = (g.score.score, g.lines);
    }
}

impl ArcadeTitle for BlocksTitle {
    fn id(&self) -> &'static str {
        "blocks"
    }

    fn title(&self) -> &'static str {
        "VIBECODED BLOCKS"
    }

    fn tagline(&self) -> &'static str {
        "a flow-state block puzzle"
    }

    fn accent_color(&self) -> Color {
        hex(0x3EE6F5)
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.mode = mode_of(mode_id);
        self.sim = Simulation::new(self.mode, seed, self.ctx.settings.handling());
        self.fx.clear();
        self.melody = 0;
        self.reported = false;
        self.demo_bot = None;
        self.smoke_bot = AutoPilot::new(true);
        self.smoke_last = (0, 0);
    }

    fn set_context(&mut self, ctx: &TitleContext) {
        self.ctx = ctx.clone();
    }

    fn on_resume(&mut self, input: &ArcadeInput) {
        self.sim.latch_input(buttons_from(input));
    }

    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        self.step(buttons_from(input), dt)
    }

    fn demo_update(&mut self, dt: f32) -> Option<FrameEvents> {
        let bot = self.demo_bot.get_or_insert_with(|| AutoPilot::relaxed(4));
        let held = bot.next_input(&self.sim.game);
        Some(self.step(held, dt))
    }

    fn draw(&self, canvas: &VirtualCanvas, theme: &Theme, colorblind: bool) {
        let today = format!("#{}", self.ctx.today_iso);
        let ctx = HudCtx {
            stage: canvas,
            theme,
            colorblind,
            pulse: self.ctx.pulse,
            best: self.ctx.best,
            daily_label: &today,
        };
        hud::draw(&self.sim.game, &ctx, &self.fx);
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        draw_blocks_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        if self.sim.game.status == GameStatus::Finished {
            // A completed run (e.g. a finished sprint) restarts cleanly ...
            let id = self.mode.key();
            self.start(id, 0xC0FFEE + tick as u64);
        }
        // ... but a top-out of the bot is a regression and must fail loudly.
        assert!(
            self.sim.game.is_playing(),
            "bot topped out at tick {tick}: {:?}",
            self.sim.game.status
        );
        let held = self.smoke_bot.next_input(&self.sim.game);
        let ev = self.step(held, 1.0 / 60.0);
        assert!(ev.flow.is_finite());
        if self.sim.game.is_playing() {
            self.check_invariants();
        }
    }
}

/// Mini board with a pre-stacked floor and a piece that keeps falling.
/// Pure function of `time` (no state, no randomness beyond fixed seeds).
pub fn draw_blocks_preview(rect: Rect, time: f32, theme: &Theme) {
    const ROWS: usize = 16;
    const STACK: usize = 4;
    let cell = (rect.w / WIDTH as f32)
        .min(rect.h / ROWS as f32)
        .floor()
        .max(2.0);
    let (w, h) = (cell * WIDTH as f32, cell * ROWS as f32);
    let (ox, oy) = (rect.x + (rect.w - w) * 0.5, rect.y + (rect.h - h) * 0.5);
    draw_rectangle(ox, oy, w, h, theme.well);
    draw_rectangle_lines(ox, oy, w, h, 2.0, with_alpha(theme.accent2, 0.4));

    // Stacked rows: every row has one gap, deterministic colours.
    let mut rng = SplitMix64::new(0xB10C);
    let mut gaps = [0usize; STACK];
    for g in gaps.iter_mut() {
        *g = rng.next_below(WIDTH as u64) as usize;
    }
    for (r, gap) in gaps.iter().enumerate() {
        let row = ROWS - STACK + r;
        for x in 0..WIDTH {
            if x != *gap {
                let c = theme.pieces[(x + r * 3) % 7];
                mino::mino(
                    ox + x as f32 * cell,
                    oy + row as f32 * cell,
                    cell,
                    c,
                    None,
                    false,
                );
            }
        }
    }

    // Falling piece: one piece per period; it rests briefly after landing.
    const PERIOD: f32 = 2.6;
    let n = (time / PERIOD).floor();
    let phase = (time / PERIOD).fract();
    let idx = n as i64;
    let kind = PieceKind::ALL[idx.rem_euclid(7) as usize];
    let cells = kind.cells(Rotation::Spawn);
    let max_dy = cells.iter().map(|c| c.1).max().unwrap_or(0);
    let max_dx = cells.iter().map(|c| c.0).max().unwrap_or(0);
    let px = ((idx * 3).rem_euclid(WIDTH as i64 - max_dx as i64)) as i32;
    let land_top = (ROWS - STACK) as i32 - 1 - max_dy;
    let fall = (phase / 0.7).min(1.0);
    let top = (-4.0 + (land_top as f32 + 4.0) * fall).floor() as i32;
    let color = theme.piece(kind);
    for (dx, dy) in cells {
        let (cx, cy) = (px + dx, top + dy);
        if cy >= 0 {
            mino::mino(
                ox + cx as f32 * cell,
                oy + cy as f32 * cell,
                cell,
                color,
                Some(kind),
                false,
            );
        }
    }
    if phase > 0.7 {
        // Landing flash.
        let k = 1.0 - (phase - 0.7) / 0.3;
        draw_rectangle(
            ox,
            oy + (land_top + max_dy) as f32 * cell,
            w,
            cell,
            with_alpha(WHITE, 0.25 * k),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every binding of the original `input_map::BINDINGS` (pause excluded:
    /// the arcade controller owns it).
    const BINDINGS: [(Button, &[Key]); 8] = [
        (Button::Left, &[Key::Left]),
        (Button::Right, &[Key::Right]),
        (Button::SoftDrop, &[Key::Down]),
        (Button::HardDrop, &[Key::Space]),
        (Button::RotateCw, &[Key::Up, Key::X]),
        (Button::RotateCcw, &[Key::Y, Key::Z, Key::Ctrl]),
        (Button::Rotate180, &[Key::A]),
        (Button::Hold, &[Key::C, Key::Shift]),
    ];

    #[test]
    fn every_original_key_binding_maps_to_exactly_its_button() {
        for (button, keys) in BINDINGS {
            for &key in keys {
                let set = buttons_from(&ArcadeInput::tap(&[key]));
                for (other, _) in BINDINGS {
                    assert_eq!(
                        set.contains(other),
                        other == button,
                        "{key:?} must trigger {button:?} and nothing else (saw {other:?})"
                    );
                }
            }
        }
        // QWERTZ and QWERTY: both Y and Z rotate left.
        assert!(buttons_from(&ArcadeInput::tap(&[Key::Y])).contains(Button::RotateCcw));
        assert!(buttons_from(&ArcadeInput::tap(&[Key::Z])).contains(Button::RotateCcw));
        // Held keys combine.
        let b = buttons_from(&ArcadeInput::new([Key::Left, Key::Down], []));
        assert!(b.contains(Button::Left) && b.contains(Button::SoftDrop));
    }

    /// The assertions of the original headless smoke test, now through the
    /// `ArcadeTitle` interface: same seed, same bot, 500 ticks.
    #[test]
    fn bot_run_keeps_the_original_smoke_test_guarantees() {
        let mut t = BlocksTitle::new();
        t.start("marathon", 0x00C0_FFEE);
        let mut bot = AutoPilot::new(true);
        let (mut moves, mut rots, mut holds, mut drops) = (0, 0, 0, 0);
        let mut peak_flow: f32 = 0.0;
        for tick in 0..500 {
            assert!(t.game().is_playing(), "bot topped out at tick {tick}");
            let held = bot.next_input(t.game());
            let ev = t.step(held, 1.0 / 60.0);
            for c in &ev.audio {
                if let crate::arcade::AudioCue::Sound { name, .. } = c {
                    match *name {
                        "move" => moves += 1,
                        "rotate" => rots += 1,
                        "hold" => holds += 1,
                        "harddrop" => drops += 1,
                        _ => {}
                    }
                }
            }
            peak_flow = peak_flow.max(ev.flow);
            t.check_invariants();
        }
        let g = t.game();
        assert!(g.pieces_placed >= 10, "too few pieces: {}", g.pieces_placed);
        assert!(g.lines >= 1, "no line clear happened");
        assert!(moves > 0, "no horizontal move");
        assert!(rots > 0, "no rotation");
        assert!(holds >= 1, "hold never used");
        assert!(drops >= 10, "too few hard drops: {drops}");
        assert!(peak_flow > 0.0, "flow never rose");
    }

    #[test]
    fn bot_plays_all_modes_through_the_title_interface() {
        // Strict: simulate_smoke_step panics if the bot tops out.
        for m in MODES {
            let mut t = BlocksTitle::new();
            t.start(m.id, 0x00C0_FFEE);
            for tick in 0..500 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().pieces_placed >= 10, "{}: too few pieces", m.id);
        }
    }

    #[test]
    fn session_end_is_reported_exactly_once() {
        let mut t = BlocksTitle::new();
        t.start("marathon", 7);
        let mut ends = 0;
        // Slam pieces straight down until the stack tops out.
        for _ in 0..400 {
            let ev = t.update(1.0 / 60.0, &ArcadeInput::tap(&[Key::Space]));
            ends += ev.session_end.is_some() as u32;
            let _ = t.update(1.0 / 60.0, &ArcadeInput::EMPTY);
        }
        assert!(!t.game().is_playing());
        assert_eq!(ends, 1);
    }

    #[test]
    fn sprint_end_is_timed_and_unfinished_runs_do_not_count() {
        let mut t = BlocksTitle::new();
        t.start("sprint", 7);
        for _ in 0..400 {
            t.update(1.0 / 60.0, &ArcadeInput::tap(&[Key::Space]));
            t.update(1.0 / 60.0, &ArcadeInput::EMPTY);
        }
        let end = t.session_end();
        assert!(end.lower_is_better);
        assert!(!end.counts_for_record, "topped out before 40 lines");
        assert_eq!(end.headline.0, "TIME");
    }
}
