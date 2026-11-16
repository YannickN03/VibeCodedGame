//! Layer 1 — pure domain logic.
//!
//! Matrix, tetrominoes, SRS, randomness, scoring, flow and the game state.
//! This module must stay free of any rendering/windowing dependency
//! (no `macroquad` imports) so it can be unit-tested and run headless.

pub mod board;
pub mod date;
pub mod flow;
pub mod game;
pub mod piece;
pub mod rng;
pub mod scoring;
pub mod srs;

pub use board::{Board, Cell, is_valid_position};
pub use flow::{FlowMeter, FlowTier};
pub use game::{GameEvent, GameMode, GameState, GameStatus, TopOut};
pub use piece::{Piece, PieceKind, RotateDir, Rotation};
