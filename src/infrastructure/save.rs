//! Highscores & settings persisted as JSON under
//! `dirs::config_dir()/VibeCodedGame/save.json`
//! (`%APPDATA%\VibeCodedGame\save.json` on Windows).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::GameMode;

/// User-tunable settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub das_ms: u32,
    pub arr_ms: u32,
    pub muted: bool,
    /// Master volume `0..=100`.
    pub volume: u32,
    pub theme: usize,
    pub colorblind: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            das_ms: 130,
            arr_ms: 30,
            muted: false,
            volume: 70,
            theme: 0,
            colorblind: false,
        }
    }
}

impl Settings {
    pub const DAS_RANGE: (u32, u32) = (40, 300);
    pub const ARR_RANGE: (u32, u32) = (0, 100);

    /// Clamps every value into its valid range (protects against edited files).
    pub fn sanitized(mut self) -> Self {
        self.das_ms = self.das_ms.clamp(Self::DAS_RANGE.0, Self::DAS_RANGE.1);
        self.arr_ms = self.arr_ms.clamp(Self::ARR_RANGE.0, Self::ARR_RANGE.1);
        self.volume = self.volume.min(100);
        self
    }

    pub fn handling(&self) -> crate::engine::Handling {
        crate::engine::Handling {
            das: self.das_ms as f32 / 1000.0,
            arr: self.arr_ms as f32 / 1000.0,
        }
    }
}

/// Everything that is persisted.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveData {
    pub version: u32,
    pub settings: Settings,
    /// Best score per mode key (Marathon, Zen, Daily overall).
    pub best_scores: BTreeMap<String, u64>,
    /// Best 40-line sprint time in milliseconds.
    pub best_sprint_ms: Option<u64>,
    /// Daily Vibe best score per ISO date.
    pub daily: BTreeMap<String, u64>,
}

/// Result of a finished run, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRecord {
    pub mode: GameMode,
    pub score: u64,
    pub time_ms: u64,
    pub completed: bool,
    pub date_iso: String,
}

impl SaveData {
    pub const VERSION: u32 = 1;

    /// Displayed "HIGHSCORE" for a mode (Sprint: best time in ms).
    pub fn best_for(&self, mode: GameMode, date_iso: &str) -> Option<u64> {
        match mode {
            GameMode::Sprint => self.best_sprint_ms,
            GameMode::DailyVibe => self.daily.get(date_iso).copied(),
            m => self.best_scores.get(m.key()).copied(),
        }
    }

    /// Stores a run; returns `true` if it is a new record.
    pub fn record(&mut self, run: &RunRecord) -> bool {
        match run.mode {
            GameMode::Sprint => {
                if !run.completed {
                    return false;
                }
                let better = self.best_sprint_ms.is_none_or(|b| run.time_ms < b);
                if better {
                    self.best_sprint_ms = Some(run.time_ms);
                }
                better
            }
            mode => {
                let mut new_record = false;
                let best = self.best_scores.entry(mode.key().to_string()).or_insert(0);
                if run.score > *best {
                    *best = run.score;
                    new_record = mode != GameMode::DailyVibe;
                }
                if mode == GameMode::DailyVibe {
                    let d = self.daily.entry(run.date_iso.clone()).or_insert(0);
                    if run.score > *d {
                        *d = run.score;
                        new_record = true;
                    }
                }
                new_record && run.score > 0
            }
        }
    }
}

/// File-backed storage. All failures are non-fatal.
#[derive(Clone, Debug)]
pub struct SaveStore {
    path: Option<PathBuf>,
}

impl SaveStore {
    pub const APP_DIR: &'static str = "VibeCodedGame";
    pub const FILE: &'static str = "save.json";

    /// Default location under the OS config dir.
    pub fn default_location() -> Self {
        SaveStore {
            path: dirs::config_dir().map(|d| d.join(Self::APP_DIR).join(Self::FILE)),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        SaveStore {
            path: Some(path.into()),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Loads or falls back to defaults (missing/corrupt file never crashes).
    pub fn load(&self) -> SaveData {
        let Some(path) = &self.path else {
            return SaveData::default();
        };
        let mut data = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<SaveData>(&s).ok())
            .unwrap_or_default();
        data.settings = data.settings.sanitized();
        data
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&self, data: &SaveData) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut data = data.clone();
        data.version = SaveData::VERSION;
        let json = serde_json::to_string_pretty(&data).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: GameMode, score: u64, ms: u64, done: bool) -> RunRecord {
        RunRecord {
            mode,
            score,
            time_ms: ms,
            completed: done,
            date_iso: "2026-11-16".into(),
        }
    }

    #[test]
    fn roundtrip_and_corrupt_fallback() {
        let dir = std::env::temp_dir().join(format!("vcg-test-{}", std::process::id()));
        let store = SaveStore::at(dir.join("save.json"));
        let mut d = SaveData::default();
        d.settings.das_ms = 99;
        d.record(&run(GameMode::Marathon, 1234, 0, false));
        store.save(&d).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.settings.das_ms, 99);
        assert_eq!(loaded.best_for(GameMode::Marathon, ""), Some(1234));
        fs::write(dir.join("save.json"), "{ not json").unwrap();
        assert_eq!(store.load(), SaveData::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn records() {
        let mut d = SaveData::default();
        assert!(d.record(&run(GameMode::Sprint, 0, 60_000, true)));
        assert!(!d.record(&run(GameMode::Sprint, 0, 70_000, true)));
        assert!(!d.record(&run(GameMode::Sprint, 0, 10_000, false)));
        assert!(d.record(&run(GameMode::Sprint, 0, 50_000, true)));
        assert_eq!(d.best_for(GameMode::Sprint, ""), Some(50_000));

        assert!(d.record(&run(GameMode::DailyVibe, 500, 0, false)));
        assert_eq!(d.best_for(GameMode::DailyVibe, "2026-11-16"), Some(500));
        assert_eq!(d.best_for(GameMode::DailyVibe, "2026-11-17"), None);
    }

    #[test]
    fn settings_sanitized() {
        let s = Settings {
            das_ms: 5,
            arr_ms: 999,
            volume: 300,
            ..Default::default()
        }
        .sanitized();
        assert_eq!((s.das_ms, s.arr_ms, s.volume), (40, 100, 100));
    }
}
