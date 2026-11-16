//! Entry point: `--smoke-test` runs headless; otherwise the window opens.

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use vibecoded_game::infrastructure::{Cli, smoke};
use vibecoded_game::presentation::{app, audio, window_conf};

fn main() {
    let cli = Cli::parse(std::env::args());
    if cli.smoke_test {
        // Fully headless: no window, no GPU, no audio device needed.
        let report = smoke::run(500);
        println!("{report}");
        std::process::exit(0);
    }
    audio::install_audio_panic_guard();
    macroquad::Window::from_config(window_conf(), app::run(cli.demo));
}
