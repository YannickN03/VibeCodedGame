//! Layer 3 — presentation: virtual stage, UI, shader background, juice and
//! procedural audio. The only layer (besides `main.rs`) that touches
//! `macroquad`.

pub mod app;
pub mod audio;
pub mod background;
pub mod blocks;
pub mod digits;
pub mod fx;
pub mod hud;
pub mod icon;
pub mod input_map;
pub mod stage;
pub mod synth;
pub mod text;
pub mod theme;

use macroquad::window::Conf;

/// Window title.
pub const TITLE: &str = "VibeCoded Blocks";

/// Window configuration (1280×720, resizable, high-DPI, 4× MSAA).
pub fn window_conf() -> Conf {
    Conf {
        window_title: TITLE.to_string(),
        window_width: 1280,
        window_height: 720,
        window_resizable: true,
        high_dpi: true,
        sample_count: 4,
        icon: icon::window_icon(),
        ..Default::default()
    }
}
