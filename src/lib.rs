//! VibeCoded Blocks — a juicy, deterministic block-stacking puzzle game.
//!
//! Architecture (strict 4-layer separation):
//!
//! 1. [`core`] — pure domain logic (no `macroquad`).
//! 2. [`engine`] — frame-independent simulation & abstract input handling.
//! 3. [`presentation`] — rendering, shaders, particles and procedural audio.
//! 4. [`infrastructure`] — persistence, clock, CLI / smoke test.

#![forbid(unsafe_code)]

pub mod core;
pub mod engine;
pub mod infrastructure;
pub mod presentation;
