//! Enforces the 4-layer architecture: the pure layers must never depend on
//! the rendering framework, and unsafe code is forbidden crate-wide.

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
