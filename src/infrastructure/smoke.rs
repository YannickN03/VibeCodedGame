//! Headless `--smoke-test`: iterates over **every** game of the registry,
//! plays each of its modes with the game's own bot for a number of ticks
//! (asserting the game's invariants), drives the arcade navigation back to
//! the main menu and validates audio and persistence. No window, no GPU and
//! no audio device are needed.

use crate::arcade::nav::{END_DELAY, Nav, Screen, Selection};
use crate::arcade::{ArcadeInput, GameRegistry};
use crate::infrastructure::save::{SaveData, Settings};
use crate::presentation::{music, synth};
use crate::titles;

/// Ticks simulated per game mode.
pub const TICKS_PER_MODE: usize = 300;

/// What was exercised for one game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameReport {
    pub id: &'static str,
    pub modes: usize,
    pub ticks: usize,
}

/// Summary printed after a successful smoke test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmokeReport {
    pub games: Vec<GameReport>,
    pub sounds: usize,
    pub music_tracks: usize,
    pub menu_returns: usize,
}

impl std::fmt::Display for SmokeReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "smoke-test OK: {} games [", self.games.len())?;
        for (i, g) in self.games.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {} modes x {} ticks", g.id, g.modes, g.ticks)?;
        }
        write!(
            f,
            "], {} returns to the main menu, {} synthesized sounds, {} music tracks",
            self.menu_returns, self.sounds, self.music_tracks
        )
    }
}

/// Smoke test of the shipped registry.
pub fn run(ticks: usize) -> SmokeReport {
    run_registry(&mut titles::default_registry(), ticks)
}

/// Smoke test of any registry (each game with each of its modes).
pub fn run_registry(registry: &mut GameRegistry, ticks: usize) -> SmokeReport {
    assert!(!registry.is_empty(), "no games registered");
    let mut report = SmokeReport::default();
    let mut nav = Nav::default();

    for g in 0..registry.len() {
        let game = registry.get_mut(g).expect("registered game");
        assert!(!game.id().is_empty() && !game.title().is_empty());
        assert!(!game.tagline().is_empty(), "{} has no tagline", game.id());
        assert!(
            !game.controls_legend().is_empty(),
            "{} has no controls",
            game.id()
        );
        let modes = game.modes();
        assert!(!modes.is_empty());

        for (m, mode) in modes.iter().enumerate() {
            nav.start(Selection { game: g, mode: m });
            assert_eq!(nav.screen, Screen::Playing);
            game.start(mode.id, 0x00C0_FFEE);
            for tick in 0..ticks {
                game.simulate_smoke_step(tick);
            }
            // The public frame path must work as well and report sane values.
            for _ in 0..30 {
                let ev = game.update(1.0 / 60.0, &ArcadeInput::EMPTY);
                assert!(
                    (0.0..=100.0).contains(&ev.flow),
                    "{}: flow out of range",
                    game.id()
                );
                assert!(ev.music_stage < music::GAME_STAGES);
            }
            // Pause / resume / end / results / back to the main menu.
            assert!(nav.pause());
            assert!(nav.resume());
            nav.session_ended();
            nav.tick(END_DELAY + 0.1);
            assert_eq!(nav.screen, Screen::Results);
            nav.to_menu();
            assert_eq!(nav.screen, Screen::Menu, "must return to the main menu");
            report.menu_returns += 1;
        }
        report.games.push(GameReport {
            id: game.id(),
            modes: modes.len(),
            ticks,
        });
    }

    // Procedural audio must produce valid WAV files (no device needed).
    let bank = synth::build_bank();
    for (name, wav) in bank.iter() {
        assert!(synth::validate_wav(wav).is_ok(), "invalid WAV '{name}'");
    }
    report.sounds = bank.len();
    for track in music::Track::all() {
        let wav = music::render_wav(track);
        assert!(synth::validate_wav(&wav).is_ok(), "invalid music {track:?}");
        report.music_tracks += 1;
    }

    // Persistence format roundtrip (in memory, nothing written to disk).
    let mut save = SaveData::default();
    save.settings = Settings::default();
    save.highscores.insert("blocks:marathon".into(), 1234);
    let json = serde_json::to_string(&save).expect("serialize save");
    let back: SaveData = serde_json::from_str(&json).expect("deserialize save");
    assert_eq!(back, save);

    report
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test_passes_for_every_registered_game() {
        let r = super::run(120);
        assert_eq!(r.games.len(), crate::titles::default_registry().len());
        assert!(r.menu_returns >= r.games.len());
    }
}
