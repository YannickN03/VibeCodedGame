//! Layer 1 — pure domain logic.
//!
//! * [`shared`] — PRNG, flow engine and date helpers used by every game.
//! * [`blocks`] — VibeCoded Blocks rules (matrix, SRS, scoring, state).
//! * [`snake`] — VibeCoded Snake rules.
//!
//! This module must stay free of any rendering/windowing dependency
//! (no `macroquad` imports) so it can be unit-tested and run headless.

pub mod armada;
pub mod blocks;
pub mod glide;
pub mod hopper;
pub mod shared;
pub mod snake;

// Stable short paths (`crate::core::board`, `crate::core::rng`, …).
pub use blocks::{bag, board, game, piece, scoring, srs};
pub use shared::{date, flow, rng};

pub use board::{Board, Cell, is_valid_position};
pub use flow::{FlowMeter, FlowTier};
pub use game::{GameEvent, GameMode, GameState, GameStatus, TopOut};
pub use piece::{Piece, PieceKind, RotateDir, Rotation};
