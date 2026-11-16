//! The list of all playable titles.

use crate::infrastructure::save::SaveData;

use super::{ArcadeTitle, GameModeInfo};

/// Owns every registered game. The controller and the menu only use this.
#[derive(Default)]
pub struct GameRegistry {
    games: Vec<Box<dyn ArcadeTitle>>,
}

impl GameRegistry {
    pub fn new() -> Self {
        GameRegistry::default()
    }

    /// Adds a game; ids must be unique.
    pub fn register(&mut self, game: Box<dyn ArcadeTitle>) {
        assert!(
            self.find(game.id()).is_none(),
            "duplicate game id '{}'",
            game.id()
        );
        assert!(!game.modes().is_empty(), "'{}' has no modes", game.id());
        self.games.push(game);
    }

    pub fn len(&self) -> usize {
        self.games.len()
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&dyn ArcadeTitle> {
        self.games.get(idx).map(|g| g.as_ref())
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut (dyn ArcadeTitle + 'static)> {
        self.games.get_mut(idx).map(|g| g.as_mut())
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.games.iter().position(|g| g.id() == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn ArcadeTitle> {
        self.games.iter().map(|g| g.as_ref())
    }

    pub fn mode(&self, game: usize, mode: usize) -> Option<&'static GameModeInfo> {
        self.get(game)?.modes().get(mode)
    }

    /// The "Arcade score": sum of every score-ranked best of every game.
    /// Timed modes and per-date daily keys are excluded.
    pub fn arcade_total(&self, save: &SaveData) -> u64 {
        save.arcade_total(|key| {
            let mut it = key.splitn(2, ':');
            let (Some(game), Some(mode)) = (it.next(), it.next()) else {
                return false;
            };
            self.find(game)
                .and_then(|g| self.get(g))
                .and_then(|g| g.modes().iter().find(|m| m.id == mode))
                .is_some_and(|m| !m.ranked_by_time)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arcade::{ArcadeInput, FrameEvents, VirtualCanvas};
    use crate::presentation::theme::Theme;
    use macroquad::prelude::{Color, Rect};

    struct Dummy(&'static str);

    const MODES: [GameModeInfo; 2] = [
        GameModeInfo::new("score", "SCORE", "points"),
        GameModeInfo::new("race", "RACE", "time").timed(),
    ];

    impl ArcadeTitle for Dummy {
        fn id(&self) -> &'static str {
            self.0
        }
        fn title(&self) -> &'static str {
            "DUMMY"
        }
        fn tagline(&self) -> &'static str {
            ""
        }
        fn accent_color(&self) -> Color {
            Color::new(1.0, 1.0, 1.0, 1.0)
        }
        fn modes(&self) -> &'static [GameModeInfo] {
            &MODES
        }
        fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn start(&mut self, _: &str, _: u64) {}
        fn update(&mut self, _: f32, _: &ArcadeInput) -> FrameEvents {
            FrameEvents::default()
        }
        fn draw(&self, _: &VirtualCanvas, _: &Theme, _: bool) {}
        fn draw_preview(&self, _: Rect, _: f32, _: &Theme) {}
        fn simulate_smoke_step(&mut self, _: usize) {}
    }

    #[test]
    fn registers_and_finds_games() {
        let mut r = GameRegistry::new();
        assert!(r.is_empty());
        r.register(Box::new(Dummy("a")));
        r.register(Box::new(Dummy("b")));
        assert_eq!(r.len(), 2);
        assert_eq!(r.find("b"), Some(1));
        assert_eq!(r.find("zzz"), None);
        assert_eq!(r.mode(0, 1).map(|m| m.id), Some("race"));
        assert!(r.mode(0, 2).is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate game id")]
    fn duplicate_ids_are_rejected() {
        let mut r = GameRegistry::new();
        r.register(Box::new(Dummy("a")));
        r.register(Box::new(Dummy("a")));
    }

    #[test]
    fn arcade_total_counts_only_score_modes_of_known_games() {
        let mut r = GameRegistry::new();
        r.register(Box::new(Dummy("a")));
        let mut s = SaveData::default();
        s.highscores.insert("a:score".into(), 100);
        s.highscores.insert("a:race".into(), 55_000);
        s.highscores.insert("daily:a:2026-11-16".into(), 70);
        s.highscores.insert("unknown:score".into(), 9);
        assert_eq!(r.arcade_total(&s), 100);
    }
}
