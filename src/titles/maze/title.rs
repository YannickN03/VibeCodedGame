//! `ArcadeTitle` adapter of VibeCoded Maze: input mapping, events to
//! sound/particles/callouts, session end, smoke bot.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use crate::core::FlowTier;
use crate::core::maze::bot;
use crate::core::maze::game::{LIVES, MazeEvent, MazeGame, MazeInput, MazeMode, Status};
use crate::core::maze::ghost::Dir;
use crate::presentation::digits;
use crate::presentation::maze_ui::{self, UiCtx, screen_pos};
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, themes};

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new(
        "classic",
        "CLASSIC MAZE",
        "Eat every dot, outsmart four ghosts, clear stage after stage.",
    ),
    GameModeInfo::new(
        "blitz",
        "BLITZ FRENZY",
        "120 seconds. Emptied maze halves refill: chain endless ghost combos.",
    ),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "A fresh symmetric maze every day, the same for everyone.",
    )
    .daily(),
    GameModeInfo::new(
        "zen",
        "ZEN",
        "No lives: a ghost touch only drains flow. Just glide and eat.",
    ),
];

const LEGEND: [(&str, &str); 4] = [
    ("ARROWS / WASD", "Steer (turns are buffered)"),
    ("ORBS", "Turn the ghosts blue and eat them"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

/// Arcade input -> Maze input (a direction pressed this frame).
pub fn input_from(input: &ArcadeInput) -> MazeInput {
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
    MazeInput { dir }
}

pub struct VibeCodedMaze {
    game: MazeGame,
    ctx: TitleContext,
    themes: [Theme; 2],
    time: f32,
    reported: bool,
    last_tier: FlowTier,
}

impl Default for VibeCodedMaze {
    fn default() -> Self {
        Self::new()
    }
}

impl VibeCodedMaze {
    pub fn new() -> Self {
        VibeCodedMaze {
            game: MazeGame::new(MazeMode::Classic, 1),
            ctx: TitleContext::default(),
            themes: themes(),
            time: 0.0,
            reported: false,
            last_tier: FlowTier::Chill,
        }
    }

    pub fn game(&self) -> &MazeGame {
        &self.game
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.ctx.settings.theme % self.themes.len()]
    }

    fn advance(&mut self, dt: f32, input: &MazeInput) -> FrameEvents {
        self.time += dt;
        self.game.update(dt, input);
        let mut ev = self.collect_events();
        if !self.game.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    fn player_pos(&self) -> Vec2 {
        screen_pos(self.game.player.x, self.game.player.y)
    }

    fn collect_events(&mut self) -> FrameEvents {
        let theme = self.theme().clone();
        let mut ev = FrameEvents::default();
        let tier = self.game.tier();
        let ti = tier.index();
        let p = self.player_pos();
        let centre = screen_pos(14.0, 11.5);
        for e in self.game.drain_events() {
            match e {
                MazeEvent::Dot { waka } => {
                    ev.sound(
                        if waka {
                            cue::ACTION_TICK
                        } else {
                            cue::ROTATE_OR_TURN
                        },
                        0.16,
                    );
                    if ti > 0 {
                        ev.note(ti, if waka { 1 } else { 3 }, 0.1);
                    }
                }
                MazeEvent::Orb => {
                    ev.burst(p, theme.accent2, 22, 240.0);
                    ev.callout("FRIGHTENED!", p + vec2(0.0, -32.0), theme.accent2, Size::X1);
                    ev.sound(cue::OVERDRIVE_UP, 0.45);
                    ev.flash(theme.accent2, 0.35);
                }
                MazeEvent::GhostEaten {
                    kind,
                    points,
                    chain,
                } => {
                    let col = maze_ui::ghost_color(&theme, kind);
                    ev.burst(p, col, 26, 300.0);
                    ev.callout(
                        format!("+{points}"),
                        p + vec2(0.0, -28.0),
                        theme.pieces[0],
                        if chain >= 3 { Size::X2 } else { Size::X1 },
                    );
                    ev.shake = ev.shake.max(4.0 + chain as f32);
                    ev.sound(cue::CLEAR_OR_EAT, 0.6);
                    ev.note(ti, (chain as usize + 3) % 7, 0.5);
                }
                MazeEvent::LifeLost => {
                    ev.shake = 12.0;
                    ev.flash(theme.danger, 0.8);
                    ev.burst(p, runner_color(), 36, 320.0);
                    ev.sound(cue::GAME_OVER, 0.55);
                }
                MazeEvent::ZenBump => {
                    ev.callout("FLOW LOST", p + vec2(0.0, -34.0), theme.danger, Size::X1);
                    ev.flash(theme.danger, 0.3);
                    ev.sound(cue::ACTION_TICK, 0.5);
                }
                MazeEvent::StageClear { stage } => {
                    ev.callout(format!("STAGE {stage}"), centre, theme.accent, Size::X3);
                    ev.flash(theme.accent, 0.8);
                    ev.sound(cue::OVERDRIVE_UP, 0.8);
                }
                MazeEvent::CoreSpawned => {
                    ev.callout("VIBE-CORE!", centre, theme.pieces[1], Size::X2);
                    ev.sound(cue::OVERDRIVE_UP, 0.4);
                }
                MazeEvent::CoreEaten { points } => {
                    ev.burst(p, theme.pieces[1], 30, 300.0);
                    ev.callout(
                        format!("+{points}"),
                        p + vec2(0.0, -34.0),
                        theme.pieces[1],
                        Size::X2,
                    );
                    ev.sound(cue::CLEAR_OR_EAT, 0.8);
                    ev.note(ti, 6, 0.5);
                }
                MazeEvent::FrightenedEnd | MazeEvent::PhaseChange { .. } | MazeEvent::Ready => {}
                MazeEvent::Regenerated { .. } => {
                    ev.callout("MAZE REFILLED!", centre, theme.accent2, Size::X2);
                    ev.flash(theme.accent2, 0.5);
                    ev.sound(cue::OVERDRIVE_UP, 0.6);
                }
                MazeEvent::GameOver | MazeEvent::TimeUp => {
                    ev.shake = ev.shake.max(8.0);
                    ev.sound(cue::GAME_OVER, 0.8);
                }
            }
        }
        if tier > self.last_tier {
            let c = theme.tiers[ti];
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
        ev.music_stage = maze_ui::music_stage(&self.game);
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.game;
        let title = match g.status {
            Status::TimeUp => "TIME UP",
            Status::Dead => "CAUGHT BY THE GHOSTS",
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
                ("STAGE".into(), g.stage.to_string()),
                ("DOTS".into(), g.dots_eaten.to_string()),
                ("GHOSTS".into(), g.ghosts_eaten.to_string()),
                ("TIME".into(), digits::format_time(g.time as f64)),
            ],
        }
    }

    /// Invariants checked on every headless bot step.
    fn check_invariants(&self) {
        let g = &self.game;
        assert!(
            g.player.x.is_finite() && (0.0..28.0).contains(&g.player.x),
            "x in the maze"
        );
        assert!((0.0..23.0).contains(&g.player.y), "y in the maze");
        let (tx, ty) = g.player.tile();
        assert!(g.maze.player_walkable(tx, ty), "runner inside a wall");
        assert!((0.0..=100.0).contains(&g.flow.flow), "flow out of range");
        assert!(g.dots_left <= g.dots_total, "dot bookkeeping");
    }
}

fn runner_color() -> Color {
    hex(0xFFD84A)
}

impl ArcadeTitle for VibeCodedMaze {
    fn id(&self) -> &'static str {
        "maze"
    }

    fn title(&self) -> &'static str {
        "VIBECODED MAZE"
    }

    fn tagline(&self) -> &'static str {
        "chomp, dodge, turn the tables"
    }

    fn accent_color(&self) -> Color {
        runner_color()
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.game = MazeGame::new(MazeMode::from_id(mode_id), seed);
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
        let i = MazeInput {
            dir: bot::choose(&self.game),
        };
        Some(self.advance(dt, &i))
    }

    fn dev_preset(&mut self, name: &str) {
        let g = &mut self.game;
        g.skip_ready();
        match name {
            // Ghosts out and frightened, the runner mid-maze.
            "frightened" => {
                for _ in 0..(8 * 120) {
                    g.step(&MazeInput::default());
                    if !g.is_playing() {
                        break;
                    }
                }
                g.frighten(g.frightened_secs());
            }
            // The blue phase is almost over: the ghosts blink.
            "blink" => {
                for _ in 0..(8 * 120) {
                    g.step(&MazeInput::default());
                    if !g.is_playing() {
                        break;
                    }
                }
                g.frighten(g.frightened_secs());
                g.fright_t = 1.4;
            }
            "overdrive" => {
                g.flow.flow = 95.0;
            }
            _ => {}
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
        maze_ui::draw_world(&self.game, &ctx);
        maze_ui::draw_hud(&self.game, &ctx);
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        maze_ui::draw_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        // A death of the bot is a regression and must fail loudly.
        assert!(
            self.game.is_playing() && self.game.lives == LIVES,
            "maze bot lost a life at tick {tick}: {:?} lives {}",
            self.game.status,
            self.game.lives
        );
        let i = MazeInput {
            dir: bot::choose(&self.game),
        };
        let ev = self.advance(1.0 / 60.0, &i);
        assert!(ev.flow.is_finite());
        self.check_invariants();
        if tick >= 280 {
            let g = &self.game;
            assert!(g.dots_eaten >= 20 && g.score > 0, "bot made no progress");
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
    fn arrows_and_wasd_steer() {
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
        assert_eq!(input_from(&ArcadeInput::EMPTY).dir, None);
    }

    #[test]
    fn smoke_bot_plays_every_mode_without_losing_a_life() {
        for m in MODES {
            let mut t = VibeCodedMaze::new();
            t.start(m.id, 20261116);
            for tick in 0..300 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().dots_eaten >= 20, "{}", m.id);
        }
    }

    #[test]
    fn game_over_reports_one_session_end_and_zen_never_ends() {
        let mut t = VibeCodedMaze::new();
        t.start("classic", 5);
        let mut ends = Vec::new();
        for _ in 0..(120 * 60) {
            if let Some(e) = t.update(1.0 / 60.0, &ArcadeInput::EMPTY).session_end {
                ends.push(e);
            }
        }
        assert_eq!(ends.len(), 1, "idling gets the runner caught exactly once");
        assert_eq!(ends[0].title, "CAUGHT BY THE GHOSTS");
        assert!(ends[0].counts_for_record && !ends[0].lower_is_better);

        let mut b = VibeCodedMaze::new();
        b.start("blitz", 5);
        let mut blitz_end = None;
        for _ in 0..(125 * 60) {
            if let Some(e) = b.demo_update(1.0 / 60.0).unwrap().session_end {
                blitz_end = Some(e);
                break;
            }
        }
        let e = blitz_end.expect("the Blitz countdown ends the session");
        assert!(e.title == "TIME UP" || e.title == "CAUGHT BY THE GHOSTS");

        let mut z = VibeCodedMaze::new();
        z.start("zen", 5);
        for _ in 0..(60 * 60) {
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
            ("frightened", "classic"),
            ("blink", "classic"),
            ("overdrive", "classic"),
            ("", "blitz"),
            ("", "daily"),
            ("", "zen"),
        ] {
            let mut t = VibeCodedMaze::new();
            t.start(mode, 11);
            t.dev_preset(preset);
            for _ in 0..(30 * 60) {
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
        let t = VibeCodedMaze::new();
        assert_eq!((t.id(), t.title()), ("maze", "VIBECODED MAZE"));
        assert_eq!(t.modes().len(), 4);
        assert!(t.modes().iter().any(|m| m.is_daily));
        assert!(!t.controls_legend().is_empty());
    }
}
