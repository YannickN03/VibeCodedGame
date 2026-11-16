//! VibeCoded Maze: pure, deterministic maze-chaser rules (no `macroquad`).

pub mod bot;
pub mod game;
pub mod ghost;
#[allow(clippy::module_inception)]
pub mod maze;

pub use game::{MazeEvent, MazeGame, MazeInput, MazeMode, Status};
pub use ghost::Dir;
