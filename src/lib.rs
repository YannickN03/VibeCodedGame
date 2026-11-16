//! VibeCoded Arcade — a modular arcade platform with VibeCoded Blocks and
//! VibeCoded Snake. New games plug in through [`arcade::ArcadeTitle`].
//!
//! Architecture:
//!
//! 1. [`core`] — pure domain logic (`shared`, `blocks`, `snake`; no `macroquad`).
//! 2. [`engine`] — frame-independent Blocks simulation & abstract input.
//! 3. [`arcade`] — the game interface: trait, events, registry, navigation.
//! 4. [`presentation`] — UI toolkit, menu, controller, shader, audio, music.
//! 5. [`titles`] — the games as `ArcadeTitle` adapters.
//! 6. [`infrastructure`] — persistence, clock, CLI / smoke test.

#![forbid(unsafe_code)]

pub mod arcade;
pub mod core;
pub mod engine;
pub mod infrastructure;
pub mod presentation;
pub mod titles;
