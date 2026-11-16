//! VibeCoded Glide: pure, deterministic endless-flyer rules (no `macroquad`).

pub mod bot;
pub mod course;
pub mod game;
pub mod geom;

pub use game::{GlideEvent, GlideGame, GlideInput, GlideMode, Status};
