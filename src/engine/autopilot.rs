//! A small deterministic bot that plays through the real input pipeline.
//!
//! Used by the headless `--smoke-test` (and handy for attract mode). It only
//! produces [`ButtonSet`]s — exactly what a keyboard would — so the whole
//! edge/DAS/simulation chain is exercised.

use crate::core::GameState;
use crate::core::board::{Board, HEIGHT, WIDTH, drop_position, is_valid_position};
use crate::core::piece::{Piece, PieceKind, Rotation};

use super::input::{Button, ButtonSet};

/// A target placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub rot: Rotation,
    pub x: i32,
    pub use_hold: bool,
    pub score: f32,
}

fn column_heights(board: &Board) -> [i32; WIDTH] {
    let mut h = [0; WIDTH];
    for (x, hx) in h.iter_mut().enumerate() {
        for y in 0..HEIGHT {
            if !board[y][x].is_empty() {
                *hx = (HEIGHT - y) as i32;
                break;
            }
        }
    }
    h
}

fn count_holes(board: &Board) -> i32 {
    let mut holes = 0;
    for x in 0..WIDTH {
        let mut seen = false;
        for y in 0..HEIGHT {
            if !board[y][x].is_empty() {
                seen = true;
            } else if seen {
                holes += 1;
            }
        }
    }
    holes
}

/// Classic 4-feature evaluation (aggregate height, lines, holes, bumpiness).
fn evaluate(board: &Board, lines: usize) -> f32 {
    let h = column_heights(board);
    let agg: i32 = h.iter().sum();
    let bump: i32 = h.windows(2).map(|w| (w[0] - w[1]).abs()).sum();
    -0.51 * agg as f32 + 0.76 * lines as f32 - 0.36 * count_holes(board) as f32 - 0.18 * bump as f32
}

fn best_for_kind(board: &Board, kind: PieceKind, start_y: i32) -> Option<(Rotation, i32, f32)> {
    let rots = if kind == PieceKind::O { 1 } else { 4 };
    let mut best: Option<(Rotation, i32, f32)> = None;
    for r in 0..rots {
        let rot = Rotation::from_index(r);
        for x in -3..WIDTH as i32 {
            let p = Piece::new(kind, rot, x, start_y);
            if !is_valid_position(board, &p) {
                continue;
            }
            let landed = drop_position(board, &p);
            let mut b = board.clone();
            b.place(&landed);
            let lines = b.clear_full_rows().len();
            let s = evaluate(&b, lines);
            if best.is_none_or(|(_, _, bs)| s > bs) {
                best = Some((rot, x, s));
            }
        }
    }
    best
}

/// Chooses the best placement for the current piece (optionally via hold).
pub fn plan(game: &GameState) -> Option<Placement> {
    let y = game.active.y;
    let own = best_for_kind(&game.board, game.active.kind, y).map(|(rot, x, score)| Placement {
        rot,
        x,
        use_hold: false,
        score,
    });
    if !game.can_hold {
        return own;
    }
    let alt_kind = game
        .hold
        .or_else(|| game.next_queue(1).first().copied())
        .unwrap_or(game.active.kind);
    let alt = best_for_kind(&game.board, alt_kind, y).map(|(rot, x, score)| Placement {
        rot,
        x,
        use_hold: true,
        score,
    });
    match (own, alt) {
        (Some(o), Some(a)) if a.score > o.score + 0.5 => Some(a),
        (Some(o), _) => Some(o),
        (None, a) => a,
    }
}

/// Closed-loop controller that taps keys until the piece is in place.
#[derive(Clone, Debug, Default)]
pub struct AutoPilot {
    target: Option<Placement>,
    piece_marker: Option<(u32, PieceKind, bool)>,
    /// Remaining idle (release) frames before the next tap.
    cooldown: u32,
    /// Release frames between taps (1 = as fast as edges allow).
    pub pace: u32,
    frames_on_piece: u32,
    /// Force a hold on the very first piece (exercises the hold path).
    pub force_first_hold: bool,
    holds_done: u32,
}

impl AutoPilot {
    pub fn new(force_first_hold: bool) -> Self {
        AutoPilot {
            force_first_hold,
            pace: 1,
            ..Default::default()
        }
    }

    /// A calmer bot for the attract-mode demo.
    pub fn relaxed(pace: u32) -> Self {
        AutoPilot {
            pace: pace.max(1),
            ..Default::default()
        }
    }

    /// Produces the held-button set for the next frame.
    pub fn next_input(&mut self, game: &GameState) -> ButtonSet {
        if !game.is_playing() {
            return ButtonSet::EMPTY;
        }
        // Alternate press / release frames so every tap is a rising edge.
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return ButtonSet::EMPTY;
        }
        let marker = (game.pieces_placed, game.active.kind, game.can_hold);
        if self.piece_marker != Some(marker) {
            self.piece_marker = Some(marker);
            self.frames_on_piece = 0;
            self.target = plan(game);
            if self.force_first_hold
                && self.holds_done == 0
                && game.can_hold
                && let Some(t) = self.target.as_mut()
            {
                t.use_hold = true;
            }
        }
        self.frames_on_piece += 1;
        self.cooldown = self.pace.max(1);

        let press = |b: Button| ButtonSet::EMPTY.with(b);
        let Some(t) = self.target else {
            return press(Button::HardDrop);
        };
        if self.frames_on_piece > 20 {
            return press(Button::HardDrop);
        }
        if t.use_hold && game.can_hold {
            self.holds_done += 1;
            return press(Button::Hold);
        }
        let a = &game.active;
        if a.rot != t.rot {
            return if a.rot.ccw() == t.rot {
                press(Button::RotateCcw)
            } else {
                press(Button::RotateCw)
            };
        }
        if a.x < t.x {
            return press(Button::Right);
        }
        if a.x > t.x {
            return press(Button::Left);
        }
        press(Button::HardDrop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::GameMode;
    use crate::engine::das::Handling;
    use crate::engine::sim::Simulation;

    #[test]
    fn bot_clears_lines() {
        let mut sim = Simulation::new(GameMode::Marathon, 2024, Handling::default());
        let mut bot = AutoPilot::new(true);
        for _ in 0..3000 {
            let input = bot.next_input(&sim.game);
            sim.update(input, 1.0 / 60.0);
        }
        assert!(sim.game.lines >= 10, "bot cleared {} lines", sim.game.lines);
        assert!(sim.game.is_playing());
    }
}
