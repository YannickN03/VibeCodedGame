//! The arcade platform: everything a game needs to plug into the menu, the
//! pause/results overlays, audio, effects and persistence.
//!
//! A game implements [`ArcadeTitle`] and is registered once (see
//! `titles::default_registry`). The controller only ever talks to
//! `Box<dyn ArcadeTitle>`, so adding a game never touches the menu, the save
//! system or the game loop. See `ARCHITECTURE.md`, chapter "Neues Spiel
//! hinzufügen".

pub mod events;
pub mod input;
pub mod nav;
pub mod registry;

use macroquad::prelude::{Color, Rect};

use crate::infrastructure::save::Settings;
use crate::presentation::stage::Stage;
use crate::presentation::theme::Theme;

pub use events::{AudioCue, CalloutSpec, FrameEvents, ParticleBurst, SessionEnd, cue};
pub use input::{ArcadeInput, Key};
pub use nav::{Nav, Screen, Selection};
pub use registry::GameRegistry;

/// The virtual 1280×720 stage a title draws on.
pub type VirtualCanvas = Stage;

/// Static description of one game mode (shown on the menu card).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameModeInfo {
    /// Stable id, used for persistence keys (`"marathon"`, `"classic"`, …).
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Everybody plays the same seed (today's date) and bests are kept per day.
    pub is_daily: bool,
    /// The stored value is a time in milliseconds (smaller is better).
    pub ranked_by_time: bool,
}

impl GameModeInfo {
    pub const fn new(id: &'static str, label: &'static str, description: &'static str) -> Self {
        GameModeInfo {
            id,
            label,
            description,
            is_daily: false,
            ranked_by_time: false,
        }
    }

    pub const fn daily(mut self) -> Self {
        self.is_daily = true;
        self
    }

    pub const fn timed(mut self) -> Self {
        self.ranked_by_time = true;
        self
    }
}

/// Per-frame data the controller hands to the running title before `update`.
#[derive(Clone, Debug, Default)]
pub struct TitleContext {
    /// Personal best of the running mode (for the HUD).
    pub best: Option<u64>,
    /// Today's date, `YYYY-MM-DD`.
    pub today_iso: String,
    /// Beat pulse `0..1` of the background shader.
    pub pulse: f32,
    pub settings: Settings,
}

/// A game that can be played inside the arcade.
///
/// Object safe: the registry stores `Box<dyn ArcadeTitle>`. Implementations
/// must not call into the windowing context from `start`, `update` and
/// `simulate_smoke_step` (they run headless in `--smoke-test`); only `draw`
/// and `draw_preview` may render.
pub trait ArcadeTitle {
    /// Stable machine id (`"blocks"`, `"snake"`); prefix of persistence keys.
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn tagline(&self) -> &'static str;
    fn accent_color(&self) -> Color;
    fn modes(&self) -> &'static [GameModeInfo];
    /// `(keys, action)` pairs shown in the pause overlay.
    fn controls_legend(&self) -> &'static [(&'static str, &'static str)];

    /// Resets the game completely and starts `mode_id` with `seed`.
    fn start(&mut self, mode_id: &str, seed: u64);
    /// Advances one rendered frame (`dt` in seconds).
    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents;
    fn draw(&self, canvas: &VirtualCanvas, theme: &Theme, colorblind: bool);
    /// Lively mini animation for the menu card. Pure function of `time`.
    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme);
    /// One headless bot tick for the automated smoke test. Must assert the
    /// game's invariants and may restart the game after it ended.
    fn simulate_smoke_step(&mut self, tick: usize);

    /// Receives settings, best score and beat pulse before every `update`.
    fn set_context(&mut self, _ctx: &TitleContext) {}
    /// Called when the game resumes from pause (held keys must not re-trigger).
    fn on_resume(&mut self, _input: &ArcadeInput) {}
    /// Developer aid (`--preset <name>`): jump into a named situation, e.g. a
    /// boss fight, for visual checks. Unknown names are ignored.
    fn dev_preset(&mut self, _name: &str) {}
    /// Attract mode: a bot plays instead of the keyboard. `None` = unsupported.
    fn demo_update(&mut self, _dt: f32) -> Option<FrameEvents> {
        None
    }
}
