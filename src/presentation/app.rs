//! `App`: the arcade controller. It owns the [`GameRegistry`], the shared
//! systems (audio, music, background shader, particles, callouts, save data)
//! and the navigation between the main menu and the games.
//!
//! It never names a concrete game: everything goes through `dyn ArcadeTitle`.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, FrameEvents, GameModeInfo, GameRegistry, Nav, Screen, Selection, SessionEnd,
    TitleContext, cue,
};
use crate::core::FlowTier;
use crate::core::date::Date;
use crate::infrastructure::Cli;
use crate::infrastructure::clock;
use crate::infrastructure::save::{SaveData, SaveStore, Settings, daily_key, score_key};
use crate::titles;

use super::audio::Audio;
use super::background::Background;
use super::digits;
use super::input_map::{poll_arcade_input, pressed_any};
use super::menu::{self, MenuAction, MenuInput, MenuState, MenuView};
use super::music::{GAME_STAGES, Track};
use super::settings::{self, SettingsInput, SettingsState};
use super::stage::{Stage, VIRTUAL_H, VIRTUAL_W};
use super::text::{self, Align, Size};
use super::theme::{Theme, themes, with_alpha};
use super::widgets::{FloatingCallouts, ModalAction, ModalKind, ModalOverlay, ParticleSystem};

/// Menu idle time before the attract-mode demo starts.
const ATTRACT_DELAY: f32 = 25.0;

/// What the results overlay shows.
#[derive(Clone, Debug)]
struct ResultInfo {
    end: SessionEnd,
    new_record: bool,
    best: Option<String>,
}

pub struct App {
    registry: GameRegistry,
    nav: Nav,
    menu: MenuState,
    settings_ui: Option<SettingsState>,
    store: SaveStore,
    save: SaveData,
    themes: [Theme; 2],
    particles: ParticleSystem,
    callouts: FloatingCallouts,
    flash: (Color, f32),
    shake: f32,
    audio: Audio,
    bg: Background,
    today: Date,
    stage: Stage,
    time: f32,
    result: Option<ResultInfo>,
    /// Attract mode: a bot plays instead of the keyboard.
    demo: bool,
    demo_game: usize,
    idle: f32,
    last_mouse: Vec2,
    flow: f32,
    music_stage: usize,
}

impl App {
    pub fn new(cli: &Cli) -> App {
        let (start_demo, start_game) = (cli.demo, cli.game.as_deref());
        let registry = titles::default_registry();
        let mut store = SaveStore::default_location();
        let save = store.load();
        if cli.screenshot.is_some() || cli.show.is_some() {
            // Visual-check runs may read the save but must never write it.
            store = SaveStore::disabled();
        }
        let audio = Audio::load(save.settings.muted, save.settings.volume as f32 / 100.0);
        let counts = mode_counts(&registry);
        let direct = start_game.and_then(|id| registry.find(id));
        let mut app = App {
            registry,
            nav: Nav::default(),
            menu: MenuState::new(&counts),
            settings_ui: None,
            store,
            save,
            themes: themes(),
            particles: ParticleSystem::new(),
            callouts: FloatingCallouts::new(),
            flash: (WHITE, 0.0),
            shake: 0.0,
            audio,
            bg: Background::new(),
            today: clock::today_utc(),
            stage: Stage::compute(),
            time: 0.0,
            result: None,
            demo: false,
            demo_game: direct.unwrap_or(0),
            idle: if start_demo { ATTRACT_DELAY } else { 0.0 },
            last_mouse: Vec2::ZERO,
            flow: 0.0,
            music_stage: 0,
        };
        if let (Some(game), false) = (direct, start_demo) {
            let mode = cli
                .mode
                .as_deref()
                .and_then(|id| {
                    app.registry
                        .get(game)?
                        .modes()
                        .iter()
                        .position(|m| m.id == id)
                })
                .unwrap_or(0);
            app.start_game(Selection { game, mode });
            if let (Some(p), Some(g)) = (cli.preset.as_deref(), app.registry.get_mut(game)) {
                g.dev_preset(p);
            }
        }
        match cli.show.as_deref() {
            Some("pause") => {
                app.nav.pause();
            }
            Some("settings") => app.settings_ui = Some(SettingsState::default()),
            // Visual check only: not persisted.
            Some("colorblind") => app.save.settings.colorblind = true,
            Some("theme2") => app.save.settings.theme = 1,
            // Menu page 2 (screenshots of the paged layout).
            Some("page2") => {
                let n = app.registry.len();
                app.menu.focus = menu::PAGE_SIZE.min(n.saturating_sub(1));
                app.menu.page = 1;
            }
            _ => {}
        }
        app
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

    fn mode_info(&self, sel: Selection) -> Option<&'static GameModeInfo> {
        self.registry.mode(sel.game, sel.mode)
    }

