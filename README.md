# VibeCoded Blocks

Ein reaktionsschnelles, Guideline-treues Block-Stacking-Puzzle in **Rust** mit viel „Game Feel“: deterministische Kollisionsphysik, SRS-Rotation mit Wall-Kicks, ein Flow-System, das sauberes Spiel mit Punkte-Multiplikatoren belohnt, ein GLSL-Hintergrund im Takt der Musik und ein komplett prozedural erzeugter Soundtrack in C-Moll-Pentatonik. Ausgeliefert als eine einzige `VibeCodedGame.exe`, ohne externe Assets.

---

## Schnellstart

```powershell
cargo run --release                 # spielen
cargo run --release -- --demo       # Attract-Mode: der Bot spielt
cargo run -- --smoke-test           # headless Selbsttest (kein Fenster), Exit-Code 0
```

Voraussetzung: Rust ≥ 1.88 (Edition 2024), Windows 10/11. Läuft auch unter Linux/macOS, Ziel-Plattform ist aber Windows.

## Steuerung

| Aktion | Tasten | Auslösung |
|---|---|---|
| Links / Rechts | `←` `→` | DAS 130 ms, ARR 30 ms (einstellbar), SOCD: letzte Eingabe gewinnt |
| Soft Drop | `↓` | gehalten, 20× Gravity (mind. 20 Zellen/s) |
| Hard Drop | `Space` | einmal pro Tastendruck |
| Drehen rechts | `↑` `X` | einmal pro Tastendruck |
| Drehen links | `Y` `Z` `Strg` | einmal pro Tastendruck (`Y` **und** `Z` → QWERTZ & QWERTY) |
| Drehen 180° | `A` | einmal pro Tastendruck |
| Hold | `C` `Shift` | einmal pro Stein |
| Pause | `P` `Esc` | |
| Theme / Farbenblind / Mute | `T` / `B` / `M` | überall |

Im Menü: `↑`/`↓` oder `1`–`4` wählen, `Enter`/Klick startet. Das Settings-Panel (DAS, ARR, Lautstärke, Sound, Theme, Farbenblind-Modus) ist per Maus bedienbar und wird sofort gespeichert. Nach 25 s ohne Eingabe startet eine Demo.

## Spielmodi

| Modus | Ziel |
|---|---|
| **Marathon** | Endlos. Level-Up alle 10 Lines, Guideline-Gravity-Kurve bis Level 20. |
| **40-Line Sprint** | 40 Lines auf Zeit, Bestzeit wird gespeichert. |
| **Daily Vibe** | 200 Lines. Seed = heutiges UTC-Datum `YYYYMMDD` → alle spielen dieselbe Steinfolge. Highscore pro Datum. |
| **Zen** | Kein Game Over: bei Top-Out löst sich das Feld in einer sanften Welle auf, das Spiel läuft weiter. |

## Regeln (Kurzfassung)

- **Matrix** 10 × 40, Zeilen 0–19 sind die unsichtbare Pufferzone, `(0,0)` oben links, `+y` nach unten. Spawn zentriert in Zeile 18–19, danach sofort eine Zeile tiefer, wenn frei.
- **Kollision:** eine zentrale, reine Funktion `is_valid_position` (Wände, Boden, Puffer-Decke, belegte Zellen). Der Ghost ist dieselbe Funktion in einer Schleife.
- **SRS:** offizielle 5er-Kick-Tabellen (J/L/S/T/Z bzw. I). Die Tabellen stehen im Code exakt wie in der Guideline (`+y` = oben) und werden als `y - dy` angewendet, damit Floor-Kicks den Stein **anheben**. Ein Unit-Test sichert das ab.
- **Lock Delay:** 500 ms, der Timer läuft nur bei Bodenkontakt (sonst pausiert er). Erfolgreiche Moves/Rotationen setzen ihn zurück, höchstens 15-mal. Gegen eine Wand drücken zählt nicht. Eine neue tiefste Zeile setzt den Zähler zurück.
- **T-Spin:** 3-Corner-Rule; die letzte erfolgreiche Aktion muss eine Rotation gewesen sein.
- **Scoring:** Single/Double/Triple/Quad 100/300/500/800, T-Spin 0–3 Lines 400/800/1200/1600, alles × Level × Flow-Multiplikator; Back-to-Back ×1,5; Combo +50 × Combo × Level; Soft Drop +1, Hard Drop +2 pro Zelle.
- **Top-Out:** Block Out (Spawn kollidiert) und Lock Out (Stein rastet komplett in der Pufferzone ein).

### Flow-Engine

