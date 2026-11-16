//! What a title reports back to the arcade controller after each frame.
//!
//! The controller turns these into sound, particles, callouts, screen shake
//! and the results screen, so no title talks to the audio device or the
//! shared effect systems directly.

use macroquad::prelude::{Color, Vec2};

use crate::presentation::text::Size;

/// Generic arcade sounds every title can trigger (all exist in the bank).
pub mod cue {
    pub const UI_MOVE: &str = "ui_move";
    pub const UI_CONFIRM: &str = "ui_confirm";
    pub const ACTION_TICK: &str = "action_tick";
    pub const ROTATE_OR_TURN: &str = "rotate_or_turn";
    pub const CLEAR_OR_EAT: &str = "clear_or_eat";
    pub const OVERDRIVE_UP: &str = "overdrive_up";
    pub const GAME_OVER: &str = "game_over";
}

/// A sound to play.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioCue {
    /// A named sound of the bank at relative `gain`.
    Sound { name: &'static str, gain: f32 },
    /// Pentatonic note `idx` with the harmony layers of flow tier `tier`.
    Note { tier: usize, idx: usize, gain: f32 },
}

impl AudioCue {
    pub fn sound(name: &'static str, gain: f32) -> Self {
        AudioCue::Sound { name, gain }
    }
}

/// A burst of particles (virtual stage coordinates).
#[derive(Clone, Debug)]
pub struct ParticleBurst {
    pub pos: Vec2,
    pub color: Color,
    pub count: usize,
    pub speed: f32,
}

/// A floating text callout (virtual stage coordinates).
#[derive(Clone, Debug)]
pub struct CalloutSpec {
    pub text: String,
    pub pos: Vec2,
    pub color: Color,
    pub size: Size,
}

/// A finished session, handed to the results overlay and the save system.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionEnd {
    /// Overlay title, e.g. `"GAME OVER"` or `"CLEAR!"`.
    pub title: &'static str,
    pub won: bool,
    /// Big headline figure, e.g. `("SCORE", "12345")` or `("TIME", "0:58.20")`.
    pub headline: (&'static str, String),
    /// Value stored as highscore (score, or milliseconds for timed modes).
    pub value: u64,
    /// `true` for timed modes where a smaller value is better.
    pub lower_is_better: bool,
    /// `false` for runs that must not be recorded (e.g. unfinished sprint).
    pub counts_for_record: bool,
    /// Extra rows of the statistics table.
    pub summary: Vec<(String, String)>,
}

/// Everything one `update` call reports.
#[derive(Clone, Debug, Default)]
pub struct FrameEvents {
    pub audio: Vec<AudioCue>,
    pub particles: Vec<ParticleBurst>,
    pub callouts: Vec<CalloutSpec>,
    /// Screen shake strength in stage units (`0` = none).
    pub shake: f32,
    /// Full-screen colour flash `(colour, strength 0..1)`.
    pub screen_flash: Option<(Color, f32)>,
    /// Current flow `0..=100` (drives shader intensity and beat tempo).
    pub flow: f32,
    /// Music intensity stage `0..GAME_STAGES`; the music follows the game.
    pub music_stage: usize,
    /// Set exactly once when the session ends.
    pub session_end: Option<SessionEnd>,
}

impl FrameEvents {
    pub fn sound(&mut self, name: &'static str, gain: f32) {
        self.audio.push(AudioCue::sound(name, gain));
    }

    pub fn note(&mut self, tier: usize, idx: usize, gain: f32) {
        self.audio.push(AudioCue::Note { tier, idx, gain });
    }

    pub fn burst(&mut self, pos: Vec2, color: Color, count: usize, speed: f32) {
        self.particles.push(ParticleBurst {
            pos,
            color,
            count,
            speed,
        });
    }

    pub fn callout(&mut self, text: impl Into<String>, pos: Vec2, color: Color, size: Size) {
        self.callouts.push(CalloutSpec {
            text: text.into(),
            pos,
            color,
            size,
        });
    }

    pub fn flash(&mut self, color: Color, strength: f32) {
        self.screen_flash = Some((color, strength));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::synth::build_bank;

    #[test]
    fn every_generic_cue_exists_in_the_sound_bank() {
        let bank = build_bank();
        for name in [
            cue::UI_MOVE,
            cue::UI_CONFIRM,
            cue::ACTION_TICK,
            cue::ROTATE_OR_TURN,
            cue::CLEAR_OR_EAT,
            cue::OVERDRIVE_UP,
            cue::GAME_OVER,
        ] {
            assert!(bank.iter().any(|(n, _)| n == name), "missing sound {name}");
        }
    }

    #[test]
    fn frame_events_collect() {
        let mut e = FrameEvents::default();
        e.sound(cue::UI_MOVE, 0.5);
        e.note(1, 2, 0.3);
        e.flash(Color::new(1.0, 0.0, 0.0, 1.0), 0.5);
        assert_eq!(e.audio.len(), 2);
        assert!(e.screen_flash.is_some() && e.session_end.is_none());
    }
}
