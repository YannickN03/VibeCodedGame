//! Proof that the arcade is open for extension: a third game is added using
//! only the public `ArcadeTitle` API and `GameRegistry::register` - no change
//! to the menu, the controller, the save system or the smoke test.
//!
//! This is the same example that `ARCHITECTURE.md` walks through
//! ("Neues Spiel hinzufügen").

use macroquad::prelude::{Color, Rect, vec2};

use vibecoded_game::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use vibecoded_game::infrastructure::save::SaveData;
use vibecoded_game::infrastructure::smoke;
use vibecoded_game::presentation::menu;
use vibecoded_game::presentation::text::Size;
use vibecoded_game::presentation::theme::Theme;
use vibecoded_game::titles::default_registry;

const MODES: [GameModeInfo; 2] = [
    GameModeInfo::new("tap", "TAP RACE", "Tap SPACE as often as you can in 5 s."),
    GameModeInfo::new("sprint", "TAP SPRINT", "Reach 30 taps as fast as possible.").timed(),
];

/// A tiny "tap as fast as you can" game.
struct TapTitle {
    taps: u32,
    time: f32,
    sprint: bool,
    reported: bool,
}

impl TapTitle {
    fn new() -> Self {
        TapTitle {
            taps: 0,
            time: 0.0,
            sprint: false,
            reported: false,
        }
    }

    fn finished(&self) -> bool {
        if self.sprint {
            self.taps >= 30
        } else {
            self.time >= 5.0
        }
    }
}

impl ArcadeTitle for TapTitle {
    fn id(&self) -> &'static str {
        "tap"
    }
    fn title(&self) -> &'static str {
        "TAP RACE"
    }
    fn tagline(&self) -> &'static str {
        "how fast are your fingers?"
    }
    fn accent_color(&self) -> Color {
        Color::new(1.0, 0.6, 0.2, 1.0)
    }
    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }
    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &[("SPACE", "Tap")]
    }
    fn start(&mut self, mode_id: &str, _seed: u64) {
        *self = TapTitle::new();
        self.sprint = mode_id == "sprint";
    }
    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        let mut ev = FrameEvents::default();
        if !self.finished() {
            self.time += dt;
            if input.pressed(Key::Space) {
                self.taps += 1;
                ev.sound(cue::ACTION_TICK, 0.4);
                ev.callout(
                    "+1",
                    vec2(640.0, 360.0),
                    Color::new(1.0, 1.0, 1.0, 1.0),
                    Size::X1,
                );
            }
        }
        ev.flow = (self.taps as f32 * 3.0).min(100.0);
        if self.finished() && !self.reported {
            self.reported = true;
            ev.session_end = Some(SessionEnd {
                title: "TIME!",
                won: true,
                headline: ("TAPS", self.taps.to_string()),
                value: if self.sprint {
                    (self.time * 1000.0) as u64
                } else {
                    self.taps as u64
                },
                lower_is_better: self.sprint,
                counts_for_record: true,
                summary: vec![("TAPS".into(), self.taps.to_string())],
            });
        }
        ev
    }
    fn draw(&self, _canvas: &VirtualCanvas, _theme: &Theme, _colorblind: bool) {}
    fn draw_preview(&self, _rect: Rect, _time: f32, _theme: &Theme) {}
    fn simulate_smoke_step(&mut self, tick: usize) {
        if self.finished() {
            self.start(if self.sprint { "sprint" } else { "tap" }, 0);
        }
        let tap = tick.is_multiple_of(3);
        let input = if tap {
            ArcadeInput::tap(&[Key::Space])
        } else {
            ArcadeInput::EMPTY
        };
        self.update(1.0 / 60.0, &input);
    }
    fn set_context(&mut self, _ctx: &TitleContext) {}
}

#[test]
fn a_third_game_plugs_in_with_register_only() {
    let mut registry = default_registry();
    let before = registry.len();
    registry.register(Box::new(TapTitle::new()));
    assert_eq!(registry.len(), before + 1);

    // The automated smoke test covers the new game without any changes.
    let report = smoke::run_registry(&mut registry, 300);
    assert!(report.games.iter().any(|g| g.id == "tap" && g.modes == 2));
    assert_eq!(report.games.len(), before + 1);
    assert!(report.menu_returns >= 2 * (before + 1));
}

#[test]
fn the_menu_lays_out_any_number_of_games() {
    let mut registry = default_registry();
    registry.register(Box::new(TapTitle::new()));
    let n = registry.len();
    assert_eq!(
        n, 6,
        "blocks + snake + armada + glide + hopper + the new game"
    );
    let lay = menu::layout(n, 0);
    assert_eq!(lay.cards.len(), 4, "the first page is a full 2x2 grid");
    assert_eq!(lay.pages, 2, "the fifth game opens a second page");
    assert!(lay.compact);
    // The new game's modes are reachable through the menu state.
    let counts: Vec<usize> = registry.iter().map(|g| g.modes().len()).collect();
    assert_eq!(counts, vec![4, 4, 4, 4, 4, 2]);
    let mut state = menu::MenuState::new(&counts);
    let out = state.update(
        &menu::MenuInput {
            digit: Some(5),
            ..Default::default()
        },
        &counts,
    );
    assert_eq!(
        out.action,
        Some(menu::MenuAction::Play(vibecoded_game::arcade::Selection {
            game: 5,
            mode: 0
        }))
    );
}

#[test]
fn highscores_of_new_games_need_no_save_changes() {
    let registry = {
        let mut r = default_registry();
        r.register(Box::new(TapTitle::new()));
        r
    };
    let mut save = SaveData::default();
    assert!(save.record_session("tap", "tap", false, "", 42, false));
    assert!(save.record_session("tap", "sprint", false, "", 9_500, true));
    assert_eq!(save.highscore("tap:tap"), Some(42));
    // Only the score-ranked mode counts towards the Arcade score.
    assert_eq!(registry.arcade_total(&save), 42);
    let json = serde_json::to_string(&save).unwrap();
    let back: SaveData = serde_json::from_str(&json).unwrap();
    assert_eq!(back, save);
}

#[test]
fn the_tap_game_itself_works() {
    let mut t = TapTitle::new();
    t.start("sprint", 1);
    let mut end = None;
    for _ in 0..40 {
        let ev = t.update(0.05, &ArcadeInput::tap(&[Key::Space]));
        end = end.or(ev.session_end);
    }
    let end = end.expect("30 taps finish the sprint");
    assert!(end.lower_is_better && end.value > 0);
}
