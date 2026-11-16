//! Headless `--smoke-test`: runs the real simulation + input pipeline without
//! opening a window, checks invariants with `assert!` and returns a report.

use crate::core::board::{WIDTH, is_valid_position};
use crate::core::flow::FLOW_MAX;
use crate::core::game::MAX_LOCK_RESETS;
use crate::core::{GameEvent, GameMode};
use crate::engine::autopilot::AutoPilot;
use crate::engine::{Handling, Simulation};
use crate::presentation::synth;

use super::save::{SaveData, Settings};

/// Summary printed after a successful smoke test.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmokeReport {
    pub ticks: u32,
    pub pieces: u32,
    pub lines: u32,
    pub score: u64,
    pub peak_flow: f32,
    pub moves: u32,
    pub rotations: u32,
    pub holds: u32,
    pub hard_drops: u32,
    pub sounds: usize,
}

impl std::fmt::Display for SmokeReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "smoke-test OK: {} ticks, {} pieces, {} lines, score {}, peak flow {:.1}, \
             {} moves, {} rotations, {} holds, {} hard drops, {} synthesized sounds",
            self.ticks,
            self.pieces,
            self.lines,
            self.score,
            self.peak_flow,
            self.moves,
            self.rotations,
            self.holds,
            self.hard_drops,
            self.sounds
        )
    }
}

/// Runs `ticks` simulated frames (1/60 s each). Panics on any violated invariant.
pub fn run(ticks: u32) -> SmokeReport {
    let mut report = SmokeReport::default();
    let mut sim = Simulation::new(GameMode::Marathon, 0x00C0_FFEE, Handling::default());
    let mut bot = AutoPilot::new(true);
    let mut last_score = 0;
    let mut last_lines = 0;

    for tick in 0..ticks {
        let input = bot.next_input(&sim.game);
        let out = sim.update(input, 1.0 / 60.0);
        report.ticks = tick + 1;

        for e in &out.events {
            match e {
                GameEvent::Moved => report.moves += 1,
                GameEvent::Rotated { .. } => report.rotations += 1,
                GameEvent::Held(_) => report.holds += 1,
                GameEvent::HardDropped { .. } => report.hard_drops += 1,
                _ => {}
            }
        }

        let g = &sim.game;
        assert!(g.is_playing(), "bot topped out at tick {tick}");
        assert!(
            is_valid_position(&g.board, &g.active),
            "active piece overlaps the matrix at tick {tick}"
        );
        assert!((0.0..=FLOW_MAX).contains(&g.flow.flow), "flow out of range");
        assert!(g.score.score >= last_score, "score decreased");
        assert!(g.lines >= last_lines, "line count decreased");
        assert!(
            g.lock_reset_count <= MAX_LOCK_RESETS,
            "lock reset budget exceeded"
        );
        assert!(g.board.full_rows().is_empty(), "uncleared full row");
        assert_eq!(
            g.board.occupied_count() as u32,
            4 * g.pieces_placed - 10 * g.lines,
            "cell conservation violated"
        );
        assert!(g.ghost().y >= g.active.y, "ghost above active piece");
        assert_eq!(g.next_queue(4).len(), 4, "preview queue too short");
        last_score = g.score.score;
        last_lines = g.lines;
        report.peak_flow = report.peak_flow.max(g.flow.flow);
    }

    let g = &sim.game;
    report.pieces = g.pieces_placed;
    report.lines = g.lines;
    report.score = g.score.score;

    assert!(
        report.pieces >= 10,
        "too few pieces placed: {}",
        report.pieces
    );
    assert!(report.lines >= 1, "no line clear happened");
    assert!(report.moves > 0, "no horizontal move");
    assert!(report.rotations > 0, "no rotation");
    assert!(report.holds >= 1, "hold never used");
    assert!(report.hard_drops >= 10, "too few hard drops");
    assert!(report.peak_flow > 0.0, "flow never rose");
    assert!(g.board.occupied_count() <= WIDTH * 40);

    // Procedural audio must produce valid WAV files (no device needed).
    let bank = synth::build_bank();
    for (name, wav) in bank.iter() {
        assert!(synth::validate_wav(wav).is_ok(), "invalid WAV '{name}'");
    }
    report.sounds = bank.len();

    // Persistence format roundtrip (in memory, nothing written to disk).
    let mut save = SaveData {
        settings: Settings::default(),
        ..Default::default()
    };
    save.best_scores.insert("marathon".into(), report.score);
    let json = serde_json::to_string(&save).expect("serialize save");
    let back: SaveData = serde_json::from_str(&json).expect("deserialize save");
    assert_eq!(back, save);

    report
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test_passes() {
        let r = super::run(500);
        assert_eq!(r.ticks, 500);
    }
}