    /// Persistence key of the personal best shown for `sel`.
    fn best_key(&self, sel: Selection) -> Option<String> {
        let game = self.registry.get(sel.game)?.id();
        let mode = self.mode_info(sel)?;
        Some(if mode.is_daily {
            daily_key(game, &self.today.iso())
        } else {
            score_key(game, mode.id)
        })
    }

    fn context(&self, sel: Selection) -> TitleContext {
        TitleContext {
            best: self.best_key(sel).and_then(|k| self.save.highscore(&k)),
            today_iso: self.today.iso(),
            pulse: self.bg.pulse(),
            settings: self.save.settings.clone(),
        }
    }

    // ------------------------------------------------------------------ session control

    fn start_game(&mut self, sel: Selection) {
        let Some(mode) = self.mode_info(sel) else {
            return;
        };
        self.today = clock::today_utc();
        let seed = if mode.is_daily {
            self.today.seed()
        } else {
            clock::session_seed()
        };
        let ctx = self.context(sel);
        let held = poll_arcade_input();
        if let Some(game) = self.registry.get_mut(sel.game) {
            game.start(mode.id, seed);
            game.set_context(&ctx);
            // Keys still held from the menu must not trigger actions.
            game.on_resume(&held);
        }
        self.reset_effects();
        self.result = None;
        self.flow = 0.0;
        self.music_stage = 0;
        self.audio.play(cue::UI_CONFIRM, 0.6);
        self.nav.start(sel);
    }

    fn reset_effects(&mut self) {
        self.particles.clear();
        self.callouts.clear();
        self.flash.1 = 0.0;
        self.shake = 0.0;
    }

    /// Starts the attract mode on the next game that supports a demo.
    fn start_demo(&mut self) {
        let n = self.registry.len();
        for k in 0..n {
            let g = (self.demo_game + k) % n;
            let sel = Selection { game: g, mode: 0 };
            self.start_game(sel);
            let supported = self
                .registry
                .get_mut(g)
                .and_then(|game| game.demo_update(0.0))
                .is_some();
            if supported {
                self.demo = true;
                self.demo_game = g + 1;
                return;
            }
            self.nav.to_menu();
        }
        self.demo = false;
        self.idle = 0.0;
    }

    fn leave_game(&mut self) {
        self.demo = false;
        self.idle = 0.0;
        self.result = None;
        self.reset_effects();
        self.nav.to_menu();
    }

    fn any_user_input(&mut self) -> bool {
        let m = mouse_position();
        let moved = (vec2(m.0, m.1) - self.last_mouse).length() > 2.0;
        self.last_mouse = vec2(m.0, m.1);
        get_last_key_pressed().is_some() || is_mouse_button_pressed(MouseButton::Left) || moved
    }

    /// Applies what a title reported; returns the session end, if any.
    fn apply_events(&mut self, ev: FrameEvents) -> Option<SessionEnd> {
        for c in &ev.audio {
            self.audio.play_cue(c);
        }
        for b in &ev.particles {
            self.particles.burst(b.pos, b.color, b.count, b.speed);
        }
        for c in &ev.callouts {
            self.callouts.add(c.text.clone(), c.pos, c.color, c.size);
        }
        self.shake = self.shake.max(ev.shake);
        if let Some((c, s)) = ev.screen_flash
            && s >= self.flash.1
        {
            self.flash = (c, s);
        }
        self.flow = ev.flow;
        self.music_stage = ev.music_stage;
        ev.session_end
    }

