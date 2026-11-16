//! `ArcadeTitle` adapter of VibeCoded Glide: input mapping, events to
//! sound/particles/callouts, session end, smoke bot.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use crate::core::FlowTier;
use crate::core::glide::bot;
use crate::core::glide::game::{
    Cause, GLIDER_X, GlideEvent, GlideGame, GlideInput, GlideMode, Status,
};
use crate::presentation::digits;
use crate::presentation::glide_ui::{self, TrailPoint, UiCtx, to_stage};
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, themes};

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new(
        "classic",
        "CLASSIC GLIDE",
        "Static pipes, rising speed and Vibe rings to chase.",
    ),
    GameModeInfo::new(
        "kinetic",
        "KINETIC SHIFT",
        "Pipes start to sway and wind gusts push you around.",
    ),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "The exact same pipes and rings for everyone today.",
    )
    .daily(),
    GameModeInfo::new(
        "zen",
        "ZEN",
        "Cannot crash: you bounce off with an EMP flash and fly on.",
    ),
];

const LEGEND: [(&str, &str); 5] = [
    ("SPACE / W / UP / CLICK", "Flap"),
    ("DOWN / S (hold)", "Dive"),
    ("SHIFT / C / RIGHT CLICK", "Phase dash (i-frames)"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

/// Arcade input -> Glide input (keys and mouse buttons).
pub fn input_from(input: &ArcadeInput) -> GlideInput {
    GlideInput {
        flap_pressed: input.any_pressed(&[Key::Space, Key::W, Key::Up])
            || input.mouse_left_pressed(),
        dive_held: input.any_down(&[Key::Down, Key::S]),
        dash_pressed: input.any_pressed(&[Key::Shift, Key::C]) || input.mouse_right_pressed(),
    }
}

const TRAIL_SECS: f32 = 0.9;

pub struct VibeCodedGlide {
    game: GlideGame,
    ctx: TitleContext,
    themes: [Theme; 2],
    time: f32,
    reported: bool,
    last_tier: FlowTier,
    trail: Vec<TrailPoint>,
}

impl Default for VibeCodedGlide {
    fn default() -> Self {
        Self::new()
    }
}

impl VibeCodedGlide {
    pub fn new() -> Self {
        VibeCodedGlide {
            game: GlideGame::new(GlideMode::Classic, 1),
            ctx: TitleContext::default(),
            themes: themes(),
            time: 0.0,
            reported: false,
            last_tier: FlowTier::Chill,
            trail: Vec::new(),
        }
    }

    pub fn game(&self) -> &GlideGame {
        &self.game
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.ctx.settings.theme % self.themes.len()]
    }

    fn advance(&mut self, dt: f32, input: &GlideInput) -> FrameEvents {
        self.time += dt;
        self.game.update(dt, input);
        // Contrail: remember where the glider has been.
        for p in &mut self.trail {
            p.0 += dt;
        }
        self.trail.retain(|p| p.0 < TRAIL_SECS);
        if self.game.started && self.game.is_playing() {
            self.trail.push((0.0, self.game.y));
        }
        let mut ev = self.collect_events();
        if !self.game.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    fn collect_events(&mut self) -> FrameEvents {
        let theme = self.theme().clone();
        let mut ev = FrameEvents::default();
        let tier = self.game.tier();
        let ti = tier.index();
        let gp = |y: f32| to_stage(GLIDER_X, y);
        for e in self.game.drain_events() {
            match e {
                GlideEvent::Started => {}
                GlideEvent::Flap => {
                    ev.burst(gp(self.game.y + 6.0), theme.accent2, 3, 110.0);
                    ev.sound(cue::ROTATE_OR_TURN, 0.28);
                }
                GlideEvent::Dash => {
                    ev.burst(gp(self.game.y), theme.accent, 14, 260.0);
                    ev.sound(cue::ROTATE_OR_TURN, 0.55);
                    ev.flash(theme.accent, 0.25);
                }
                GlideEvent::Passed { y, points } => {
                    ev.callout(
                        format!("+{points}"),
                        gp(y) + vec2(70.0, -20.0),
                        theme.text,
                        Size::X1,
                    );
                    // A soft rising pentatonic tick for every pipe.
                    ev.note(ti, self.game.passed as usize % 5, 0.3);
                }
                GlideEvent::Ring {
                    y,
                    points,
                    perfect,
                    combo,
                } => {
                    let p = gp(y) + vec2(40.0, 0.0);
                    let gold = theme.pieces[1];
                    ev.burst(p, gold, 22, 300.0);
                    let label = if perfect { "PERFECT RING!" } else { "RING!" };
                    ev.callout(
                        format!("{label} +{points}"),
                        p + vec2(0.0, -34.0),
                        gold,
                        Size::X2,
                    );
                    // The combo raises the pitch: the cue itself is a single
                    // sample, so the rising tone comes from the note ladder.
                    ev.sound(cue::CLEAR_OR_EAT, 0.45);
                    ev.note(ti, (combo as usize).min(7), 0.7);
                }
                GlideEvent::RingMissed => {}
                GlideEvent::Skim { y, points } => {
                    let p = gp(y);
                    ev.callout(
                        format!("CLOSE SKIM! +{points}"),
                        p + vec2(60.0, -40.0),
                        theme.accent2,
                        Size::X1,
                    );
                    ev.burst(p, theme.accent2, 8, 200.0);
                    ev.sound(cue::ACTION_TICK, 0.4);
                    ev.note(ti, 7, 0.4);
                }
                GlideEvent::Bounce(_) => {
                    ev.flash(theme.accent, 0.8);
                    ev.burst(gp(self.game.y), theme.accent, 26, 320.0);
                    ev.callout(
                        "EMP!",
                        gp(self.game.y) + vec2(40.0, -30.0),
                        theme.accent,
                        Size::X2,
                    );
                    ev.sound(cue::ACTION_TICK, 0.5);
                }
                GlideEvent::GustStart { lift } => {
                    let text = if lift < 0.0 { "UPDRAFT!" } else { "DOWNDRAFT!" };
                    ev.callout(text, to_stage(480.0, 120.0), theme.accent2, Size::X2);
                    ev.sound(cue::ACTION_TICK, 0.3);
                }
                GlideEvent::Died(_) => {
                    ev.shake = 12.0;
                    ev.flash(theme.danger, 0.9);
                    ev.burst(gp(self.game.y), theme.danger, 40, 380.0);
                    ev.sound(cue::GAME_OVER, 0.8);
                }
            }
        }
        if tier > self.last_tier {
            let c = theme.tiers[ti];
            if tier == FlowTier::Overdrive {
                ev.callout("OVERDRIVE 2.0x", to_stage(480.0, 220.0), c, Size::X3);
                ev.flash(c, 1.0);
                ev.sound(cue::OVERDRIVE_UP, 0.8);
            } else {
                ev.callout(
                    format!("{} {}x", tier.name(), tier.multiplier()),
                    to_stage(480.0, 220.0),
                    c,
                    Size::X2,
                );
            }
        }
        self.last_tier = tier;
        ev.flow = self.game.flow.flow;
        ev.music_stage = glide_ui::music_stage(&self.game);
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.game;
        let title = match g.status {
            Status::Dead(Cause::Pipe) => "CRASHED INTO A PIPE",
            Status::Dead(Cause::Ground) => "HIT THE GROUND",
            Status::Dead(Cause::Ceiling) => "HIT THE CEILING",
            Status::Playing => "GAME OVER",
        };
        SessionEnd {
            title,
            won: false,
            headline: ("SCORE", g.score.to_string()),
            value: g.score,
            lower_is_better: false,
            counts_for_record: true,
            summary: vec![
                ("PIPES".into(), g.passed.to_string()),
                ("RINGS".into(), g.rings.to_string()),
                ("BEST RING COMBO".into(), g.best_combo.to_string()),
                ("CLOSE SKIMS".into(), g.skims.to_string()),
                ("TIME".into(), digits::format_time(g.elapsed)),
            ],
        }
    }

    /// Invariants checked on every headless bot step.
    fn check_invariants(&self) {
        let g = &self.game;
        assert!(
            g.y.is_finite() && g.vy.is_finite(),
            "glider state is finite"
        );
        assert!((0.0..=100.0).contains(&g.flow.flow), "flow out of range");
        assert!(
            g.pipes.windows(2).all(|w| w[1].x > w[0].x),
            "pipes are ordered"
        );
        assert!(
            g.pipes.iter().all(|p| p.gap_h() >= 138.0),
            "gaps never below 138"
        );
        assert!((220.0..=340.0).contains(&g.speed()));
    }
}

impl ArcadeTitle for VibeCodedGlide {
    fn id(&self) -> &'static str {
        "glide"
    }

    fn title(&self) -> &'static str {
        "VIBECODED GLIDE"
    }

    fn tagline(&self) -> &'static str {
        "flap, thread the rings, ride the flow"
    }

    fn accent_color(&self) -> Color {
        hex(0xB46CFF)
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.game = GlideGame::new(GlideMode::from_id(mode_id), seed);
        self.time = 0.0;
        self.reported = false;
        self.last_tier = FlowTier::Chill;
        self.trail.clear();
    }

    fn set_context(&mut self, ctx: &TitleContext) {
        self.ctx = ctx.clone();
    }

    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        let i = input_from(input);
        self.advance(dt, &i)
    }

    fn demo_update(&mut self, dt: f32) -> Option<FrameEvents> {
        // The bot decides per fixed step; one decision per frame is enough
        // because it only depends on the current state.
        let i = bot::choose(&self.game);
        Some(self.advance(dt, &i))
    }

    fn dev_preset(&mut self, name: &str) {
        let (mode, seed) = (self.game.mode, self.game.seed);
        match name {
            // Overdrive: rainbow contrail, widened gaps, rings in view.
            "showcase" => {
                let g = &mut self.game;
                g.started = true;
                g.flow.flow = 95.0;
                g.passed = 12;
                g.combo = 5;
                g.score = 3400;
                g.y = 280.0;
                g.vy = -300.0;
            }
            // A Kinetic Shift run with a wind gust blowing.
            "gust" => {
                self.game = GlideGame::new(GlideMode::KineticShift, seed);
                let g = &mut self.game;
                g.started = true;
                g.passed = 8;
                g.skip_to_gust();
            }
            // Zen: sitting on the floor, about to bounce.
            "bounce" => {
                self.game = GlideGame::new(GlideMode::Zen, seed);
                let g = &mut self.game;
                g.started = true;
                g.y = 560.0;
                g.vy = 400.0;
            }
            // A falling glider (nose down).
            "dive" => {
                let g = &mut self.game;
                g.started = true;
                g.y = 250.0;
                g.vy = 650.0;
            }
            _ => {
                let _ = mode;
            }
        }
    }

    fn draw(&self, canvas: &VirtualCanvas, theme: &Theme, colorblind: bool) {
        let label = MODES
            .iter()
            .find(|m| m.id == self.game.mode.id())
            .map(|m| m.label)
            .unwrap_or("");
        let ctx = UiCtx {
            stage: canvas,
            theme,
            colorblind,
            time: self.time,
            pulse: self.ctx.pulse,
            best: self.ctx.best,
            mode_label: label,
        };
        glide_ui::draw_arena(&self.game, &ctx, &self.trail);
        glide_ui::draw_hud(&self.game, &ctx);
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        glide_ui::draw_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        // A crash of the bot is a regression and must fail loudly.
        assert!(
            self.game.is_playing(),
            "glide bot crashed at tick {tick}: {:?}",
            self.game.status
        );
        let i = bot::choose(&self.game);
        let ev = self.advance(1.0 / 60.0, &i);
        assert!(ev.flow.is_finite());
        self.check_invariants();
        if tick >= 280 {
            let g = &self.game;
            assert!(
                g.started && g.passed >= 1 && g.score > 0,
                "bot made no progress"
            );
        }
    }
}

