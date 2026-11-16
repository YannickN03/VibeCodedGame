//! The games of the arcade. `default_registry` is the one place that names
//! concrete titles; everything else works on `dyn ArcadeTitle`.

pub mod armada;
pub mod blocks;
pub mod glide;
pub mod hopper;
pub mod snake;

use crate::arcade::GameRegistry;

/// All games shipped with the arcade, in menu order.
pub fn default_registry() -> GameRegistry {
    let mut r = GameRegistry::new();
    r.register(Box::new(blocks::BlocksTitle::new()));
    r.register(Box::new(snake::SnakeTitle::new()));
    r.register(Box::new(armada::VibeCodedArmada::new()));
    r.register(Box::new(glide::VibeCodedGlide::new()));
    r.register(Box::new(hopper::VibeCodedHopper::new()));
    r
}
