//! VibeCoded Snake: pure, deterministic rules (no rendering, no `macroquad`).

pub mod bot;
pub mod game;
pub mod grid;
pub mod interp;
pub mod obstacles;
pub mod turn;

pub use game::{Collision, SnakeEvent, SnakeGame, SnakeMode, Status};
pub use grid::{Direction, GridPos};
pub use turn::TurnBuffer;
