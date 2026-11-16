//! Application state machine: menu, playing, pause and results screens.

use macroquad::prelude::*;

use crate::core::date::Date;
use crate::core::{FlowTier, GameEvent, GameMode, GameStatus, TopOut};
use crate::engine::Simulation;
use crate::engine::autopilot::AutoPilot;
use crate::infrastructure::clock;
use crate::infrastructure::save::{RunRecord, SaveData, SaveStore, Settings};

use super::audio::Audio;
use super::background::Background;
use super::blocks;
use super::digits;
use super::fx::{Fx, LOCK_FLASH_SECS, ROW_FLASH_SECS};
use super::hud::{self, BOARD_W, BOARD_X, BOARD_Y, CELL, HudCtx};
use super::input_map::{held_buttons, pressed_any};
use super::stage::{Stage, VIRTUAL_H, VIRTUAL_W};
use super::text::{self, Align, Size};
use super::theme::{Theme, lighten, themes, with_alpha};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Menu,
    Playing,
    Paused,
    Results,
}

#[derive(Clone, Debug)]
struct ResultInfo {
    title: &'static str,
    score: u64,
    lines: u32,
    seconds: f64,
    new_record: bool,
}

/// Interactive settings rows on the start screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingRow {
    Das,
    Arr,
    Volume,
    Sound,
    Theme,
    Colorblind,
}

const SETTING_ROWS: [SettingRow; 6] = [
    SettingRow::Das,
    SettingRow::Arr,
    SettingRow::Volume,
    SettingRow::Sound,
    SettingRow::Theme,
    SettingRow::Colorblind,
];

/// Seconds the final board stays visible before the results overlay.
const END_DELAY: f32 = 0.9;
/// Menu idle time before the attract-mode demo starts.
const ATTRACT_DELAY: f32 = 25.0;

pub struct App {
    screen: Screen,
    store: SaveStore,
    save: SaveData,
    themes: [Theme; 2],
    sim: Option<Simulation>,
    mode: GameMode,
    menu_sel: usize,
    fx: Fx,
    audio: Audio,
    bg: Background,
    today: Date,
    end_timer: f32,
    result: Option<ResultInfo>,
    stage: Stage,
    screen_time: f32,
    melody: usize,
    /// Attract mode: the bot plays instead of the keyboard.
    demo: Option<AutoPilot>,
    idle: f32,
    last_mouse: Vec2,
}

impl App {
    pub fn new(start_demo: bool) -> App {
        let store = SaveStore::default_location();
        let save = store.load();
        let audio = Audio::load(save.settings.muted, save.settings.volume as f32 / 100.0);
        App {
            screen: Screen::Menu,
            store,
            save,
            themes: themes(),
            sim: None,
            mode: GameMode::Marathon,
            menu_sel: 0,
            fx: Fx::new(),
            audio,
            bg: Background::new(),
            today: clock::today_utc(),
            end_timer: 0.0,
            result: None,
            stage: Stage::compute(),
            screen_time: 0.0,
            melody: 0,
            demo: None,
            idle: if start_demo { ATTRACT_DELAY } else { 0.0 },
            last_mouse: Vec2::ZERO,
        }
    }

