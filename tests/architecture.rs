//! Enforces the architecture: the pure layers must never depend on the
//! rendering framework, shared code must not know individual games, the
//! controller must not name concrete titles, and unsafe code is forbidden
//! crate-wide.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("readable source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

fn src(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(sub)
}

#[test]
fn core_and_engine_do_not_import_macroquad() {
    for layer in ["core", "engine"] {
        let files = rust_files(&src(layer));
        assert!(!files.is_empty(), "layer {layer} has no files");
        for f in files {
            let code = fs::read_to_string(&f).expect("readable file");
            // Comments may *mention* the rule; code lines must not use it.
            for (i, line) in code.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                assert!(
                    !trimmed.contains("macroquad"),
                    "{}:{} references macroquad",
                    f.display(),
                    i + 1
                );
            }
        }
    }
}

#[test]
fn unsafe_code_is_forbidden() {
    for root in ["lib.rs", "main.rs"] {
        let code = fs::read_to_string(src(root)).expect("crate root");
        assert!(
            code.contains("#![forbid(unsafe_code)]"),
            "{root} must forbid unsafe code"
        );
    }
}

#[test]
fn release_binary_has_no_console_window() {
    let main = fs::read_to_string(src("main.rs")).expect("main.rs");
    assert!(main.contains(r#"#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]"#));
}

/// Non-comment lines of all Rust files below `sub`, with their location.
fn code_lines(sub: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    for f in rust_files(&src(sub)) {
        let code = fs::read_to_string(&f).expect("readable file");
        for (i, line) in code.lines().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("//") {
                out.push((f.display().to_string(), i + 1, t.to_string()));
            }
        }
    }
    out
}

#[test]
fn core_has_a_module_per_game() {
    for dir in [
        "shared", "blocks", "snake", "armada", "glide", "hopper", "maze",
    ] {
        assert!(src("core").join(dir).is_dir(), "src/core/{dir} missing");
    }
}

/// Core modules of the games, and the Blocks-internal paths that are also
/// re-exported at `core::` level (`core::board`, `core::piece`, ...).
const GAMES: [&str; 6] = ["blocks", "snake", "armada", "glide", "hopper", "maze"];
const BLOCKS_INTERNALS: [&str; 7] = [
    "core::game",
    "core::board",
    "core::piece",
    "core::srs",
    "core::scoring",
    "core::bag",
    "core::GameMode",
];

#[test]
fn games_do_not_depend_on_each_other_and_shared_knows_no_game() {
    // `core::shared` must not know any game.
    for (file, line, text) in code_lines("core/shared") {
        for g in GAMES {
            for banned in [format!("core::{g}"), format!("super::{g}")] {
                assert!(
                    !text.contains(&banned),
                    "{file}:{line} shared code uses {banned}"
                );
            }
        }
    }
    // No game core may use another game's core.
    for game in GAMES {
        for (file, line, text) in code_lines(&format!("core/{game}")) {
            for other in GAMES.iter().filter(|o| **o != game) {
                let banned = format!("core::{other}");
                assert!(
                    !text.contains(&banned),
                    "{file}:{line} {game} core uses {banned}"
                );
            }
            if game != "blocks" {
                for banned in BLOCKS_INTERNALS {
                    assert!(
                        !text.contains(banned),
                        "{file}:{line} {game} core uses Blocks internals ({banned})"
                    );
                }
            }
        }
    }
}

#[test]
fn controller_menu_and_widgets_do_not_name_concrete_titles() {
    let concrete = [
        "BlocksTitle",
        "SnakeTitle",
        "VibeCodedArmada",
        "VibeCodedGlide",
        "VibeCodedHopper",
        "VibeCodedMaze",
        "titles::blocks",
        "titles::snake",
        "titles::armada",
        "titles::glide",
        "titles::hopper",
        "titles::maze",
    ];
    // The controller may only call `titles::default_registry`.
    for dir in ["presentation", "arcade", "infrastructure"] {
        for (file, line, text) in code_lines(dir) {
            if file.ends_with("registry.rs") && text.contains("Dummy") {
                continue; // test doubles inside the registry unit tests
            }
            for name in concrete {
                assert!(
                    !text.contains(name),
                    "{file}:{line} names the concrete title '{name}'; use dyn ArcadeTitle"
                );
            }
        }
    }
}

#[test]
fn only_the_titles_module_registers_concrete_games() {
    let code = fs::read_to_string(src("titles/mod.rs")).expect("titles/mod.rs");
    assert!(code.contains("BlocksTitle") && code.contains("SnakeTitle"));
    for title in [
        "BlocksTitle",
        "SnakeTitle",
        "VibeCodedArmada",
        "VibeCodedGlide",
        "VibeCodedHopper",
        "VibeCodedMaze",
    ] {
        assert!(code.contains(title), "{title} is not registered");
    }
    assert!(code.contains("register("));
}
