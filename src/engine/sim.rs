//! Frame-independent fixed-timestep simulation driving [`GameState`].

use crate::core::piece::RotateDir;
use crate::core::{GameEvent, GameMode, GameState};

use super::das::{DasHandler, Handling};
use super::input::{Button, ButtonSet, EdgeDetector};

/// Fixed simulation step (240 Hz keeps DAS/ARR precise).
pub const FIXED_DT: f32 = 1.0 / 240.0;
/// Frame delta cap: protects against spiral-of-death after stalls.
pub const MAX_FRAME_DT: f32 = 0.05;

/// What the caller needs to know after a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameOutput {
    pub pause_pressed: bool,
    pub steps: u32,
    pub events: Vec<GameEvent>,
}

/// Owns the game and turns held buttons + frame time into game actions.
#[derive(Clone, Debug)]
pub struct Simulation {
    pub game: GameState,
    pub handling: Handling,
    das: DasHandler,
    edges: EdgeDetector,
    pending: ButtonSet,
    accumulator: f32,
}

impl Simulation {
    pub fn new(mode: GameMode, seed: u64, handling: Handling) -> Self {
        Simulation {
            game: GameState::new(mode, seed),
            handling,
            das: DasHandler::default(),
            edges: EdgeDetector::default(),
            pending: ButtonSet::EMPTY,
            accumulator: 0.0,
        }
    }

    /// Call when resuming from pause so held keys don't re-trigger.
    pub fn latch_input(&mut self, held: ButtonSet) {
        self.edges.latch(held);
        self.pending = ButtonSet::EMPTY;
    }

    /// Advances by one rendered frame.
    pub fn update(&mut self, held: ButtonSet, frame_dt: f32) -> FrameOutput {
        let dt = if frame_dt.is_finite() {
            frame_dt.clamp(0.0, MAX_FRAME_DT)
        } else {
            0.0
        };
        let pressed = self.edges.update(held);
        let mut out = FrameOutput::default();
        if pressed.contains(Button::Pause) {
            out.pause_pressed = true;
            out.events = self.game.drain_events();
            return out;
        }
        self.pending = self.pending.union(pressed);
        self.accumulator += dt;
        while self.accumulator >= FIXED_DT {
            self.accumulator -= FIXED_DT;
            let edges = std::mem::take(&mut self.pending);
            self.step(held, edges);
            out.steps += 1;
        }
        out.events = self.game.drain_events();
        out
    }

    /// One fixed simulation step.
    fn step(&mut self, held: ButtonSet, pressed: ButtonSet) {
        let g = &mut self.game;
        if !g.is_playing() {
            return;
        }
        if pressed.contains(Button::Hold) {
            g.hold();
        }
        if pressed.contains(Button::RotateCw) {
            g.rotate(RotateDir::Cw);
        }
        if pressed.contains(Button::RotateCcw) {
            g.rotate(RotateDir::Ccw);
        }
        if pressed.contains(Button::Rotate180) {
            g.rotate(RotateDir::Half);
        }

        if let Some(shift) = self.das.update(
            held.contains(Button::Left),
            held.contains(Button::Right),
            pressed.contains(Button::Left),
            pressed.contains(Button::Right),
            FIXED_DT,
            self.handling,
        ) {
            let mut n = 0;
            while n < shift.cells && n < 16 && g.move_horizontal(shift.dir.dx()) {
                n += 1;
            }
        }

        g.set_soft_drop(held.contains(Button::SoftDrop));

        if pressed.contains(Button::HardDrop) {
            g.hard_drop();
        }

        g.tick(FIXED_DT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Simulation {
        Simulation::new(GameMode::Marathon, 77, Handling::default())
    }

    #[test]
    fn holding_space_drops_only_once() {
        let mut s = sim();
        let held = ButtonSet::EMPTY.with(Button::HardDrop);
        for _ in 0..60 {
            s.update(held, 1.0 / 60.0);
        }
        assert_eq!(s.game.pieces_placed, 1);
    }

    #[test]
    fn holding_rotate_rotates_only_once() {
        let mut s = sim();
        let held = ButtonSet::EMPTY.with(Button::RotateCw);
        let rot = s.game.active.rot;
        for _ in 0..30 {
            s.update(held, 1.0 / 60.0);
        }
        if s.game.active.kind != crate::core::PieceKind::O {
            assert_eq!(s.game.active.rot, rot.cw());
        }
    }

    #[test]
    fn dt_is_capped() {
        let mut s = sim();
        let out = s.update(ButtonSet::EMPTY, 10.0);
        assert!(out.steps as f32 * FIXED_DT <= MAX_FRAME_DT + 1e-4);
    }

    #[test]
    fn short_frames_keep_pending_edges() {
        let mut s = sim();
        // Press with a frame too short for a fixed step, release next frame.
        s.update(ButtonSet::EMPTY.with(Button::HardDrop), 0.001);
        s.update(ButtonSet::EMPTY, 1.0 / 60.0);
        assert_eq!(s.game.pieces_placed, 1);
    }

    #[test]
    fn pause_edge_reported() {
        let mut s = sim();
        let out = s.update(ButtonSet::EMPTY.with(Button::Pause), 0.016);
        assert!(out.pause_pressed);
        let out = s.update(ButtonSet::EMPTY.with(Button::Pause), 0.016);
        assert!(!out.pause_pressed);
    }
}
