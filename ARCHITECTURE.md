# Architektur: VibeCoded Arcade

Dieses Dokument beschreibt, wie die Plattform aufgebaut ist und **wie man ein drittes Spiel hinzufügt** (Kapitel 6). Die Regeln der einzelnen Spiele stehen in der [README](README.md).

## 1. Leitidee

Die Anwendung kennt keine konkreten Spiele. Sie kennt nur das Trait `ArcadeTitle` und eine `GameRegistry` (`Vec<Box<dyn ArcadeTitle>>`). Hauptmenü, Pause- und Ergebnis-Overlay, Audio, Musik, Partikel, Highscores und Smoke-Test arbeiten ausschließlich über dieses Trait. Ein neues Spiel braucht deshalb ein Modul und eine Zeile `register(...)`.

```
                         main.rs  (--smoke-test / Fenster)
                            │
                  presentation::app::App   (Arcade-Controller)
        ┌───────────┬───────┴────────┬──────────────┬──────────────┐
      Nav        MenuState       Audio+Musik   Partikel/Callouts   SaveData
 (Zustände)   (Karten, Modi)   (Cues, Stufen)   (Widgets)       (Highscores)
        └───────────┴───────┬────────┴──────────────┴──────────────┘
                            │  nur  dyn ArcadeTitle
                     arcade::GameRegistry
                            │
          ┌─────────────────┴─────────────────┐
   titles::blocks::BlocksTitle        titles::snake::SnakeTitle      (+ dein Spiel)
          │                                   │
   core::blocks + engine              core::snake
          └──────────── core::shared ─────────┘
              (SplitMix64, FlowEngine, Datum)
```

## 2. Schichten und Regeln

| Schicht | Pfad | Darf `macroquad`? | Inhalt |
|---|---|---|---|
| Core | `src/core/` | **nein** | `shared/` (RNG, Flow, Datum), `blocks/`, `snake/`, `armada/`, `glide/`, `hopper/`, `maze/`: reine, deterministische Regeln |
| Engine | `src/engine/` | **nein** | Blocks-Simulation: Fixed Timestep, DAS/ARR, Autopilot |
| Arcade | `src/arcade/` | ja (nur `Color`, `Rect`, `Vec2`) | `ArcadeTitle`, `FrameEvents`, `ArcadeInput`, `GameRegistry`, `Nav` |
| Presentation | `src/presentation/` | ja | Controller, Menü, Settings, Widgets, Stage, Shader, Synth, Musik |
| Titles | `src/titles/` | ja | Die Spiele als Adapter plus ihre Zeichencode |
| Infrastructure | `src/infrastructure/` | nein | Save, Uhr, CLI, Smoke-Test |

Diese Regeln prüft `tests/architecture.rs` automatisch:

- `src/core/` und `src/engine/` enthalten kein `macroquad` (rekursiv über alle Unterordner), `#![forbid(unsafe_code)]` steht in `lib.rs` und `main.rs`.
- `core::shared` kennt kein Spiel. `blocks`, `snake`, `armada`, `glide`, `hopper` und `maze` kennen sich nicht.
- `presentation/`, `arcade/` und `infrastructure/` nennen niemals `BlocksTitle`, `SnakeTitle`, `VibeCodedArmada`, `VibeCodedGlide`, `VibeCodedHopper`, `VibeCodedMaze` oder `titles::<spiel>`. Nur `src/titles/mod.rs` (`default_registry`) registriert konkrete Spiele.

## 3. Das Trait `ArcadeTitle`

Definiert in [src/arcade/mod.rs](src/arcade/mod.rs). Es ist objektsicher.

| Methode | Zweck |
|---|---|
| `id()` | Stabile ID (`"blocks"`, `"snake"`), Präfix der Highscore-Schlüssel |
| `title()`, `tagline()`, `accent_color()` | Anzeige auf der Menü-Karte und im Overlay |
| `modes()` → `&'static [GameModeInfo]` | `id`, `label`, `description`, `is_daily`, `ranked_by_time` |
| `controls_legend()` | `(Tasten, Aktion)`-Paare, das Pause-Overlay zeigt sie automatisch |
| `start(mode_id, seed)` | Setzt das Spiel komplett zurück und startet den Modus |
| `update(dt, &ArcadeInput) -> FrameEvents` | Ein Frame. Liefert Audio-Cues, Partikel, Callouts, Shake, Flash, Flow, Musikstufe und einmalig `SessionEnd` |
| `draw(&VirtualCanvas, &Theme, colorblind)` | Zeichnet das Spiel auf die 1280×720-Bühne |
| `draw_preview(rect, time, &Theme)` | Lebendige Mini-Animation für die Menü-Karte (reine Funktion von `time`) |
| `simulate_smoke_step(tick)` | Ein Bot-Schritt für den headless Smoke-Test, prüft Invarianten per `assert!` |