    fn settings(&self) -> &Settings {
        &self.save.settings
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.save.settings.theme % self.themes.len()]
    }

    fn persist(&self) {
        // Failure to save (read-only profile etc.) must never crash the game.
        let _ = self.store.save(&self.save);
    }

    fn apply_audio_settings(&mut self) {
        self.audio.muted = self.save.settings.muted;
        self.audio.volume = self.save.settings.volume as f32 / 100.0;
    }

    fn set_screen(&mut self, s: Screen) {
        self.screen = s;
        self.screen_time = 0.0;
    }

    fn start_game(&mut self, mode: GameMode) {
        self.demo = None;
        self.idle = 0.0;
        self.mode = mode;
        self.today = clock::today_utc();
        let seed = match mode {
            GameMode::DailyVibe => self.today.seed(),
            _ => clock::session_seed(),
        };
        let mut sim = Simulation::new(mode, seed, self.settings().handling());
        sim.latch_input(held_buttons());
        self.sim = Some(sim);
        self.fx.clear();
        self.result = None;
        self.end_timer = 0.0;
        self.audio.play("select", 0.6);
        self.set_screen(Screen::Playing);
    }

    fn start_demo(&mut self) {
        self.start_game(GameMode::Marathon);
        self.demo = Some(AutoPilot::relaxed(4));
    }

    fn any_user_input(&mut self) -> bool {
        let m = mouse_position();
        let moved = (vec2(m.0, m.1) - self.last_mouse).length() > 2.0;
        self.last_mouse = vec2(m.0, m.1);
        get_last_key_pressed().is_some() || is_mouse_button_pressed(MouseButton::Left) || moved
    }

    // ------------------------------------------------------------------ update

    pub fn update(&mut self, dt: f32) {
        self.stage = Stage::compute();
        self.screen_time += dt;
        self.handle_global_keys();

        let tier = self
            .sim
            .as_ref()
            .filter(|_| self.screen != Screen::Menu)
            .map(|s| s.game.tier())
            .unwrap_or(FlowTier::Chill);
        let bg_dt = if self.screen == Screen::Paused {
            dt * 0.25
        } else {
            dt
        };
        self.bg.update(bg_dt, tier.bpm());
        if self.screen != Screen::Paused {
            self.fx.update(dt);
        }

        match self.screen {
            Screen::Menu => self.update_menu(dt),
            Screen::Playing => self.update_playing(dt),
            Screen::Paused => self.update_paused(),
            Screen::Results => self.update_results(),
        }
    }

    fn handle_global_keys(&mut self) {
        let mut changed = false;
        if is_key_pressed(KeyCode::T) {
            self.save.settings.theme = (self.save.settings.theme + 1) % self.themes.len();
            changed = true;
        }
        if is_key_pressed(KeyCode::B) {
            self.save.settings.colorblind = !self.save.settings.colorblind;
            changed = true;
        }
        if is_key_pressed(KeyCode::M) {
            self.save.settings.muted = !self.save.settings.muted;
            changed = true;
        }
        if changed {
            self.apply_audio_settings();
            self.audio.play("select", 0.5);
            self.persist();
        }
    }

    fn update_menu(&mut self, dt: f32) {
        if self.any_user_input() {
            self.idle = 0.0;
        } else {
            self.idle += dt;
        }
        if self.idle >= ATTRACT_DELAY {
            self.start_demo();
            return;
        }
        let n = GameMode::ALL.len();
        if pressed_any(&[KeyCode::Down, KeyCode::S]) {
            self.menu_sel = (self.menu_sel + 1) % n;
            self.audio.play("move", 0.6);
        }
        if pressed_any(&[KeyCode::Up, KeyCode::W]) {
            self.menu_sel = (self.menu_sel + n - 1) % n;
            self.audio.play("move", 0.6);
        }
        for (i, k) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4]
            .iter()
            .enumerate()
        {
            if is_key_pressed(*k) {
                self.start_game(GameMode::ALL[i]);
                return;
            }
        }
        if pressed_any(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]) {
            self.start_game(GameMode::ALL[self.menu_sel]);
            return;
        }

        let m = self.stage.mouse();
        for i in 0..n {
            if mode_card_rect(i).contains(m) {
                if self.menu_sel != i {
                    self.menu_sel = i;
                }
                if is_mouse_button_pressed(MouseButton::Left) {
                    self.start_game(GameMode::ALL[i]);
                    return;
                }
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            self.click_settings(m);
        }
    }

    fn click_settings(&mut self, m: Vec2) {
        let mut changed = false;
        for (i, row) in SETTING_ROWS.iter().enumerate() {
            let (minus, plus, toggle) = setting_rects(i);
            let s = &mut self.save.settings;
            let step = |v: u32, d: i32, lo: u32, hi: u32| {
                (v as i32 + d).clamp(lo as i32, hi as i32) as u32
            };
            match row {
                SettingRow::Das | SettingRow::Arr | SettingRow::Volume => {
                    let d = if minus.contains(m) {
                        -1
                    } else if plus.contains(m) {
                        1
                    } else {
                        continue;
                    };
                    match row {
                        SettingRow::Das => {
                            s.das_ms = step(
                                s.das_ms,
                                d * 10,
                                Settings::DAS_RANGE.0,
                                Settings::DAS_RANGE.1,
                            )
                        }
                        SettingRow::Arr => {
                            s.arr_ms = step(
                                s.arr_ms,
                                d * 5,
                                Settings::ARR_RANGE.0,
                                Settings::ARR_RANGE.1,
                            )
                        }
                        _ => s.volume = step(s.volume, d * 10, 0, 100),
                    }
                    changed = true;
                }
                _ if toggle.contains(m) => {
                    match row {
                        SettingRow::Sound => s.muted = !s.muted,
                        SettingRow::Theme => s.theme = (s.theme + 1) % self.themes.len(),
                        _ => s.colorblind = !s.colorblind,
                    }
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            self.apply_audio_settings();
            self.audio.play("select", 0.6);
            self.persist();
        }
    }

    fn update_playing(&mut self, dt: f32) {
        let Some(sim) = self.sim.as_mut() else {
            self.set_screen(Screen::Menu);
            return;
        };
        sim.handling = self.save.settings.handling();
        let input = match self.demo.as_mut() {
            Some(bot) => bot.next_input(&sim.game),
            None => held_buttons(),
        };
        let out = sim.update(input, dt);
        self.handle_events(&out.events);

        if self.demo.is_some() {
            let ended = self.sim.as_ref().is_some_and(|s| !s.game.is_playing());
            if ended {
                self.start_demo();
            } else if self.screen_time > 0.5 && self.any_user_input() {
                self.demo = None;
                self.sim = None;
                self.idle = 0.0;
                self.set_screen(Screen::Menu);
            }
            return;
        }

        let sim = self.sim.as_ref().expect("sim present");
        if !sim.game.is_playing() {
            self.end_timer += dt;
            if self.end_timer >= END_DELAY {
                self.finish_run();
            }
            return;
        }
        if out.pause_pressed {
            self.set_screen(Screen::Paused);
        }
    }

    fn update_paused(&mut self) {
        if pressed_any(&[KeyCode::P, KeyCode::Escape, KeyCode::Enter]) {
            if let Some(sim) = self.sim.as_mut() {
                sim.latch_input(held_buttons());
            }
            self.set_screen(Screen::Playing);
        } else if is_key_pressed(KeyCode::R) {
            self.start_game(self.mode);
        } else if is_key_pressed(KeyCode::Q) {
            self.sim = None;
            self.set_screen(Screen::Menu);
        }
    }

    fn update_results(&mut self) {
        if self.screen_time < 0.35 {
            return; // avoid instantly skipping with a held key
        }
        if pressed_any(&[KeyCode::R, KeyCode::Enter, KeyCode::KpEnter]) {
            self.start_game(self.mode);
        } else if pressed_any(&[KeyCode::Escape, KeyCode::Q]) {
            self.sim = None;
            self.set_screen(Screen::Menu);
        }
    }

    fn finish_run(&mut self) {
        let Some(sim) = self.sim.as_ref() else {
            return;
        };
        let g = &sim.game;
        let completed = g.status == GameStatus::Finished;
        let record = RunRecord {
            mode: g.mode,
            score: g.score.score,
            time_ms: (g.elapsed * 1000.0) as u64,
            completed,
            date_iso: self.today.iso(),
        };
        let new_record = self.save.record(&record);
        self.persist();
        let title = match g.status {
            GameStatus::Finished => "CLEAR!",
            GameStatus::GameOver(TopOut::BlockOut) => "BLOCK OUT",
            GameStatus::GameOver(TopOut::LockOut) => "LOCK OUT",
            GameStatus::Playing => "PAUSED",
        };
        self.result = Some(ResultInfo {
            title,
            score: g.score.score,
            lines: g.lines,
            seconds: g.elapsed,
            new_record,
        });
        if new_record {
            self.audio.play("levelup", 0.8);
        }
        self.set_screen(Screen::Results);
    }

    /// Turns game events into juice and sound.
    fn handle_events(&mut self, events: &[GameEvent]) {
        let theme = self.theme().clone();
        let Some(sim) = self.sim.as_ref() else {
            return;
        };
        let tier = sim.game.tier().index();
        let callout_pos = vec2(BOARD_X + BOARD_W * 0.5, BOARD_Y + 210.0);

        for e in events {
            match e {
                GameEvent::Moved => self.audio.play("move", 0.35),
                GameEvent::Rotated { kick_index } => {
                    self.audio
                        .play("rotate", if *kick_index > 0 { 0.55 } else { 0.4 });
                }
                GameEvent::Held(_) => self.audio.play("hold", 0.55),
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
                    self.audio.play("harddrop", 0.75);
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
                    self.audio.note(tier, self.melody, 0.22);
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
                            self.fx.burst(
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
                        self.fx.callout(*l, callout_pos, col, size);
                    }
                    if result.back_to_back {
                        self.fx
                            .callout("BACK-TO-BACK", callout_pos, theme.accent, Size::X1);
                    }
                    if result.combo > 0 {
                        self.fx.callout(
                            format!("COMBO x{}", result.combo),
                            callout_pos,
                            theme.accent2,
                            Size::X1,
                        );
                    }
                    let idx = (2 + result.lines as usize + result.combo.max(0) as usize).min(7);
                    self.audio.note(tier, idx, 0.8);
                    if big {
                        self.audio.play("sparkle", 0.7);
                        self.fx.screen_flash = 0.6;
                        self.fx.screen_flash_color = theme.accent2;
                    }
                }
                GameEvent::TSpinNoLines { .. } => {
                    self.fx
                        .callout("T-SPIN!", callout_pos, theme.accent, Size::X2);
                    self.audio.play("sparkle", 0.5);
                }
                GameEvent::LevelUp(l) => {
                    self.fx.callout(
                        format!("LEVEL {l}"),
                        callout_pos + vec2(0.0, 120.0),
                        theme.text,
                        Size::X2,
                    );
                    self.audio.play("levelup", 0.6);
                }
                GameEvent::TierChanged { from, to } => {
                    if to > from {
                        let c = theme.tiers[to.index()];
                        if *to == FlowTier::Overdrive {
                            self.fx.callout(
                                "OVERDRIVE 2.0x",
                                callout_pos - vec2(0.0, 70.0),
                                c,
                                Size::X3,
                            );
                            self.fx.screen_flash = 1.0;
                            self.fx.screen_flash_color = c;
                            self.audio.play("overdrive", 0.8);
                        } else {
                            let txt = format!("{} {}x", to.name(), to.multiplier());
                            self.fx
                                .callout(txt, callout_pos - vec2(0.0, 70.0), c, Size::X2);
                        }
                    }
                }
                GameEvent::TopOut(_) => self.audio.play("gameover", 0.8),
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
                    self.fx
                        .callout("ZEN RESET", callout_pos, theme.accent2, Size::X2);
                    self.audio.play("zen", 0.7);
                }
                GameEvent::Finished => {
                    self.fx
                        .callout("COMPLETE!", callout_pos, theme.accent2, Size::X3);
                    self.audio.play("finish", 0.8);
                }
                GameEvent::Spawned(_) | GameEvent::SoftDropped(_) => {}
            }
        }
    }

    // ------------------------------------------------------------------ draw

    pub fn draw(&self) {
        clear_background(BLACK);
        self.stage.apply();
        let theme = self.theme();
        let intensity = match (&self.sim, self.screen) {
            (Some(s), Screen::Playing | Screen::Paused | Screen::Results) => {
                s.game.flow.flow / 100.0
            }
            _ => 0.15,
        };
        self.bg.draw(theme, intensity);

        match self.screen {
            Screen::Menu => self.draw_menu(theme),
            Screen::Playing | Screen::Paused | Screen::Results => {
                if let Some(sim) = &self.sim {
                    let today = format!("#{}", self.today.iso());
                    let ctx = HudCtx {
                        stage: &self.stage,
                        theme,
                        colorblind: self.settings().colorblind,
                        pulse: self.bg.pulse(),
                        best: self.save.best_for(sim.game.mode, &self.today.iso()),
                        daily_label: &today,
                    };
                    hud::draw(&sim.game, &ctx, &self.fx);
                }
                self.fx.draw_overlay(&self.stage);
                if self.demo.is_some() {
                    let a = 0.55 + 0.45 * self.bg.pulse();
                    draw_rectangle(BOARD_X, 2.0, BOARD_W, 26.0, Color::new(0.0, 0.0, 0.0, 0.55));
                    text::label(
                        &self.stage,
                        "DEMO  -  PRESS ANY KEY",
                        BOARD_X + BOARD_W * 0.5,
                        7.0,
                        Size::X1,
                        with_alpha(theme.accent2, a),
                        Align::Center,
                    );
                }
                if self.screen == Screen::Paused {
                    self.draw_pause(theme);
                }
                if self.screen == Screen::Results {
                    self.draw_results(theme);
                }
            }
        }
        set_default_camera();
    }

    fn draw_menu(&self, theme: &Theme) {
        let st = &self.stage;
        let t = self.bg.time();
        // Title with animated mini blocks
        let kinds = crate::core::PieceKind::ALL;
        for (i, k) in kinds.iter().enumerate() {
            let x = 640.0 - 3.5 * 30.0 + i as f32 * 30.0;
            let y = 40.0 + (t * 2.2 + i as f32 * 0.7).sin() * 4.0;
            blocks::mino(
                x,
                y,
                24.0,
                theme.piece(*k),
                Some(*k),
                self.settings().colorblind,
            );
        }
        text::label(
            st,
            "VIBECODED BLOCKS",
            640.0,
            84.0,
            Size::X3,
            theme.text,
            Align::Center,
        );
        text::label(
            st,
            "a flow-state block puzzle",
            640.0,
            136.0,
            Size::X1,
            with_alpha(theme.accent2, 0.9),
            Align::Center,
        );

        // Mode cards
        let m = st.mouse();
        for (i, mode) in GameMode::ALL.iter().enumerate() {
            let r = mode_card_rect(i);
            let sel = i == self.menu_sel;
            let accent = if sel {
                theme.accent2
            } else {
                theme.panel_border
            };
            hud::panel(r.x, r.y, r.w, r.h, theme, accent);
            if sel {
                let a = 0.35 + 0.35 * self.bg.pulse();
                draw_rectangle_lines(
                    r.x - 3.0,
                    r.y - 3.0,
                    r.w + 6.0,
                    r.h + 6.0,
                    2.0,
                    with_alpha(theme.accent2, a),
                );
            } else if r.contains(m) {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, with_alpha(theme.text_dim, 0.6));
            }
            digits::draw(
                &(i + 1).to_string(),
                r.x + 18.0,
                r.y + 22.0,
                38.0,
                if sel { theme.accent2 } else { theme.text_dim },
            );
            text::label(
                st,
                mode.title(),
                r.x + 62.0,
                r.y + 14.0,
                Size::X2,
                theme.text,
                Align::Left,
            );
            if *mode == GameMode::DailyVibe {
                let d = format!("#{}", self.today.iso());
                digits::draw_right(&d, r.x + r.w - 16.0, r.y + 60.0, 14.0, theme.accent2);
            }
            text::label(
                st,
                mode.tagline(),
                r.x + 62.0,
                r.y + 52.0,
                Size::X1,
                theme.text_dim,
                Align::Left,
            );
            let best = self.save.best_for(*mode, &self.today.iso());
            if let Some(b) = best {
                let s = if *mode == GameMode::Sprint {
                    digits::format_time(b as f64 / 1000.0)
                } else {
                    b.to_string()
                };
                text::label(
                    st,
                    "BEST",
                    r.x + r.w - 16.0,
                    r.y + 14.0,
                    Size::X1,
                    theme.text_dim,
                    Align::Right,
                );
                digits::draw_right(&s, r.x + r.w - 16.0, r.y + 36.0, 18.0, theme.accent);
            }
        }
        text::label(
            st,
            "ENTER / CLICK to play   -   UP/DOWN or 1-4 to choose",
            400.0,
            584.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );

        // Settings panel
        let (px, py, pw, ph) = (720.0, 168.0, 400.0, 392.0);
        hud::panel(px, py, pw, ph, theme, theme.accent);
        text::label(
            st,
            "SETTINGS",
            px + 20.0,
            py + 16.0,
            Size::X2,
            theme.text,
            Align::Left,
        );
        let s = self.settings();
        for (i, row) in SETTING_ROWS.iter().enumerate() {
            let (minus, plus, toggle) = setting_rects(i);
            let y = minus.y;
            let name = match row {
                SettingRow::Das => "DAS",
                SettingRow::Arr => "ARR",
                SettingRow::Volume => "VOLUME",
                SettingRow::Sound => "SOUND",
                SettingRow::Theme => "THEME",
                SettingRow::Colorblind => "COLORBLIND",
            };
            text::label(
                st,
                name,
                px + 20.0,
                y + 8.0,
                Size::X1,
                theme.text_dim,
                Align::Left,
            );
            match row {
                SettingRow::Das | SettingRow::Arr | SettingRow::Volume => {
                    for (r, sym) in [(minus, "-"), (plus, "+")] {
                        button(st, theme, r, sym, r.contains(m));
                    }
                    let v = match row {
                        SettingRow::Das => format!("{}", s.das_ms),
                        SettingRow::Arr => format!("{}", s.arr_ms),
                        _ => format!("{}%", s.volume),
                    };
                    let cx = (minus.x + minus.w + plus.x) * 0.5;
                    digits::draw_centered(&v, cx, y + 6.0, 20.0, theme.text);
                    if *row != SettingRow::Volume {
                        text::label(
                            st,
                            "ms",
                            plus.x + plus.w + 8.0,
                            y + 8.0,
                            Size::X1,
                            theme.text_dim,
                            Align::Left,
                        );
                    }
                }
                _ => {
                    let label = match row {
                        SettingRow::Sound => self.audio.status().to_string(),
                        SettingRow::Theme => theme.name.to_string(),
                        _ => (if s.colorblind { "ON" } else { "OFF" }).to_string(),
                    };
                    button(st, theme, toggle, &label, toggle.contains(m));
                }
            }
        }
        // Controls
        let (cx, cy) = (720.0, 576.0);
        hud::panel(cx, cy, 400.0, 112.0, theme, theme.panel_border);
        let lines = [
            "LEFT/RIGHT move    DOWN soft    SPACE hard",
            "UP/X rotate   Y/Z/CTRL ccw   A 180",
            "C/SHIFT hold   P/ESC pause",
            "T theme   B colorblind   M mute",
        ];
        for (i, l) in lines.iter().enumerate() {
            text::label(
                st,
                l,
                cx + 16.0,
                cy + 12.0 + i as f32 * 23.0,
                Size::X1,
                theme.text_dim,
                Align::Left,
            );
        }
    }

    fn draw_pause(&self, theme: &Theme) {
        let st = &self.stage;
        draw_rectangle(
            0.0,
            0.0,
            VIRTUAL_W,
            VIRTUAL_H,
            Color::new(0.02, 0.03, 0.05, 0.72),
        );
        text::label(
            st,
            "PAUSED",
            640.0,
            270.0,
            Size::X3,
            theme.text,
            Align::Center,
        );
        text::label(
            st,
            "P / ESC  resume",
            640.0,
            350.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
        text::label(
            st,
            "R  restart",
            640.0,
            376.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
        text::label(
            st,
            "Q  quit to menu",
            640.0,
            402.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
    }

    fn draw_results(&self, theme: &Theme) {
        let Some(r) = &self.result else {
            return;
        };
        let st = &self.stage;
        let k = (self.screen_time / 0.3).min(1.0);
        draw_rectangle(
            0.0,
            0.0,
            VIRTUAL_W,
            VIRTUAL_H,
            Color::new(0.02, 0.03, 0.05, 0.7 * k),
        );
        let (w, h) = (440.0, 340.0);
        let (x, y) = (640.0 - w * 0.5, 190.0 + (1.0 - k) * 30.0);
        hud::panel(x, y, w, h, theme, theme.accent2);
        text::label(
            st,
            r.title,
            640.0,
            y + 24.0,
            Size::X3,
            theme.text,
            Align::Center,
        );
        text::label(
            st,
            self.mode.title(),
            640.0,
            y + 80.0,
            Size::X1,
            theme.accent2,
            Align::Center,
        );
        text::label(
            st,
            "SCORE",
            640.0,
            y + 110.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
        digits::draw_centered(&r.score.to_string(), 640.0, y + 132.0, 40.0, theme.text);
        text::label(
            st,
            "LINES",
            540.0,
            y + 192.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
        digits::draw_centered(&r.lines.to_string(), 540.0, y + 214.0, 24.0, theme.text);
        text::label(
            st,
            "TIME",
            740.0,
            y + 192.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
        digits::draw_centered(
            &digits::format_time(r.seconds),
            740.0,
            y + 214.0,
            24.0,
            theme.text,
        );
        if r.new_record {
            let a = 0.6 + 0.4 * self.bg.pulse();
            text::label(
                st,
                "NEW RECORD!",
                640.0,
                y + 256.0,
                Size::X2,
                with_alpha(lighten(theme.accent2, 0.2), a),
                Align::Center,
            );
        }
        text::label(
            st,
            "R / ENTER  retry      ESC  menu",
            640.0,
            y + h - 30.0,
            Size::X1,
            theme.text_dim,
            Align::Center,
        );
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new(false)
    }
}

fn mode_card_rect(i: usize) -> Rect {
    Rect::new(160.0, 168.0 + i as f32 * 98.0, 480.0, 84.0)
}

/// (minus, plus, toggle) rects of settings row `i`.
fn setting_rects(i: usize) -> (Rect, Rect, Rect) {
    let y = 226.0 + i as f32 * 54.0;
    (
        Rect::new(880.0, y, 32.0, 32.0),
        Rect::new(1020.0, y, 32.0, 32.0),
        Rect::new(880.0, y, 220.0, 32.0),
    )
}

fn button(stage: &Stage, theme: &Theme, r: Rect, label: &str, hover: bool) {
    let fill = if hover {
        with_alpha(theme.accent, 0.45)
    } else {
        Color::new(0.0, 0.0, 0.0, 0.35)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, fill);
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        1.5,
        if hover {
            theme.accent2
        } else {
            theme.panel_border
        },
    );
    text::label(
        stage,
        label,
        r.x + r.w * 0.5,
        r.y + 8.0,
        Size::X1,
        theme.text,
        Align::Center,
    );
}

/// Main loop.
pub async fn run(start_demo: bool) {
    let mut app = App::new(start_demo);
    loop {
        let dt = get_frame_time();
        app.update(dt);
        app.draw();
        next_frame().await;
    }
}
