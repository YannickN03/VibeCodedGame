//! Layer 2 — simulation & input.
//!
//! Frame-independent fixed-timestep simulation (frame `dt` capped at 50 ms),
//! rising-edge detection, DAS/ARR with SOCD last-input priority. Like `core`,
//! this layer does not depend on `macroquad`: input arrives as an abstract
//! [`input::ButtonSet`], which keeps everything testable and headless.

pub mod autopilot;
pub mod das;
pub mod input;
pub mod sim;

pub use das::Handling;
pub use input::{Button, ButtonSet};
pub use sim::Simulation;