#[cfg(test)]
impl VibeCodedGlide {
    /// Injects a ring event with `combo` as the game would emit it.
    fn game_events_for_test(&mut self, combo: u32) {
        self.game.push_test_event(GlideEvent::Ring {
            y: 300.0,
            points: 150,
            perfect: true,
            combo,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(keys: &[Key]) -> ArcadeInput {
        ArcadeInput::tap(keys)
    }

    #[test]
    fn keys_and_mouse_map_to_flap_dive_and_dash() {
        for k in [Key::Space, Key::W, Key::Up] {
            let i = input_from(&tap(&[k]));
            assert!(i.flap_pressed && !i.dash_pressed, "{k:?}");
        }
        assert!(input_from(&ArcadeInput::EMPTY.with_mouse(true, false)).flap_pressed);
        for k in [Key::Shift, Key::C] {
            assert!(input_from(&tap(&[k])).dash_pressed, "{k:?}");
        }
        assert!(input_from(&ArcadeInput::EMPTY.with_mouse(false, true)).dash_pressed);
        for k in [Key::Down, Key::S] {
            assert!(input_from(&ArcadeInput::new([k], [])).dive_held, "{k:?}");
        }
        // Holding flap does not flap again (rising edge only).
        let held = input_from(&ArcadeInput::new([Key::Space], []));
        assert!(!held.flap_pressed);
    }

    #[test]
    fn smoke_bot_plays_every_mode_with_progress() {
        for m in MODES {
            let mut t = VibeCodedGlide::new();
            t.start(m.id, 20261116);
            for tick in 0..300 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().passed >= 1, "{}", m.id);
        }
    }

    #[test]
    fn a_crash_reports_one_session_end_and_zen_never_ends() {
        let mut t = VibeCodedGlide::new();
        t.start("classic", 5);
        let mut ends = Vec::new();
        t.update(0.02, &tap(&[Key::Space]));
        for _ in 0..200 {
            if let Some(e) = t.update(1.0 / 30.0, &ArcadeInput::EMPTY).session_end {
                ends.push(e);
            }
        }
        assert_eq!(ends.len(), 1, "falling to the ground ends the run once");
        assert_eq!(ends[0].title, "HIT THE GROUND");
        assert!(ends[0].counts_for_record && !ends[0].lower_is_better && !ends[0].won);
        assert!(ends[0].summary.iter().any(|(k, _)| k == "RINGS"));

        let mut z = VibeCodedGlide::new();
        z.start("zen", 5);
        z.update(0.02, &tap(&[Key::Space]));
        for _ in 0..600 {
            assert!(
                z.update(1.0 / 30.0, &ArcadeInput::EMPTY)
                    .session_end
                    .is_none()
            );
        }
        assert!(z.game().is_playing());
    }

    #[test]
    fn rings_raise_the_note_pitch_with_the_combo() {
        let mut t = VibeCodedGlide::new();
        t.start("classic", 5);
        let mut pitches = Vec::new();
        for combo in 1..=4u32 {
            t.game.drain_events();
            t.game.combo = combo;
            // Craft the event as the game would emit it.
            t.game.step(&GlideInput {
                flap_pressed: true,
                ..Default::default()
            });
            t.game.drain_events();
            t.game_events_for_test(combo);
            let ev = t.collect_events();
            for c in ev.audio {
                if let crate::arcade::AudioCue::Note { idx, .. } = c {
                    pitches.push(idx);
                }
            }
        }
        assert_eq!(pitches, vec![1, 2, 3, 4], "note index follows the combo");
    }

    #[test]
    fn every_sound_the_title_requests_exists_in_the_bank() {
        let bank = crate::presentation::synth::build_bank();
        let mut names = std::collections::HashSet::new();
        for (preset, mode) in [
            ("showcase", "classic"),
            ("gust", "kinetic"),
            ("bounce", "zen"),
            ("", "classic"),
        ] {
            let mut t = VibeCodedGlide::new();
            t.start(mode, 11);
            t.dev_preset(preset);
            for _ in 0..1800 {
                let ev = t.demo_update(1.0 / 60.0).unwrap();
                for c in ev.audio {
                    if let crate::arcade::AudioCue::Sound { name, .. } = c {
                        names.insert(name);
                    }
                }
                assert!(ev.music_stage < crate::presentation::music::GAME_STAGES);
            }
        }
        assert!(names.len() >= 3, "{names:?}");
        for n in names {
            assert!(
                bank.iter().any(|(b, _)| b == n),
                "sound '{n}' missing in the bank"
            );
        }
    }

    #[test]
    fn metadata_is_complete() {
        let t = VibeCodedGlide::new();
        assert_eq!((t.id(), t.title()), ("glide", "VIBECODED GLIDE"));
        assert_eq!(t.modes().len(), 4);
        assert!(t.modes().iter().any(|m| m.is_daily));
        assert!(!t.controls_legend().is_empty());
    }
}
