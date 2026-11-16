//! Highscores & settings persisted as JSON under
//! `dirs::config_dir()/VibeCodedGame/save.json`
//! (`%APPDATA%\VibeCodedGame\save.json` on Windows).

use std::collections::{BTreeMap, HashMap};
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
    /// Snake: hold the boost key to move faster.
    pub snake_boost: bool,
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
            snake_boost: true,
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
///
/// Highscores are generic key/value pairs so new games need no new fields:
/// `"blocks:marathon"`, `"snake:classic"`, `"daily:snake:2026-11-16"`.
/// The three legacy fields below are only read (to migrate version-1 save
/// files) and are never written back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveData {
    pub version: u32,
    pub settings: Settings,
    pub highscores: HashMap<String, u64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    best_scores: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    best_sprint_ms: Option<u64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    daily: BTreeMap<String, u64>,
}

/// Result of a finished Blocks run, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRecord {
    pub mode: GameMode,
    pub score: u64,
    pub time_ms: u64,
    pub completed: bool,
    pub date_iso: String,
}

/// Key of a mode's overall best, e.g. `"snake:classic"`.
pub fn score_key(game: &str, mode: &str) -> String {
    format!("{game}:{mode}")
}

/// Key of a daily run's best for one date, e.g. `"daily:snake:2026-11-16"`.
pub fn daily_key(game: &str, date_iso: &str) -> String {
    format!("daily:{game}:{date_iso}")
}

impl SaveData {
    pub const VERSION: u32 = 2;

    /// Moves values of the version-1 layout into [`Self::highscores`].
    pub fn migrate_legacy(&mut self) {
        for (mode, v) in std::mem::take(&mut self.best_scores) {
            let e = self
                .highscores
                .entry(score_key("blocks", &mode))
                .or_insert(0);
            *e = (*e).max(v);
        }
        if let Some(ms) = self.best_sprint_ms.take() {
            let e = self
                .highscores
                .entry(score_key("blocks", "sprint"))
                .or_insert(ms);
            *e = (*e).min(ms);
        }
        for (date, v) in std::mem::take(&mut self.daily) {
            let e = self
                .highscores
                .entry(daily_key("blocks", &date))
                .or_insert(0);
            *e = (*e).max(v);
        }
    }

    pub fn highscore(&self, key: &str) -> Option<u64> {
        self.highscores.get(key).copied()
    }

    /// Stores `value` under `key` if it beats the stored one. Returns `true`
    /// for a new record. A score of `0` is never a record.
    pub fn submit(&mut self, key: &str, value: u64, lower_is_better: bool) -> bool {
        if !lower_is_better && value == 0 {
            return false;
        }
        let better = match self.highscores.get(key) {
            None => true,
            Some(&old) if lower_is_better => value < old,
            Some(&old) => value > old,
        };
        if better {
            self.highscores.insert(key.to_string(), value);
        }
        better
    }

    /// Records a finished session of any game; returns `true` for a new
    /// record. Daily modes also keep a per-date best, and only that one
    /// decides about the record badge.
    pub fn record_session(
        &mut self,
        game: &str,
        mode: &str,
        is_daily: bool,
        date_iso: &str,
        value: u64,
        lower_is_better: bool,
    ) -> bool {
        let overall = self.submit(&score_key(game, mode), value, lower_is_better);
        if is_daily {
            self.submit(&daily_key(game, date_iso), value, lower_is_better)
        } else {
            overall
        }
    }

    /// Sum of all score-ranked overall bests (the "Arcade score").
    /// `counts` decides per key; per-date daily keys never count.
    pub fn arcade_total(&self, mut counts: impl FnMut(&str) -> bool) -> u64 {
        self.highscores
            .iter()
            .filter(|(k, _)| !k.starts_with("daily:") && counts(k))
            .map(|(_, v)| *v)
            .sum()
    }

    /// Displayed "HIGHSCORE" of a Blocks mode (Sprint: best time in ms).
    pub fn best_for(&self, mode: GameMode, date_iso: &str) -> Option<u64> {
        match mode {
            GameMode::DailyVibe => self.highscore(&daily_key("blocks", date_iso)),
            m => self.highscore(&score_key("blocks", m.key())),
        }
    }

