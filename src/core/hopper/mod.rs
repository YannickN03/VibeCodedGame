//! VibeCoded Hopper: pure, deterministic endless-hopper rules (no `macroquad`).

pub mod bot;
pub mod game;
pub mod geom;
pub mod world;

pub use game::{Cause, Dir, HopperEvent, HopperGame, HopperInput, HopperMode, Status};