| Stufe | Flow | Score-Multiplikator | Shader-Takt |
|---|---|---|---|
| Chill | 0–25 | 1.0× | 110 BPM |
| Groove | 25–60 | 1.25× | 116 BPM |
| Sync | 60–90 | 1.5× | 124 BPM |
| Overdrive | 90–100 | 2.0× | 132 BPM |

Aufbau: Lock < 1,2 s nach dem Spawn +5, Single/Double/Triple/Quad +10/16/22/30, T-Spin +25, Combo +6 × Combo. Bei 100 greift ein 4-s-Overdrive-Shield. Nach 1,5 s ohne Lock (und ohne Shield) fällt der Flow um 3,5/s. **Flow beeinflusst nie die Gravity.** Das prüft ein eigener Test.

## Architektur

Strikte 4-Schichten-Trennung. `#![forbid(unsafe_code)]` gilt für die ganze Crate, und `tests/architecture.rs` prüft automatisch, dass `core` und `engine` kein `macroquad` verwenden.

```
src/
├── core/            1) Reine Domänenlogik, kein macroquad
│   ├── board.rs        Matrix 10×40, is_valid_position, Line Clears (Naive Gravity)
│   ├── piece.rs        Tetrominos, Rotationszustände, Bounding Boxes
│   ├── srs.rs          SRS-Kick-Tabellen + Rotationsauflösung (y-Inversion)
│   ├── rng.rs          SplitMix64 + Fisher-Yates-7-Bag (ohne rand-Crate)
│   ├── flow.rs         Flow-Meter, Tiers, Overdrive-Shield
│   ├── scoring.rs      Guideline-Scoring, B2B, Combo
│   ├── date.rs         UTC-Kalender → Daily-Seed YYYYMMDD
│   └── game.rs         GameState: Gravity, Lock Delay, Hold, T-Spin, Top-Out, Modi, Events
├── engine/          2) Simulation & Input, kein macroquad
│   ├── input.rs        abstrakte Buttons, Rising-Edge-Erkennung
│   ├── das.rs          DAS/ARR mit SOCD-Last-Input-Priority
│   ├── sim.rs          Fixed Timestep 240 Hz, Frame-dt auf 50 ms gekappt
│   └── autopilot.rs    Bot (Smoke-Test & Demo) – spielt über die echte Input-Pipeline
├── presentation/    3) Rendering & Audio (macroquad)
│   ├── stage.rs        virtuelle 1280×720-Bühne, Letterboxing, DPI-korrekte Maus
│   ├── text.rs         Standard-Font nur in 1×/2×/3× mit 1:1-Texel-Mapping + Schatten
│   ├── digits.rs       geometrischer Vektor-Ziffern-Renderer (0-9 : . x % # -)
│   ├── blocks.rs       4-Layer-Minos, Ghost, Farbenblind-Symbole
│   ├── background.rs   GLSL-Shader: Gradient, Grid, Vignette, BPM-Puls (+CPU-Fallback)
│   ├── fx.rs           Partikel, Hard-Drop-Schweif, Feder-Bounce, Flashes, Callouts
│   ├── synth.rs        WAV-Synthese im RAM (RIFF-Header, Pentatonik, Tier-Harmonien)
│   ├── audio.rs        Wiedergabe mit Graceful Fallback ohne Audiogerät
│   ├── hud.rs, app.rs  3-Spalten-HUD, Menü, Settings, Pause, Ergebnis
│   └── icon.rs         prozedurales Fenster-Icon
├── infrastructure/  4) Persistenz, Uhr, CLI, Smoke-Test
│   ├── save.rs         %APPDATA%\VibeCodedGame\save.json (atomar, fehlertolerant)
│   ├── clock.rs        UTC-Datum via SystemTime
│   └── smoke.rs        headless 500-Tick-Test mit Invarianten
└── main.rs          Einstiegspunkt: --smoke-test / --demo, Window::from_config
```

**Datenfluss:** `presentation::input_map` liest die gehaltenen Tasten in ein `ButtonSet`. `engine::Simulation` erkennt Flanken, wendet DAS/ARR an und treibt `core::GameState` mit festen 240-Hz-Schritten. Der Core erzeugt `GameEvent`s, aus denen die Präsentation Effekte und Sound ableitet. Weil Input abstrakt ist, laufen Smoke-Test und Demo durch exakt dieselbe Pipeline wie ein Spieler.

### Robustheit unter Windows