    fn finish_session(&mut self, sel: Selection, end: SessionEnd) {
        let (Some(game), Some(mode)) = (self.registry.get(sel.game), self.mode_info(sel)) else {
            return;
        };
        let mut new_record = false;
        if end.counts_for_record {
            new_record = self.save.record_session(
                game.id(),
                mode.id,
                mode.is_daily,
                &self.today.iso(),
                end.value,
                end.lower_is_better,
            );
            self.persist();
        }
        let best = self
            .best_key(sel)
            .and_then(|k| self.save.highscore(&k))
            .map(|v| {
                if mode.ranked_by_time {
                    digits::format_time(v as f64 / 1000.0)
                } else {
                    v.to_string()
                }
            });
        if new_record {
            self.audio.play("levelup", 0.8);
        }
        self.result = Some(ResultInfo {
            end,
            new_record,
            best,
        });
    }

    // ------------------------------------------------------------------ update

    pub fn update(&mut self, dt: f32) {
        self.stage = Stage::compute();
        self.time += dt;
        self.nav.tick(dt);
        self.handle_global_keys();

        let tier = if self.nav.in_game() {
            FlowTier::from_flow(self.flow)
        } else {
            FlowTier::Chill
        };
        let bg_dt = if self.nav.screen == Screen::Paused {
            dt * 0.25
        } else {
            dt
        };
        self.bg.update(bg_dt, tier.bpm());

        if self.settings_ui.is_some() {
            self.update_settings();
        } else {
            match self.nav.screen {
                Screen::Menu => self.update_menu(dt),
                Screen::Playing => self.update_playing(dt),
                Screen::Ending => self.update_ending(dt),
                Screen::Paused => self.update_paused(),
                Screen::Results => self.update_results(),
            }
        }

        if self.nav.screen != Screen::Paused {
            self.particles.update(dt);
            self.callouts.update(dt);
            self.flash.1 = (self.flash.1 - dt * 2.5).max(0.0);
            self.shake *= (1.0 - 7.0 * dt).max(0.0);
            if self.shake < 0.05 {
                self.shake = 0.0;
            }
        }

        let track = self.music_track();
        self.audio
            .set_music(track, self.nav.screen == Screen::Paused);
        self.audio.update_music(dt);
    }

    /// Background track for the current screen. In-game the stage comes from
    /// the running title, so the music follows the game's intensity.
    fn music_track(&self) -> Option<Track> {
        match self.nav.screen {
            Screen::Menu => Some(Track::Menu),
            // Let the game-over / finish jingle ring out before the menu loop.
            Screen::Results => (self.nav.screen_time > 1.5).then_some(Track::Menu),
            Screen::Ending => None,
            Screen::Playing | Screen::Paused => {
                Some(Track::Game(self.music_stage.min(GAME_STAGES - 1)))
            }
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
            self.audio.play(cue::UI_CONFIRM, 0.5);
            self.persist();
        }
    }