    /// Stores a Blocks run; returns `true` if it is a new record.
    pub fn record(&mut self, run: &RunRecord) -> bool {
        match run.mode {
            GameMode::Sprint => {
                run.completed
                    && self.record_session("blocks", "sprint", false, "", run.time_ms, true)
            }
            mode => self.record_session(
                "blocks",
                mode.key(),
                mode == GameMode::DailyVibe,
                &run.date_iso,
                run.score,
                false,
            ),
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

    /// A store that never touches the disk (screenshot / visual-check runs).
    pub fn disabled() -> Self {
        SaveStore { path: None }
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
    /// Old save files are migrated to the generic highscore layout.
    pub fn load(&self) -> SaveData {
        let Some(path) = &self.path else {
            return SaveData::default();
        };
        let mut data = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<SaveData>(&s).ok())
            .unwrap_or_default();
        data.migrate_legacy();
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
    fn disabled_store_never_writes() {
        let store = SaveStore::disabled();
        assert!(store.path().is_none());
        store.save(&SaveData::default()).unwrap();
        assert_eq!(store.load(), SaveData::default());
    }

    #[test]
    fn records() {
        let mut d = SaveData::default();
        assert!(d.record(&run(GameMode::Sprint, 0, 60_000, true)));
        assert!(!d.record(&run(GameMode::Sprint, 0, 70_000, true)));
        assert!(!d.record(&run(GameMode::Sprint, 0, 10_000, false)));
        assert!(d.record(&run(GameMode::Sprint, 0, 50_000, true)));
        assert_eq!(d.highscore("blocks:sprint"), Some(50_000));

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

    #[test]
    fn legacy_save_is_migrated_to_generic_keys() {
        let old = r#"{
            "version": 1,
            "settings": { "das_ms": 99, "volume": 40 },
            "best_scores": { "marathon": 5000, "zen": 70, "daily": 900 },
            "best_sprint_ms": 61000,
            "daily": { "2026-11-15": 900 }
        }"#;
        let mut d: SaveData = serde_json::from_str(old).unwrap();
        d.migrate_legacy();
        assert_eq!(d.highscore("blocks:marathon"), Some(5000));
        assert_eq!(d.highscore("blocks:zen"), Some(70));
        assert_eq!(d.highscore("blocks:sprint"), Some(61_000));
        assert_eq!(d.highscore("daily:blocks:2026-11-15"), Some(900));
        assert_eq!(d.settings.das_ms, 99);
        assert!(d.settings.snake_boost, "new setting gets its default");
        // Legacy fields are not written back.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("best_scores") && !json.contains("best_sprint_ms"));
        // Idempotent.
        let again: SaveData = serde_json::from_str(&json).unwrap();
        assert_eq!(again, d);
    }

    #[test]
    fn empty_and_unknown_fields_load() {
        let d: SaveData = serde_json::from_str("{}").unwrap();
        assert_eq!(d, SaveData::default());
        let d: SaveData = serde_json::from_str(r#"{"future_field": 1}"#).unwrap();
        assert_eq!(d, SaveData::default());
    }

    #[test]
    fn generic_records_and_arcade_total() {
        let mut d = SaveData::default();
        assert!(d.record_session("snake", "classic", false, "", 300, false));
        assert!(!d.record_session("snake", "classic", false, "", 200, false));
        assert!(!d.record_session("snake", "classic", false, "", 0, false));
        assert!(d.record_session("snake", "daily", true, "2026-11-16", 400, false));
        assert!(!d.record_session("snake", "daily", true, "2026-11-16", 100, false));
        assert!(d.record_session("snake", "daily", true, "2026-11-17", 100, false));
        assert!(d.record_session("blocks", "sprint", false, "", 60_000, true));
        assert!(d.record_session("blocks", "sprint", false, "", 50_000, true));
        assert_eq!(d.highscore("blocks:sprint"), Some(50_000));
        // Time-ranked modes and per-date keys are excluded from the total.
        let total = d.arcade_total(|k| k != "blocks:sprint");
        assert_eq!(total, 300 + 400);
    }
}
