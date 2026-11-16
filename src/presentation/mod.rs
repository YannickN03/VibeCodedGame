//! Layer 3 — presentation: virtual stage, UI toolkit, arcade menu and
//! controller, shader background and procedural audio/music. Together with
//! `arcade` (the game interface) and `titles` it is the part of the crate
//! that touches `macroquad`.

pub mod app;
pub mod armada_ui;
pub mod audio;
pub mod background;
pub mod digits;
pub mod glide_ui;
pub mod icon;
pub mod input_map;
pub mod menu;
pub mod music;
pub mod settings;
pub mod stage;
pub mod synth;
pub mod text;
pub mod theme;
pub mod widgets;

use macroquad::window::Conf;

/// Window title.
pub const TITLE: &str = "VibeCoded Arcade";

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
