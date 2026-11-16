//! `ArcadeTitle` adapter of VibeCoded Hopper: input mapping, events to
//! sound/particles/callouts, session end, smoke bot.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use crate::core::FlowTier;
use crate::core::hopper::bot;
use crate::core::hopper::game::{
    Cause, Dir, HopperEvent, HopperGame, HopperInput, HopperMode, Status,
};
use crate::core::hopper::world::ItemKind;
use crate::presentation::digits;
use crate::presentation::hopper_ui::{self, FIELD_X, FIELD_Y, UiCtx, sx, sy};
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, themes};

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new(
        "classic",
        "CLASSIC ENDLESS",
        "Climb through meadows, highways, rails, rivers and hazard zones.",
    ),
    GameModeInfo::new(
        "blitz",
        "BLITZ RUSH",
        "The camera sprints. More shards and Phase-Boots to keep up.",
    ),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "The same world, traffic timing and shards for everyone today.",
    )
    .daily(),
    GameModeInfo::new(
        "zen",
        "ZEN",
        "No camera, no death: a hit just puts you back on safe ground.",
    ),
];

const LEGEND: [(&str, &str); 4] = [
    ("ARROWS / WASD", "Hop (taps are buffered for 100 ms)"),
    ("LOGS CARRY YOU", "Stay on the river platforms"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

/// Arcade input -> Hopper input (rising edge of a direction key).
pub fn input_from(input: &ArcadeInput) -> HopperInput {
    let dir = if input.any_pressed(&[Key::Up, Key::W]) {
        Some(Dir::Up)
    } else if input.any_pressed(&[Key::Down, Key::S]) {
        Some(Dir::Down)
    } else if input.any_pressed(&[Key::Left, Key::A]) {
        Some(Dir::Left)
    } else if input.any_pressed(&[Key::Right, Key::D]) {
        Some(Dir::Right)
    } else {
        None
    };
    HopperInput { dir }
}

pub struct VibeCodedHopper {
    game: HopperGame,
    ctx: TitleContext,
    themes: [Theme; 2],
    time: f32,
    reported: bool,
    last_tier: FlowTier,
}

impl Default for VibeCodedHopper {
    fn default() -> Self {
        Self::new()
    }
}

impl VibeCodedHopper {
    pub fn new() -> Self {
        VibeCodedHopper {
            game: HopperGame::new(HopperMode::Classic, 1),
            ctx: TitleContext::default(),
            themes: themes(),
            time: 0.0,
            reported: false,
            last_tier: FlowTier::Chill,
        }
    }

    pub fn game(&self) -> &HopperGame {
        &self.game
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.ctx.settings.theme % self.themes.len()]
    }

    fn advance(&mut self, dt: f32, input: &HopperInput) -> FrameEvents {
        self.time += dt;
        self.game.update(dt, input);
        let mut ev = self.collect_events();
        if !self.game.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    /// Stage position of the centre of the player's tile.
    fn player_pos(&self) -> Vec2 {
        vec2(
            sx(self.game.pos_x()),
            sy(&self.game, self.game.pos_row()) + hopper_ui::TILE * 0.7,
        )
    }

    fn collect_events(&mut self) -> FrameEvents {
        let theme = self.theme().clone();
        let mut ev = FrameEvents::default();
        let tier = self.game.tier();
        let ti = tier.index();
        let p = self.player_pos();
        for e in self.game.drain_events() {
            match e {
                HopperEvent::Hop => {
                    ev.burst(p, theme.text_dim, 3, 90.0);
                    ev.sound(cue::ACTION_TICK, 0.22);
                }
                HopperEvent::Bump => ev.sound(cue::ACTION_TICK, 0.12),
                HopperEvent::Progress { row, combo, .. } => {
                    ev.note(ti, (row as usize) % 5, 0.18);
                    if combo >= 5 && combo % 5 == 0 {
                        ev.callout(
                            format!("COMBO x{combo}"),
                            p + vec2(0.0, -40.0),
                            theme.accent2,
                            Size::X1,
                        );
                    }
                }
                HopperEvent::Shard { points, .. } => {
                    ev.burst(p, theme.pieces[0], 14, 220.0);
                    ev.callout(
                        format!("+{points}"),
                        p + vec2(0.0, -30.0),
                        theme.pieces[0],
                        Size::X1,
                    );
                    ev.sound(cue::CLEAR_OR_EAT, 0.5);
                    ev.note(ti, 6, 0.4);
                }
                HopperEvent::Item(kind) => {
                    let (text, col) = match kind {
                        ItemKind::Aegis => ("AEGIS SHIELD!", theme.accent2),
                        ItemKind::Boots => ("PHASE-BOOTS!", theme.pieces[1]),
                    };
                    ev.callout(text, p + vec2(0.0, -44.0), col, Size::X2);
                    ev.burst(p, col, 20, 240.0);
                    ev.sound(cue::CLEAR_OR_EAT, 0.7);
                }
                HopperEvent::NearMiss { points, .. } => {
                    ev.callout(
                        format!("NEAR MISS! +{points}"),
                        p + vec2(0.0, -44.0),
                        theme.danger,
                        Size::X2,
                    );
                    ev.burst(p, theme.pieces[1], 16, 280.0);
                    ev.sound(cue::ACTION_TICK, 0.55);
                    ev.note(ti, 7, 0.5);
                }
                HopperEvent::TrainWarning { .. } => ev.sound(cue::ROTATE_OR_TURN, 0.45),
                HopperEvent::TrainPass { .. } => {
                    ev.shake = ev.shake.max(5.0);
                    ev.sound("harddrop", 0.3);
                }
                HopperEvent::BridgeSink { .. } => ev.sound(cue::ACTION_TICK, 0.3),
                HopperEvent::Splash => {
                    ev.burst(p, theme.tiers[0], 30, 260.0);
                    ev.shake = ev.shake.max(5.0);
                }
                HopperEvent::ShieldSaved => {
                    ev.callout(
                        "SHIELD SAVED!",
                        p + vec2(0.0, -48.0),
                        theme.accent2,
                        Size::X2,
                    );
                    ev.flash(theme.accent2, 0.5);
                    ev.sound(cue::OVERDRIVE_UP, 0.5);
                }
                HopperEvent::IceSaved => {
                    ev.callout("ICE BRIDGE!", p + vec2(0.0, -48.0), theme.accent2, Size::X2);
                    ev.flash(theme.accent2, 0.5);
                    ev.sound(cue::OVERDRIVE_UP, 0.5);
                }
                HopperEvent::ZenReset => {
                    ev.callout(
                        "BACK TO SAFETY",
                        p + vec2(0.0, -48.0),
                        theme.accent,
                        Size::X1,
                    );
                    ev.flash(theme.accent, 0.4);
                }
                HopperEvent::Died(_) => {
                    ev.shake = 12.0;
                    ev.flash(theme.danger, 0.9);
                    ev.burst(p, theme.danger, 36, 340.0);
                    ev.sound(cue::GAME_OVER, 0.8);
                }
            }
        }
        if tier > self.last_tier {
            let c = theme.tiers[ti];
            let centre = vec2(FIELD_X + hopper_ui::FIELD_W * 0.5, FIELD_Y + 200.0);
            if tier == FlowTier::Overdrive {
                ev.callout("OVERDRIVE 2.0x", centre, c, Size::X3);
                ev.flash(c, 1.0);
                ev.sound(cue::OVERDRIVE_UP, 0.8);
            } else {
                ev.callout(
                    format!("{} {}x", tier.name(), tier.multiplier()),
                    centre,
                    c,
                    Size::X2,
                );
            }
        }
        self.last_tier = tier;
        ev.flow = self.game.flow.flow;
        ev.music_stage = hopper_ui::music_stage(&self.game);
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.game;
        let title = match g.status {
            Status::Dead(Cause::Traffic) => "RUN OVER",
            Status::Dead(Cause::Train) => "HIT BY THE MAGLEV",
            Status::Dead(Cause::Splash) => "SPLASH!",
            Status::Dead(Cause::Hazard) => "LASERED",
            Status::Dead(Cause::Fall) => "THE BRIDGE GAVE WAY",
            Status::Dead(Cause::Sweep) => "SWEPT AWAY",
            Status::Dead(Cause::Idle) => "TOO SLOW",
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
                ("DISTANCE".into(), g.max_row.to_string()),
                ("SHARDS".into(), g.shards.to_string()),
                ("NEAR MISSES".into(), g.near_misses.to_string()),
                ("BEST COMBO".into(), g.best_combo.to_string()),
                ("TIME".into(), digits::format_time(g.time as f64)),
            ],
        }
    }

    /// Invariants checked on every headless bot step.
    fn check_invariants(&self) {
        let g = &self.game;
        assert!(
            g.x.is_finite() && (0.0..=15.0).contains(&g.x),
            "x in the playfield"
        );
        assert!(g.row >= 0 && g.max_row >= g.row.min(g.max_row));
        assert!((0.0..=100.0).contains(&g.flow.flow), "flow out of range");
        assert!(g.world.cached_rows() < 200, "old rows are dropped");
    }
}

impl ArcadeTitle for VibeCodedHopper {
    fn id(&self) -> &'static str {
        "hopper"
    }

    fn title(&self) -> &'static str {
        "VIBECODED HOPPER"
    }

    fn tagline(&self) -> &'static str {
        "hop, dodge, ride the flow"
    }

    fn accent_color(&self) -> Color {
        hex(0xFFD84A)
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.game = HopperGame::new(HopperMode::from_id(mode_id), seed);
        self.time = 0.0;
        self.reported = false;
        self.last_tier = FlowTier::Chill;
    }

    fn set_context(&mut self, ctx: &TitleContext) {
        self.ctx = ctx.clone();
    }

    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        let i = input_from(input);
        self.advance(dt, &i)
    }

    fn demo_update(&mut self, dt: f32) -> Option<FrameEvents> {
        let i = bot::choose(&mut self.game);
        Some(self.advance(dt, &i))
    }

    fn dev_preset(&mut self, name: &str) {
        let (mode, seed) = (self.game.mode, self.game.seed);
        let find = |g: &mut HopperGame,
                    f: &dyn Fn(&crate::core::hopper::world::RowSpec) -> bool| {
            (4..400i64).find(|&r| f(&g.spec(r)))
        };
        match name {
            // Stand just below a river and a rail, with a shield and boots.
            "river" => {
                self.game = HopperGame::new(HopperMode::Zen, seed);
                let g = &mut self.game;
                if let Some(r) = find(g, &|s| {
                    matches!(s.lane, crate::core::hopper::world::LaneKind::River { .. })
                }) {
                    g.row = (r - 1).max(0);
                    g.max_row = g.row;
                    g.camera_row = g.row as f32 - 4.0;
                    g.last_safe = (g.row, 7.5);
                    g.ensure_rows();
                }
            }
            "train" => {
                self.game = HopperGame::new(HopperMode::Zen, seed);
                let g = &mut self.game;
                if let Some(r) = find(g, &|s| {
                    matches!(s.lane, crate::core::hopper::world::LaneKind::Rail { .. })
                }) {
                    g.row = (r - 1).max(0);
                    g.max_row = g.row;
                    g.camera_row = g.row as f32 - 4.0;
                    g.ensure_rows();
                    // Jump to a moment when the warning lights blink.
                    if let crate::core::hopper::world::LaneKind::Rail { period, phase, .. } =
                        g.spec(r).lane
                    {
                        let mut t = 0.0;
                        while (t + phase).rem_euclid(period) < period - 0.35 - 0.9 {
                            t += 0.02;
                        }
                        g.time = t;
                    }
                }
            }
            "hazard" => {
                self.game = HopperGame::new_at_row(HopperMode::Zen, seed, 36);
                let g = &mut self.game;
                // Stand right below the first hazard lane.
                if let Some(r) = (35..400i64).find(|&r| {
                    matches!(
                        g.spec(r).lane,
                        crate::core::hopper::world::LaneKind::Laser { .. }
                            | crate::core::hopper::world::LaneKind::Bridge { .. }
                    )
                }) {
                    g.row = r - 1;
                    g.max_row = g.row;
                    g.camera_row = g.row as f32 - 4.0;
                    g.last_safe = (g.row, 7.5);
                    g.ensure_rows();
                }
            }
            "overdrive" => {
                let g = &mut self.game;
                g.flow.flow = 95.0;
                g.shield = true;
                g.boots = 6.0;
                g.combo = 6;
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
        hopper_ui::draw_world(&self.game, &ctx);
        hopper_ui::draw_hud(&self.game, &ctx);
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        hopper_ui::draw_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        // A death of the bot is a regression and must fail loudly.
        assert!(
            self.game.is_playing(),
            "hopper bot died at tick {tick}: {:?}",
            self.game.status
        );
        let i = bot::choose(&mut self.game);
        let ev = self.advance(1.0 / 60.0, &i);
        assert!(ev.flow.is_finite());
        self.check_invariants();
        if tick >= 280 {
            let g = &self.game;
            assert!(g.max_row >= 3 && g.score > 0, "bot made no progress");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(keys: &[Key]) -> ArcadeInput {
        ArcadeInput::tap(keys)
    }

    #[test]
    fn arrows_and_wasd_hop_and_holding_does_not_repeat() {
        for (k, d) in [
            (Key::Up, Dir::Up),
            (Key::W, Dir::Up),
            (Key::Down, Dir::Down),
            (Key::S, Dir::Down),
            (Key::Left, Dir::Left),
            (Key::A, Dir::Left),
            (Key::Right, Dir::Right),
            (Key::D, Dir::Right),
        ] {
            assert_eq!(input_from(&tap(&[k])).dir, Some(d), "{k:?}");
        }
        assert_eq!(input_from(&ArcadeInput::new([Key::Up], [])).dir, None);
        assert_eq!(input_from(&ArcadeInput::EMPTY).dir, None);
    }

    #[test]
    fn smoke_bot_plays_every_mode_with_progress() {
        for m in MODES {
            let mut t = VibeCodedHopper::new();
            t.start(m.id, 20261116);
            for tick in 0..300 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().max_row >= 3, "{}", m.id);
        }
    }

    #[test]
    fn a_death_reports_one_session_end_and_zen_never_ends() {
        let mut t = VibeCodedHopper::new();
        t.start("classic", 5);
        let mut ends = Vec::new();
        for _ in 0..(10 * 60) {
            if let Some(e) = t.update(1.0 / 60.0, &ArcadeInput::EMPTY).session_end {
                ends.push(e);
            }
        }
        assert_eq!(ends.len(), 1, "idling too long ends the run once");
        // Classic's camera creeps up and sweeps the idle player first.
        assert_eq!(ends[0].title, "SWEPT AWAY");
        assert!(ends[0].counts_for_record && !ends[0].lower_is_better);
        assert!(ends[0].summary.iter().any(|(k, _)| k == "DISTANCE"));

        let mut z = VibeCodedHopper::new();
        z.start("zen", 5);
        for _ in 0..(20 * 60) {
            assert!(
                z.update(1.0 / 60.0, &ArcadeInput::EMPTY)
                    .session_end
                    .is_none()
            );
        }
        assert!(z.game().is_playing());
    }

    #[test]
    fn every_sound_the_title_requests_exists_in_the_bank() {
        let bank = crate::presentation::synth::build_bank();
        let mut names = std::collections::HashSet::new();
        for (preset, mode) in [
            ("river", "zen"),
            ("train", "zen"),
            ("hazard", "zen"),
            ("overdrive", "classic"),
            ("", "blitz"),
        ] {
            let mut t = VibeCodedHopper::new();
            t.start(mode, 11);
            t.dev_preset(preset);
            for _ in 0..(20 * 60) {
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
        let t = VibeCodedHopper::new();
        assert_eq!((t.id(), t.title()), ("hopper", "VIBECODED HOPPER"));
        assert_eq!(t.modes().len(), 4);
        assert!(t.modes().iter().any(|m| m.is_daily));
        assert!(!t.controls_legend().is_empty());
    }
}
