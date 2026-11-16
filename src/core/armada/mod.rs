//! VibeCoded Armada: pure, deterministic shooter rules (no `macroquad`).

pub mod bot;
pub mod bunker;
pub mod game;
pub mod geom;
pub mod waves;

pub use game::{ArmadaEvent, ArmadaGame, ArmadaInput, ArmadaMode, Status};
pub use geom::Aabb;