    fn update_settings(&mut self) {
        let Some(state) = self.settings_ui.as_mut() else {
            return;
        };
        let input = SettingsInput {
            up: pressed_any(&[KeyCode::Up, KeyCode::W]),
            down: pressed_any(&[KeyCode::Down, KeyCode::S]),
            left: pressed_any(&[KeyCode::Left, KeyCode::A]),
            right: pressed_any(&[KeyCode::Right, KeyCode::D]),
            close: pressed_any(&[KeyCode::Tab, KeyCode::Escape, KeyCode::Enter]),
            mouse: self.stage.mouse(),
            click: is_mouse_button_pressed(MouseButton::Left),
        };
        let out = state.update(&input, &mut self.save.settings, self.themes.len());
        if out.changed {
            self.apply_audio_settings();
            self.audio.play(cue::UI_CONFIRM, 0.6);
            self.persist();
        }
        if out.close {
            self.settings_ui = None;
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
        let digit_keys = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
            KeyCode::Key7,
            KeyCode::Key8,
            KeyCode::Key9,
        ];
        let input = MenuInput {
            left: pressed_any(&[KeyCode::Left, KeyCode::A]),
            right: pressed_any(&[KeyCode::Right, KeyCode::D]),
            up: pressed_any(&[KeyCode::Up, KeyCode::W]),
            down: pressed_any(&[KeyCode::Down, KeyCode::S]),
            confirm: pressed_any(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]),
            settings: is_key_pressed(KeyCode::Tab),
            digit: digit_keys.iter().position(|&k| is_key_pressed(k)),
            mouse: self.stage.mouse(),
            click: is_mouse_button_pressed(MouseButton::Left),
        };
        let counts = mode_counts(&self.registry);
        let out = self.menu.update(&input, &counts);
        if out.moved {
            self.audio.play(cue::UI_MOVE, 0.6);
        }
        match out.action {
            Some(MenuAction::Play(sel)) => self.start_game(sel),
            Some(MenuAction::OpenSettings) => {
                self.audio.play(cue::UI_CONFIRM, 0.5);
                self.settings_ui = Some(SettingsState::default());
            }
            None => {}
        }
    }

    fn update_playing(&mut self, dt: f32) {
        let Some(sel) = self.nav.active else {
            self.nav.to_menu();
            return;
        };
        if !self.demo && pressed_any(&[KeyCode::P, KeyCode::Escape]) {
            self.nav.pause();
            return;
        }
        let ctx = self.context(sel);
        let input = poll_arcade_input();
        let demo = self.demo;
        let ev = {
            let Some(game) = self.registry.get_mut(sel.game) else {
                self.nav.to_menu();
                return;
            };
            game.set_context(&ctx);
            if demo {
                game.demo_update(dt).unwrap_or_default()
            } else {
                game.update(dt, &input)
            }
        };
        let end = self.apply_events(ev);

        if demo {
            if end.is_some() {
                self.start_demo();
            } else if self.nav.screen_time > 0.5 && self.any_user_input() {
                self.leave_game();
            }
            return;
        }
        if let Some(end) = end {
            self.finish_session(sel, end);
            self.nav.session_ended();
        }
    }

    /// The last frame stays visible for a moment; effects keep animating.
    fn update_ending(&mut self, dt: f32) {
        let Some(sel) = self.nav.active else {
            self.nav.to_menu();
            return;
        };
        let ctx = self.context(sel);
        let ev = match self.registry.get_mut(sel.game) {
            Some(game) => {
                game.set_context(&ctx);
                game.update(dt, &ArcadeInput::EMPTY)
            }
            None => FrameEvents::default(),
        };
        self.apply_events(ev);
    }

    fn pause_modal_click(&self) -> Option<ModalAction> {
        let sel = self.nav.active?;
        let game = self.registry.get(sel.game)?;
        ModalOverlay {
            kind: ModalKind::Pause {
                game_title: game.title(),
                legend: game.controls_legend(),
            },
            accent: game.accent_color(),
            age: self.nav.screen_time,
        }
        .click(self.stage.mouse())
    }

    fn update_paused(&mut self) {
        let clicked = if is_mouse_button_pressed(MouseButton::Left) {
            self.pause_modal_click()
        } else {
            None
        };
        if pressed_any(&[KeyCode::P, KeyCode::Escape, KeyCode::Enter])
            || clicked == Some(ModalAction::Resume)
        {
            let held = poll_arcade_input();
            if let Some(game) = self.nav.active.and_then(|s| self.registry.get_mut(s.game)) {
                game.on_resume(&held);
            }
            self.nav.resume();
        } else if is_key_pressed(KeyCode::R) || clicked == Some(ModalAction::Restart) {
            if let Some(sel) = self.nav.active {
                self.start_game(sel);
            }
        } else if is_key_pressed(KeyCode::Q) || clicked == Some(ModalAction::MainMenu) {
            self.leave_game();
        }
    }

    fn results_modal_click(&self) -> Option<ModalAction> {
        let (sel, r) = (self.nav.active?, self.result.as_ref()?);
        let game = self.registry.get(sel.game)?;
        let rows = r.end.summary.clone();
        ModalOverlay {
            kind: ModalKind::Results {
                title: r.end.title,
                won: r.end.won,
                headline: (r.end.headline.0, &r.end.headline.1),
                rows: &rows,
                new_record: r.new_record,
                best: r.best.as_deref(),
            },
            accent: game.accent_color(),
            age: self.nav.screen_time,
        }
        .click(self.stage.mouse())
    }

    fn update_results(&mut self) {
        if self.nav.screen_time < 0.35 {
            return; // avoid instantly skipping with a held key
        }
        let clicked = if is_mouse_button_pressed(MouseButton::Left) {
            self.results_modal_click()
        } else {
            None
        };
        if pressed_any(&[KeyCode::R, KeyCode::Enter, KeyCode::KpEnter])
            || clicked == Some(ModalAction::Restart)
        {
            if let Some(sel) = self.nav.active {
                self.start_game(sel);
            }
        } else if pressed_any(&[KeyCode::Escape, KeyCode::Q])
            || clicked == Some(ModalAction::MainMenu)
        {
            self.leave_game();
        }
    }

    // ------------------------------------------------------------------ draw

    pub fn draw(&self) {
        clear_background(BLACK);
        let shake = vec2(
            (self.time * 91.0).sin() * self.shake,
            (self.time * 77.0).cos() * self.shake,
        );
        self.stage.apply_with_offset(shake);
        let theme = self.theme();
        let intensity = if self.nav.in_game() {
            self.flow / 100.0
        } else {
            0.15
        };
        self.bg.draw(theme, intensity);
        let pulse = self.bg.pulse();

        match self.nav.screen {
            Screen::Menu => menu::draw(&MenuView {
                stage: &self.stage,
                theme,
                registry: &self.registry,
                state: &self.menu,
                save: &self.save,
                today: &self.today,
                bg: &self.bg,
                time: self.time,
            }),
            Screen::Playing | Screen::Paused | Screen::Ending | Screen::Results => {
                self.draw_game(theme, pulse)
            }
        }
        if let Some(state) = &self.settings_ui {
            settings::draw(&self.stage, theme, state, self.settings(), pulse);
        }
    }

    fn draw_game(&self, theme: &Theme, pulse: f32) {
        let Some(sel) = self.nav.active else {
            return;
        };
        let Some(game) = self.registry.get(sel.game) else {
            return;
        };
        game.draw(&self.stage, theme, self.settings().colorblind);
        self.particles.draw();
        self.callouts.draw(&self.stage);
        if self.flash.1 > 0.0 {
            draw_rectangle(
                0.0,
                0.0,
                VIRTUAL_W,
                VIRTUAL_H,
                with_alpha(self.flash.0, self.flash.1 * 0.18),
            );
        }
        if self.demo {
            // Bottom centre: free in every title (HUDs live at the top/sides).
            let a = 0.55 + 0.45 * pulse;
            draw_rectangle(
                VIRTUAL_W * 0.5 - 130.0,
                VIRTUAL_H - 28.0,
                260.0,
                24.0,
                Color::new(0.0, 0.0, 0.0, 0.55),
            );
            text::label(
                &self.stage,
                "DEMO  -  PRESS ANY KEY",
                VIRTUAL_W * 0.5,
                VIRTUAL_H - 24.0,
                Size::X1,
                with_alpha(theme.accent2, a),
                Align::Center,
            );
        }
        let mouse = self.stage.mouse();
        match self.nav.screen {
            Screen::Paused => ModalOverlay {
                kind: ModalKind::Pause {
                    game_title: game.title(),
                    legend: game.controls_legend(),
                },
                accent: game.accent_color(),
                age: self.nav.screen_time,
            }
            .draw(&self.stage, theme, mouse, pulse),
            Screen::Results => {
                if let Some(r) = &self.result {
                    ModalOverlay {
                        kind: ModalKind::Results {
                            title: r.end.title,
                            won: r.end.won,
                            headline: (r.end.headline.0, &r.end.headline.1),
                            rows: &r.end.summary,
                            new_record: r.new_record,
                            best: r.best.as_deref(),
                        },
                        accent: game.accent_color(),
                        age: self.nav.screen_time,
                    }
                    .draw(&self.stage, theme, mouse, pulse);
                }
            }
            _ => {}
        }
    }
}

fn mode_counts(registry: &GameRegistry) -> Vec<usize> {
    registry.iter().map(|g| g.modes().len()).collect()
}

/// Default seconds after which `--screenshot` captures the frame.
const SCREENSHOT_AFTER: f32 = 4.0;

pub async fn run(cli: Cli) {
    let mut app = App::new(&cli);
    let mut elapsed = 0.0;
    loop {
        let dt = get_frame_time();
        app.update(dt);
        app.draw();
        elapsed += dt;
        if let Some(path) = &cli.screenshot
            && elapsed >= cli.wait.unwrap_or(SCREENSHOT_AFTER)
        {
            // Only the game's own framebuffer is captured, never the desktop.
            get_screen_data().export_png(path);
            std::process::exit(0);
        }
        next_frame().await;
    }
}