- **Keine Konsole:** `windows_subsystem = "windows"` in Release-Builds.
- **Kein Audiogerät:** Das Windows-Audio-Backend öffnet das Gerät in einem eigenen Thread und bricht dort per `assert!` ab, wenn keins da ist. Ein Panic-Hook fängt Panics in Nicht-Haupt-Threads ab und schaltet auf „NO DEVICE“ (lautlos). Das Laden der Sounds läuft zusätzlich in `catch_unwind`. Kein `.unwrap()` im Audiopfad.
- **Defekte/fehlende Save-Datei:** Es wird mit Defaults weitergespielt. Werte werden auf gültige Bereiche geklemmt, geschrieben wird atomar (Temp-Datei + Rename).
- **Shader-Fehler:** Kompiliert der Shader nicht, zeichnet ein CPU-Fallback Gradient und Grid.
- **High-DPI:** Viewport in physischen Pixeln, Maus über dieselbe Letterbox-Mathematik (unit-getestet für 100/125/150 %).

## Build, Test, Qualität

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                   # Unit-, Architektur- und Smoke-Tests
cargo build --release                        # target\release\VibeCodedGame.exe
```

Die Release-Exe ist ein GUI-Programm (ohne Konsole). Deshalb wartet man in PowerShell explizit auf das Ende und prüft den Exit-Code:

```powershell
$p = Start-Process .\target\release\VibeCodedGame.exe -ArgumentList '--smoke-test' -Wait -PassThru; $p.ExitCode   # 0
```

Release-Profil: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`.

### CI: GitHub Actions

`.github/workflows/build-release.yml` läuft auf `windows-latest` und führt `fmt --check`, `clippy -D warnings`, `cargo test`, `cargo build --release` und den Smoke-Test der fertigen Exe aus. Danach lädt er `VibeCodedGame.exe` als Artifact hoch. Bei einem Tag `v*` wird zusätzlich ein GitHub-Release erstellt:

```powershell
git tag v1.0.0; git push origin v1.0.0
```

Falls der Workflow nicht mit gepusht werden konnte (fehlender `workflow`-Scope des Tokens), lässt er sich mit diesem Einzeiler nachträglich aktivieren:

```powershell
gh auth refresh -s workflow; git add .github/workflows/build-release.yml; git commit -m "ci: windows build"; git push
```

### Testabdeckung (Auswahl)

- Wand- und Floor-Kicks inkl. y-Inversion, I-Kicks, komplett blockierte Rotation
- Lock Delay 500 ms, Limit von 15 Resets, kein Reset beim Drücken gegen eine Wand, Reset bei neuer tiefster Zeile, Timer pausiert in der Luft
- T-Spin Double (3-Corner-Rule) vs. gleiche Lage ohne Rotation; Hold-Regeln; Block Out / Lock Out; Zen ohne Top-Out
- SplitMix64-Referenzwerte, 7-Bag-Determinismus und -Vollständigkeit, Daily-Datum/Seed
- Flow-Tiers, Multiplikatoren, Gewinne, Decay, 4-s-Shield, Gravity unabhängig vom Flow
- Rising Edge (gehaltenes Space = genau ein Hard Drop), DAS/ARR-Timing, SOCD, ARR = 0
- WAV-RIFF-Header Byte für Byte, Gültigkeit der gesamten Sound-Bank, Pentatonik-Frequenzen
- Letterbox- und Maus-Mapping, Federamplitude 4–6 px, Save-Roundtrip/Korruption, CLI
- Smoke-Test (500 Ticks, Bot): Moves, Rotationen, Hold, Hard Drops, Line Clears, Flow-Anstieg, Zellen-Erhaltung, keine Überlappung

## Review- & Optimierungsrunden

1. **Runde 1 – Code-Qualität:** Clippy auf `-D warnings` gebracht (let-chains statt verschachtelter `if`), tote Variablen und ungenutzte HUD-Felder entfernt, Autopilot-Tempo parametrisiert, Architektur-Regeln als Integrationstest festgeschrieben.
2. **Runde 2 – Feel & Visuals:** Bounce-Federimpuls so kalibriert, dass jeder Hard Drop 4–6 px ergibt (vorher unter 4 px bei kurzen Drops, jetzt per Test abgesichert). Zen-Reset als sanfte Auflösungswelle von unten nach oben statt hartem Clear. Zentrierte „1“ im Vektor-Font. Daily-Karte entzerrt, weil der Text überlief. Prozedurales Fenster-Icon. Attract-Mode mit ruhigerem Bot.
3. **Runde 3 – Balancing & Release:** Dev-Profil optimiert (flüssiges Debug-Spielen), Release-Profil mit LTO, CI-Smoke-Test über `Start-Process -Wait`, weil eine GUI-Exe in PowerShell sonst nicht abgewartet wird.

## Speicherort

`%APPDATA%\VibeCodedGame\save.json` enthält Highscores pro Modus, die Sprint-Bestzeit, Daily-Highscores pro Datum und die Einstellungen (DAS/ARR, Lautstärke, Mute, Theme, Farbenblind-Modus). Löschen setzt alles zurück.

## Lizenz

MIT