Optionale Methoden mit Standardimplementierung:

| Methode | Zweck |
|---|---|
| `set_context(&TitleContext)` | Vor jedem `update`: Einstellungen (DAS/ARR, Boost, Theme), persönlicher Bestwert, Tagesdatum, Beat-Puls |
| `on_resume(&ArcadeInput)` | Nach Pause und nach `start`: gehaltene Tasten dürfen nichts auslösen |
| `demo_update(dt) -> Option<FrameEvents>` | Attract-Mode: ein Bot spielt. `None` = nicht unterstützt |

**Wichtig für Headless-Läufe:** `start`, `update` und `simulate_smoke_step` dürfen keine Fenster-, GPU- oder Audiofunktionen von `macroquad` aufrufen (`get_time`, `screen_width`, `measure_text`, Texturen …). Nur `draw` und `draw_preview` dürfen zeichnen. Der Smoke-Test läuft vor dem Öffnen eines Fensters.

**Eingabe:** `ArcadeInput` enthält gehaltene und neu gedrückte Tasten (`Key`) sowie die Maustasten, die in diesem Frame gedrückt wurden (`mouse_left_pressed()`, `mouse_right_pressed()`). Glide nutzt sie für Flap und Dash.

### Was ein Spiel zurückgibt: `FrameEvents`

Ein Spiel spricht nie direkt mit Audio oder den gemeinsamen Effekt-Systemen:

- `audio`: `AudioCue::Sound { name, gain }` oder `AudioCue::Note { tier, idx, gain }`. Die generischen Namen stehen in `arcade::cue`: `ui_move`, `ui_confirm`, `action_tick`, `rotate_or_turn`, `clear_or_eat`, `overdrive_up`, `game_over`. Eigene Sounds der Bank (z. B. Blocks' `harddrop`) sind ebenfalls erlaubt.
- `particles`, `callouts`: in Bühnenkoordinaten, der Controller speist `ParticleSystem` und `FloatingCallouts`.
- `shake`, `screen_flash`: Kamerawackeln und Vollbild-Blitz.
- `flow` (0–100) und `music_stage` (0–4): steuern Shader-Intensität, Beat-Tempo und die Musik.
- `session_end: Option<SessionEnd>`: genau einmal beim Ende. Enthält Titel, Gewonnen-Flag, Headline (`("SCORE", "1234")`), den zu speichernden Wert (`value`, bei Zeit-Modi Millisekunden mit `lower_is_better`), `counts_for_record` und die Zeilen der Statistik-Tabelle.

Effekte, die **im Spielfeld** leben (Blocks: Hard-Drop-Schweif, Board-Bounce, Lock-Flash, Zen-Auflösung), bleiben im Spiel selbst und werden dort mitgezeichnet.

## 4. Registry, Navigation und Controller

- **`GameRegistry`** hält die Spiele und lehnt doppelte IDs ab. `arcade_total(&SaveData)` summiert alle Score-Highscores (Zeit-Modi und Pro-Tag-Schlüssel zählen nicht).
- **`Nav`** ist eine kleine, reine Zustandsmaschine: `Menu → Playing ⇄ Paused`, `Playing → Ending → Results → Menu`. `Ending` hält das letzte Bild 0,9 s sichtbar. Weil sie rein ist, treibt der Smoke-Test exakt dieselben Übergänge.
- **`App`** ([src/presentation/app.rs](src/presentation/app.rs)) verbindet alles: pro Frame `set_context` → `update` → `apply_events` (Audio, Partikel, Callouts, Shake) → Musikwahl. Beim Spielende speichert er den Highscore und zeigt das `ModalOverlay`.
- **Menü** ([src/presentation/menu.rs](src/presentation/menu.rs)): `layout(n, page)` ist eine reine Funktion (getestet für 1–9 Spiele), ebenso `MenuState::update`. 1–3 Spiele stehen nebeneinander, ab 4 Spielen gibt es ein 2×2-Raster mit Seiten-Pfeilen.

## 5. Persistenz

`SaveData` ([src/infrastructure/save.rs](src/infrastructure/save.rs)) speichert Highscores generisch in `highscores: HashMap<String, u64>`:

| Schlüssel | Bedeutung |
|---|---|
| `"<spiel>:<modus>"` | Bester Wert des Modus (`"blocks:marathon"`, `"snake:classic"`) |
| `"daily:<spiel>:<YYYY-MM-DD>"` | Bester Wert eines Tages in einem Daily-Modus |

Alle Strukturen tragen `#[serde(default)]`: Fehlende Felder bekommen Defaults, unbekannte werden ignoriert. Dateien der Version 1 werden beim Laden in die neuen Schlüssel übernommen (Test: `legacy_save_is_migrated_to_generic_keys`).

## 6. Neues Spiel hinzufügen

Beispiel: ein weiteres Spiel „TAP RACE“. Der komplette, lauffähige Code steht als Integrationstest in [tests/plugin.rs](tests/plugin.rs).

Hinweis: Blocks, Snake, Armada, Glide, Hopper und Maze sind die sechs mitgelieferten Spiele. Das Beispiel unten ist ein **siebtes** Spiel (siehe `tests/plugin.rs`).

**Schritt 1: reine Logik (optional, empfohlen).** Lege `src/core/tap/` an (kein `macroquad`!) und deklariere es in `src/core/mod.rs` mit `pub mod tap;`. Nutze `core::rng::SplitMix64` für Zufall, `core::flow::FlowEngine` für Flow und `core::date::Date::seed()` für Daily-Seeds. Teste die Regeln hier mit Unit-Tests.

**Schritt 2: den Adapter schreiben.** Lege `src/titles/tap/mod.rs` an und implementiere `ArcadeTitle`:

```rust
use macroquad::prelude::{Color, Rect};
use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, VirtualCanvas, cue,
};
use crate::presentation::theme::Theme;

const MODES: [GameModeInfo; 2] = [
    GameModeInfo::new("tap", "TAP RACE", "Tap SPACE as often as you can in 5 s."),
    GameModeInfo::new("sprint", "TAP SPRINT", "Reach 30 taps as fast as possible.").timed(),
];

pub struct TapTitle { taps: u32, time: f32, sprint: bool, reported: bool }

impl TapTitle {
    pub fn new() -> Self {
        TapTitle { taps: 0, time: 0.0, sprint: false, reported: false }
    }

    fn finished(&self) -> bool {
        if self.sprint { self.taps >= 30 } else { self.time >= 5.0 }
    }
}

impl ArcadeTitle for TapTitle {
    fn id(&self) -> &'static str { "tap" }
    fn title(&self) -> &'static str { "TAP RACE" }
    fn tagline(&self) -> &'static str { "how fast are your fingers?" }
    fn accent_color(&self) -> Color { Color::new(1.0, 0.6, 0.2, 1.0) }
    fn modes(&self) -> &'static [GameModeInfo] { &MODES }
    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] { &[("SPACE", "Tap")] }

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
                ev.sound(cue::ACTION_TICK, 0.4);          // Sound über FrameEvents
            }
        }
        ev.flow = (self.taps as f32 * 3.0).min(100.0);     // Shader, Beat und Musik
        if self.finished() && !self.reported {
            self.reported = true;
            ev.session_end = Some(SessionEnd {              // Ergebnis-Overlay + Highscore
                title: "TIME!", won: true,
                headline: ("TAPS", self.taps.to_string()),
                value: if self.sprint { (self.time * 1000.0) as u64 } else { self.taps as u64 },
                lower_is_better: self.sprint,
                counts_for_record: true,
                summary: vec![("TAPS".into(), self.taps.to_string())],
            });
        }
        ev
    }

    fn draw(&self, _c: &VirtualCanvas, _t: &Theme, _colorblind: bool) { /* zeichnen */ }
    fn draw_preview(&self, _rect: Rect, _time: f32, _t: &Theme) { /* Mini-Animation */ }
    fn simulate_smoke_step(&mut self, tick: usize) {
        // Bot-Input erzeugen, update() aufrufen, Invarianten asserten.
        if self.finished() {
            self.start(if self.sprint { "sprint" } else { "tap" }, 0);
        }
        let tap = tick.is_multiple_of(3);
        self.update(1.0 / 60.0, &if tap { ArcadeInput::tap(&[Key::Space]) } else { ArcadeInput::EMPTY });
    }
}
```

Der Code oben ist eine gekürzte Fassung von [tests/plugin.rs](tests/plugin.rs). Dort läuft er als Test gegen die echte Registry (Smoke-Test, Menü-Layout, Highscores). Maßgeblich ist immer die Testdatei.

**Schritt 3: registrieren.** In `src/titles/mod.rs`:

```rust
pub mod tap;

pub fn default_registry() -> GameRegistry {
    let mut r = GameRegistry::new();
    r.register(Box::new(blocks::BlocksTitle::new()));
    r.register(Box::new(snake::SnakeTitle::new()));
    r.register(Box::new(armada::VibeCodedArmada::new()));
    r.register(Box::new(glide::VibeCodedGlide::new()));
    r.register(Box::new(hopper::VibeCodedHopper::new()));
    r.register(Box::new(maze::VibeCodedMaze::new()));
    r.register(Box::new(tap::TapTitle::new()));   // <- die eine neue Zeile
    r
}
```

Fertig. Ohne weitere Änderung gilt automatisch:

- Das Hauptmenü zeigt die neue Karte (das siebte Spiel steht auf Seite 2 des Menüs, zusammen mit dem fünften und sechsten) mit Titel, Tagline, `draw_preview`, Modi und Highscore.
- `1`–`9`, `Enter` und Klick starten das Spiel. Pause (`P`/`Esc`) zeigt `controls_legend`, das Ergebnis-Overlay die Statistik-Tabelle samt Rekord-Badge.
- Highscores landen unter `tap:tap`, `tap:sprint` bzw. `daily:tap:<Datum>` in `save.json`, ohne neue Felder.
- `--smoke-test` spielt jeden Modus des Spiels 300 Ticks, fährt zurück ins Hauptmenü und endet mit Exit-Code `0`.
- Die generischen Audio-Cues (`cue::*`), Partikel, Callouts, Shake und die Musikstufe (`FrameEvents::music_stage`) funktionieren.

**Checkliste:**

1. Modus-IDs eindeutig und stabil (sie sind Teil der Highscore-Schlüssel).
2. Zeit-Modi mit `.timed()` markieren und `value` in Millisekunden liefern, `lower_is_better: true` setzen.
3. Daily-Modi mit `.daily()` markieren. Der Controller übergibt dann `seed = YYYYMMDD`, deterministisch ableiten.
4. `session_end` genau einmal senden. `draw_preview` muss eine reine Funktion von `time` sein.
5. `start`/`update`/`simulate_smoke_step` ohne `macroquad`-Kontext schreiben (headless).
6. `cargo test` ausführen: `tests/architecture.rs` meldet Verstöße gegen die Schichtregeln.

## 7. Widget-Toolkit

`src/presentation/widgets/` arbeitet im virtuellen 1280×720-Koordinatensystem (Letterboxing und High-DPI-Maus-Umrechnung übernimmt `Stage`):

| Widget | Zweck |
|---|---|
| `RoundedPanel` | Abgerundete, transparente Karte mit Rahmen und Akzent-Streifen (`.opaque()` für Dialoge) |
| `ActionButton` | Maus-Hover und -Klick, Tastatur-Fokus-Highlight |
| `ModePillSelector` | Vertikale/horizontale Moduswahl, `pill_rects` ist reine Layout-Mathematik |
| `StatBox` | Label plus geometrische Vektor-Ziffern (`0-9 : . x % # -`) |
| `FlowMeterBar` | Leuchtende Flow-Anzeige (horizontal/vertikal) mit Tier-Badge |
| `ModalOverlay` | Pause (mit `controls_legend`) und Ergebnis (Statistik-Tabelle, Rekord-Badge, `Restart [R]`, `Main Menu [Q]`) |
| `ParticleSystem`, `FloatingCallouts` | Zentral vom Controller über `FrameEvents` bespielt |

## 8. Build und Pipeline

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release                  # target\release\VibeCodedGame.exe
cargo run -- --smoke-test              # headless, alle Spiele, Exit-Code 0
```

`.github/workflows/build-release.yml` (`windows-latest`) führt dieselben Schritte aus, startet die fertige Exe mit `--smoke-test` (`Start-Process -Wait`, da die Release-Exe ein GUI-Programm ohne Konsole ist) und lädt `VibeCodedGame.exe` als Artifact hoch. Tags `v*` erzeugen ein GitHub-Release.
