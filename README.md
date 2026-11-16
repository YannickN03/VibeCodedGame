# VibeCoded Arcade

Eine modulare **Arcade-Plattform in Rust** (`macroquad`) mit vier Spielen, die sich Hauptmenü, Sound, Musik, Effekte, Flow-Engine und Speichersystem teilen:

- **VibeCoded Blocks**: reaktionsschnelles, Guideline-treues Block-Stacking-Puzzle mit SRS-Wall-Kicks, Flow-System und viel „Game Feel“.
- **VibeCoded Snake**: Snake mit 2-Slot-Turn-Buffer, Portal-Wrap, Daily-Hindernissen, Pulse-Äpfeln und Flow-Multiplikatoren.
- **VibeCoded Armada**: Arcade-Shooter mit 5 Gegnertypen, UFO, Dreadnought-Boss, zerstörbaren Bunkern, Phase-Dash, Graze-Bonus und 5 Power-Ups.
- **VibeCoded Glide**: präziser Endless-Flyer mit Vibe-Ringen, Close-Skim-Bonus, Phase-Dash und Windböen.

Alles ist deterministisch (eigener `SplitMix64`, keine `rand`-Crate), der Hintergrund ein GLSL-Shader im Takt der Musik, der Soundtrack komplett prozedural. Ausgeliefert als eine einzige `VibeCodedGame.exe` ohne externe Assets.

**Weitere Spiele lassen sich ohne Umbau an Menü, Save-System oder Game-Loop andocken.** Die Anleitung steht in [ARCHITECTURE.md](ARCHITECTURE.md), Kapitel „Neues Spiel hinzufügen“.

---

## Schnellstart

```powershell
cargo run --release                          # Arcade-Hauptmenü
cargo run --release -- --demo                # Attract-Mode: ein Bot spielt
cargo run --release -- --game snake          # Menü überspringen, direkt Snake starten
cargo run -- --smoke-test                    # headless Selbsttest aller Spiele (kein Fenster), Exit-Code 0
```

| Schalter | Wirkung |
|---|---|
| `--smoke-test` | Spielt jeden Modus **jedes registrierten Spiels** 300 Ticks mit dem Bot, prüft die Invarianten, fährt Pause, Ende, Ergebnis und Rückkehr ins Hauptmenü ab und validiert Audio und Save. Exit-Code `0` = alles in Ordnung. |
| `--demo` | Startet direkt den Attract-Mode (ein Bot spielt). |
| `--game <id>` `--mode <id>` | Startet direkt ein Spiel (`blocks`, `snake`, `armada`, `glide`) in einem Modus. |
| `--screenshot <datei.png>` | Speichert nach ein paar Sekunden den Inhalt des **Spielfensters** und beendet sich (mit `--wait <s>`, `--show pause`, `settings`, `colorblind` oder `theme2`, und `--preset boss`, `showcase`, `breach` für Armada). Gedacht für Doku und visuelle Prüfung. |

Voraussetzung: Rust 1.97.0 (Edition 2024; über `rust-toolchain.toml` festgelegt, rustup installiert es automatisch), Windows 10/11. Läuft auch unter Linux/macOS, Ziel-Plattform ist aber Windows.

## Das Arcade-Hauptmenü

Das Menü ist **datengetrieben**: Es zeigt automatisch eine Karte pro Spiel der `GameRegistry`. Bei 1 bis 3 Spielen stehen die Karten nebeneinander. Mit den **vier** mitgelieferten Spielen ergibt sich ein symmetrisches **2×2-Raster**, ab dem fünften Spiel kommen Seiten mit Pfeilen dazu.

Jede Karte zeigt Titel, Tagline, eine **lebendige Mini-Animation** des Spiels (`draw_preview`), die Moduswahl, deinen Highscore des gewählten Modus und den Start-Button. Die Leiste oben zeigt das Tages-Datum (`DAILY #YYYY-MM-DD`) und den **Arcade-Score**: die Summe aller gespeicherten Score-Highscores. Zeit-Modi wie der Sprint zählen nicht mit.

| Eingabe | Wirkung |
|---|---|
| `←` `→` / `A` `D` | Spiel wählen (läuft zyklisch durch alle Karten) |
| `↑` `↓` / `W` `S` | Modus des gewählten Spiels wählen |
| `Enter` / `Space` / Klick auf **PLAY** | Starten |
| `1`–`9` | Spiel Nr. n direkt starten |
| `Tab` / Klick auf **SETTINGS** | Einstellungen: DAS/ARR (Blocks), Sprint-Boost (Snake), Lautstärke, Sound, Theme, Farbenblind |
| `T` / `B` / `M` | Theme / Farbenblind-Modus / Mute (überall) |

Nach 25 s ohne Eingabe startet der Attract-Mode. In jedem Spiel pausiert `P`/`Esc` (das Pause-Overlay zeigt die Tastenbelegung des Spiels), `R` startet neu und `Q` führt zurück ins Hauptmenü.

---

## VibeCoded Blocks

### Steuerung

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

### Spielmodi

| Modus | Ziel |
|---|---|
| **Marathon** | Endlos. Level-Up alle 10 Lines, Guideline-Gravity-Kurve bis Level 20. |
| **40-Line Sprint** | 40 Lines auf Zeit, Bestzeit wird gespeichert. |
| **Daily Vibe** | 200 Lines. Seed = heutiges UTC-Datum `YYYYMMDD` → alle spielen dieselbe Steinfolge. Highscore pro Datum. |
| **Zen** | Kein Game Over: bei Top-Out löst sich das Feld in einer sanften Welle auf, das Spiel läuft weiter. |

### Regeln (Kurzfassung)

- **Matrix** 10 × 40, Zeilen 0–19 sind die unsichtbare Pufferzone, `(0,0)` oben links, `+y` nach unten. Spawn zentriert in Zeile 18–19, danach sofort eine Zeile tiefer, wenn frei.
- **Kollision:** eine zentrale, reine Funktion `is_valid_position` (Wände, Boden, Puffer-Decke, belegte Zellen). Der Ghost ist dieselbe Funktion in einer Schleife.
- **SRS:** offizielle 5er-Kick-Tabellen (J/L/S/T/Z bzw. I). Die Tabellen stehen im Code exakt wie in der Guideline (`+y` = oben) und werden als `y - dy` angewendet, damit Floor-Kicks den Stein **anheben**. Ein Unit-Test sichert das ab.
- **Lock Delay:** 500 ms, der Timer läuft nur bei Bodenkontakt (sonst pausiert er). Erfolgreiche Moves/Rotationen setzen ihn zurück, höchstens 15-mal. Gegen eine Wand drücken zählt nicht. Eine neue tiefste Zeile setzt den Zähler zurück.
- **T-Spin:** 3-Corner-Rule; die letzte erfolgreiche Aktion muss eine Rotation gewesen sein.
- **Scoring:** Single/Double/Triple/Quad 100/300/500/800, T-Spin 0–3 Lines 400/800/1200/1600, alles × Level × Flow-Multiplikator; Back-to-Back ×1,5; Combo +50 × Combo × Level; Soft Drop +1, Hard Drop +2 pro Zelle.
- **Top-Out:** Block Out (Spawn kollidiert) und Lock Out (Stein rastet komplett in der Pufferzone ein).

---

## VibeCoded Snake

`32×18`-Raster, Start mit Länge 4 bei `(16, 9)` Richtung rechts. Die Schlange läuft in festen Schritten (7,5 bis 18 Schritte/s, logarithmisch nach gegessenen Äpfeln), wird aber **zwischen den Schritten weich interpoliert**. Sie hat verjüngte Segmente, ein Rückenmuster im Farbenblind-Modus (`B`) und **Augen, die dem nächsten Apfel folgen**.

### Steuerung

| Aktion | Tasten |
|---|---|
| Lenken | `←` `↑` `↓` `→` oder `W` `A` `S` `D` |
| Sprint-Boost | `Space` / `Shift` gehalten (in den Settings abschaltbar), 1,6× Tempo |
| Pause | `P` / `Esc` |

**Turn-Buffer:** Bis zu 2 Richtungswechsel werden vorgemerkt und einer pro Schritt ausgeführt. Jede Eingabe wird gegen die **zuletzt vorgemerkte** Richtung geprüft, nicht gegen die aktuelle. So lässt sich ein schneller Doppel-Input (`↑` dann `←` bei Fahrt nach rechts) sauber als U-Turn spielen, und ein 180°-Wechsel in den eigenen Hals ist unmöglich.

### Modi

| Modus | Regeln |
|---|---|
| **Classic** | Wände töten. Das Tempo steigt mit jedem Apfel von 7,5 auf 18 Schritte/s. |
| **Portal Wrap** | Wände sind Portale (Modulo-Umbruch, ohne Streifen über das Feld). Nur der eigene Körper tötet. |
| **Daily Vibe** | Punktsymmetrische Hindernisse aus dem `YYYYMMDD`-Seed: für alle am selben Tag identisch, ohne Sackgassen und immer zusammenhängend (per Flood-Fill geprüft, sonst nächster abgeleiteter Seed). Highscore pro Datum. |
| **Zen** | Unsterblich: Wände wrappen, ein Biss in den eigenen Körper schneidet nur den Schwanz ab. |

### Regeln

- **Kollisionsreihenfolge pro Schritt:** 1. Wand (Wrap per Modulo oder `WallCollision`), 2. Hindernis (`ObstacleCollision`), 3. Futter, 4. Selbstkollision, 5. Segment-Update (`push_front`, `pop_back` falls nicht gewachsen).
- **Tail-Vacating:** Wächst die Schlange in diesem Schritt nicht, verlässt die Schwanzspitze ihr Feld gleichzeitig, der Kopf darf also hinein (geprüft wird nur gegen `segments[0..len-1]`). Wächst sie, auch durch noch ausstehendes Wachstum eines Pulse-Apfels, zählen alle Segmente.
- **Äpfel:** Freie Zellen werden gesammelt und per `SplitMix64` gewählt. Gibt es keine mehr, gewinnt man (`Victory`). Standard-Apfel `+100 × Flow`, `+1` Länge. Nach jedem 5. Apfel erscheint zusätzlich ein **Pulse-Apfel** für 7,0 s (Countdown-Ring): `+250 × Flow`, `+2` Länge, `+25` Flow.
- **Flow:** dieselbe Flow-Engine wie in Blocks (siehe unten). Äpfel füllen sie, Leerlauf leert sie. Sie ändert nur den Score-Multiplikator und das Tempo des Hintergrund-Pulses, nie die Spielgeschwindigkeit.

---

## VibeCoded Armada

Ein moderner Space-Invaders-Shooter in einer `900×640`-Arena (zentriert in der 1280×720-Bühne, links und rechts je ein HUD-Panel). Das Schiff steht bei `y = 595`, die Defense Line bei `y = 560`.

### Steuerung

| Aktion | Tasten |
|---|---|
| Bewegen | `←` `→` oder `A` `D` (420 Einheiten/s) |
| Feuern | `Space` / `W` / `↑` / `J`, gedrückt halten = Dauerfeuer (Cooldown 0,22 s, max. 3 Salven gleichzeitig) |
| Phase-Dash | `Shift` / `C` / `K`: 0,16 s Boost mit **0,22 s Unverwundbarkeit**, Cooldown 1,8 s (Bullet Swarm: 1,1 s) |
| Pause | `P` / `Esc` |

Fire- und Dash-Tastendrücke werden kurz gepuffert, ein Tipp geht also auch zwischen zwei Simulationsschritten nicht verloren.

### Gegner und Wellen

| Typ | HP | Punkte | Verhalten |
|---|---|---|---|
| Scout Drone | 1 | 100 | Frontkämpfer, gerade Plasma-Bolzen |
| Interceptor | 1 | 150 | Schüsse (teils als Zwillinge) auf die X-Position des Spielers |
| Aegis Striker | 2 | 250 | Schild: erster Treffer bricht ihn (Riss und Metall-Ping) |
| Phase Weaver | 2 | 300 | Pendelt zwischen Slots und lässt bevorzugt Power-Ups fallen |
| Command Mothership (UFO) | 1 | 500–1500 | Alle 18–25 s bei `y = 36`, lässt garantiert eine Kapsel fallen |

- **Formation:** 10 Spalten, bewegt sich geschlossen. Berührt der äußerste *lebende* Gegner den Rand (`x ≤ 24` oder `x ≥ 876`), wird geklemmt, die Formation rückt um 18 Einheiten nach unten, kehrt um und wird mit weniger Gegnern logarithmisch schneller.
- **Column-Bottom-Shooter:** Nur der unterste lebende Gegner jeder Spalte feuert. Gleichzeitig sind je nach Welle 4 bis 7 Gegner-Projektile erlaubt.
- **Dreadnought** (jede 5. Welle): Kern (25+ HP), zwei zerstörbare Seiten-Geschütze und ein Laser-Sweep, der **0,9 s vorher** als Warnlinie angezeigt wird.
- **Bunker:** 3 Kuppeln aus je einem `7×4`-Raster von Micro-Zellen (unten in der Mitte offen). Jeder Treffer, auch durch eigene Schüsse, entfernt genau eine Zelle.
- **Game Over:** Hull 0 oder ein Gegner erreicht die Defense Line.

### Power-Ups (Einsammeln: +150 Punkte)

| Kapsel | Wirkung |
|---|---|
| `TRI-SPREAD` (10 s) | 3 gefächerte Bolzen |
| `RAIL-BEAM` (8 s) | Sofortiger Strahl, der ganze Gegner-Spalten durchschlägt |
| `OVERCLOCK` (10 s) | Halber Feuer-Cooldown, +25 % Schiffstempo |
| `AEGIS REPAIR` | +1 Hull (bei vollem Hull: alle Bunker-Zellen repariert) |
| `VIBE SURGE` | +30 Flow und eine EMP-Welle, die alle Gegner-Projektile auflöst |

Ein Treffer kostet 1 Hull (Start 3, max. 5), gibt 1,5 s Schild und setzt aktive Upgrades zurück.

### Flow und Graze

Dieselbe Flow-Engine wie in den anderen Spielen. Abschüsse innerhalb von 2,5 s bauen eine Combo auf. **Graze:** Streift ein Gegner-Projektil das Schiff im Radius 22 ohne zu treffen, gibt es einmalig pro Projektil `+4` Flow und `+50` Punkte (`"GRAZE!"`). Im Zustand **Overdrive (2,0×)** bekommt das Schiff zusätzliche Flügelspitzen-Blaster und einen chromatischen Neon-Schweif.

### Modi

| Modus | Regeln |
|---|---|
| **Classic Waves** | Wellen mit steigendem Tempo, UFOs, Bunker und Dreadnought alle 5 Wellen |
| **Bullet Swarm** | Mehr Interceptoren und Weaver, Dash-Cooldown 1,1 s, höhere Graze-Boni, häufigere Drops |
| **Daily Vibe** | Der `YYYYMMDD`-Seed bestimmt Aufstellung, UFO-Zeitpunkte und Power-Up-Drops. Alles wird aus dem Seed vorausberechnet und hängt nie vom Spielerverhalten ab. |
| **Zen** | Kein Game Over: bei Hull 0 oder Durchbruch löst ein EMP aus, löscht Projektile und setzt die Welle zurück |

---

## VibeCoded Glide

Ein präziser Endless-Flyer in einer `960×640`-Arena (zentriert in der 1280×720-Bühne, flankiert von zwei HUD-Panels). Der Gleiter bleibt auf `x = 260`, die Röhren scrollen heran. Boden bei `y = 590`, Decke bei `y = 20`.

### Steuerung

| Aktion | Eingabe |
|---|---|
| Flap | `Space` / `W` / `↑` / Linksklick: setzt `vy` fest auf `-430` (kein Addieren). Ein Tipp vor dem Rundenstart wird 80 ms gepuffert. |
| Dive | `↓` / `S` gehalten: +65 % Gravitation |
| Phase-Dash | `Shift` / `C` / Rechtsklick: 0,22 s Phase-Shield (i-frames), Cooldown 2,5 s |
| Pause | `P` / `Esc` |

**Der Dash** schiebt die Welt für die Dauer des Schilds 2,6× schneller am Gleiter vorbei. Erst dadurch reichen 0,22 s i-frames, um eine komplette Röhre (Körper + Lippen + Gleiter-Durchmesser, rund 122 Einheiten) zu durchgleiten. Boden und Decke bleiben auch während des Dashs tödlich.

### Physik und Röhren

- **Feste 120-Hz-Schritte** (`dt` vorher auf 0,05 s gekappt): Sprunghöhe und Flugbahn sind auf 60 Hz und 144 Hz bitgenau identisch (Test).
- **Röhren:** 76 breit mit je 8 Einheiten Lippenüberstand, Abstand 310, Lücke 168 (schrumpft sanft auf 138). Die Lückenmitte liegt in `135..=475` und weicht höchstens **195** von der vorherigen ab. Die Garantie gilt auch für die *bewegten* Lücken des Kinetic Shift, weil die Amplituden entsprechend begrenzt werden.
- **Kollision:** exakter Kreis (`r = 15`) gegen Rechteck für Röhrenkörper und Lippen, dazu Boden und Decke.
- **Tempo:** 220 bis 340 Einheiten/s, logarithmisch nach passierten Röhren.

### Wertung

| Ereignis | Punkte | Flow |
|---|---|---|
| Röhre passiert | `+100 ×` Flow-Multiplikator | +3 |
| **Vibe-Ring** (jede 2. Lücke, leicht außermittig) | `+150 ×` Flow-Multiplikator, Ring-Combo +1 | +15 |
| **Close Skim** (Lippe mit weniger als 10 Einheiten Abstand, einmal pro Röhre, nie nach Kontakt oder während des Dashs) | +50 | +5 |

Ein ausgelassener Ring setzt die Combo zurück, ohne zu töten. Im **Overdrive (2,0×)** werden Lücken von Röhren, die noch weit voraus liegen, um 16 Einheiten breiter (nahe Röhren ändern sich nie mehr), und der Gleiter zieht einen Regenbogen-Kondensstreifen.

### Modi

| Modus | Regeln |
|---|---|
| **Classic Glide** | Statische Röhren, steigendes Tempo, Vibe-Ringe |
| **Kinetic Shift** | Ab der 6. Röhre schwingen einzelne Röhrenpaare sinusförmig. Zeitlich getaktete Windböen verändern den Auftrieb. |
| **Daily Vibe** | Der `YYYYMMDD`-Seed bestimmt Röhren, Ringe, Bewegungen und Böen. Sie hängen nie vom Spielerverhalten ab. |
| **Zen** | Kein Tod: bei Kontakt prallt der Gleiter mit EMP-Blitz ab und fliegt weiter (nach einem Röhrentreffer kurz geschützt, damit er hinausgleiten kann) |

---

## Flow-Engine (gemeinsam)

| Stufe | Flow | Score-Multiplikator | Shader-Takt |
|---|---|---|---|
| Chill | 0–25 | 1.0× | 110 BPM |
| Groove | 25–60 | 1.25× | 116 BPM |
| Sync | 60–90 | 1.5× | 124 BPM |
| Overdrive | 90–100 | 2.0× | 132 BPM |

Blocks: Lock < 1,2 s nach dem Spawn +5, Single/Double/Triple/Quad +10/16/22/30, T-Spin +25, Combo +6 × Combo. Snake: Apfel +8 (+4, wenn zwei Äpfel innerhalb von 2 s), Pulse-Apfel +25. Armada: Abschuss +3 (+0,8 je Combo-Stufe), Graze +4, UFO +6, Vibe Surge +30. Glide: Röhre +3, Ring +15, Skim +5. Bei 100 greift ein 4-s-Overdrive-Shield. Nach 1,5 s ohne Ereignis (und ohne Shield) fällt der Flow um 3,5/s. **Flow beeinflusst nie die Gravity bzw. das Schlangentempo.** Das prüft ein eigener Test.

## Hintergrundmusik (gemeinsam)

Die Musik wird beim Start in einem eigenen Thread prozedural erzeugt (keine Asset-Dateien) und als nahtloser 8-Takt-Loop abgespielt. Das Menü hat einen ruhigen Track (84 BPM, Pad, Glocken, kein Schlagzeug). Im Spiel gibt es fünf Stufen, die beim Wechsel ineinander überblenden. Jedes Spiel meldet seine Stufe selbst (`FrameEvents::music_stage`): Blocks nach Level bzw. Sprint-Fortschritt, Snake nach der aktuellen Grundgeschwindigkeit, Armada nach der Welle (der Dreadnought ist der Höhepunkt), Glide nach dem Scroll-Tempo.

| Stufe | Blocks: Marathon / Daily | Blocks: Sprint | Snake (Schritte/s) | Tempo | neu dazu |
|---|---|---|---|---|---|
| 1 | Level 1–2 | 0–9 Lines | 7,5–9,6 | 112 BPM | Pad, Bass, Kick auf 1 & 3, Hi-Hats |
| 2 | Level 3–5 | 10–19 Lines | 9,6–11,7 | 122 BPM | Four-on-the-floor, Snare, treibender Bass, Sidechain-Pumpen |
| 3 | Level 6–9 | 20–29 Lines | 11,7–13,8 | 132 BPM | 16tel-Hats, 16tel-Arpeggio |
| 4 | Level 10–14 | 30–34 Lines | 13,8–15,9 | 144 BPM | Lead-Melodie, Oktav-Bass, Open Hats |
| 5 | ab Level 15 | ab 35 Lines | ab 15,9 | 158 BPM | Melodie gedoppelt, verzerrter Bass, Snare-Wirbel |

Zen (Blocks) bleibt auf Stufe 1. In der Pause wird die Musik leiser, am Spielende blendet sie aus, damit der Jingle durchkommt. `M` und der Lautstärkeregler gelten auch für die Musik.

---

## Architektur (Kurzfassung)

Details, Datenfluss und die Anleitung für neue Spiele stehen in **[ARCHITECTURE.md](ARCHITECTURE.md)**.

```
src/
├── core/            1) Reine Logik, NULL macroquad-Imports (per Test erzwungen)
│   ├── shared/         SplitMix64 + Tages-Seed, FlowEngine (Tiers, Shield, BPM), Datum
│   ├── blocks/         Matrix, SRS, 7-Bag, Scoring, GameState (Blocks)
│   ├── snake/          Grid, TurnBuffer, Hindernisse, SnakeGame, Interpolation, Bot
│   ├── armada/         AABB, Bunker-Micro-Zellen, Wellen/Drops/UFO-Plan, ArmadaGame, Bot
│   └── glide/          Kreis-gegen-Rechteck, Kurs- und Böen-Generator, 120-Hz-Physik, GlideGame, Bot
├── engine/          2) Blocks-Simulation (240-Hz-Fixed-Step, DAS/ARR, Autopilot)
├── arcade/          3) Die Spiel-Schnittstelle: ArcadeTitle, GameRegistry, FrameEvents,
│                       ArcadeInput, Nav (Menu/Playing/Paused/Ending/Results)
├── presentation/    4) Arcade-Controller (app.rs), Menü, Settings, Widget-Toolkit
│   ├── widgets/        RoundedPanel, ActionButton, ModePillSelector, StatBox, FlowMeterBar,
│   │                   ModalOverlay (Pause/Ergebnis), ParticleSystem, FloatingCallouts
│   ├── armada_ui.rs    Zeichencode von Armada (Schiff, Gegner, Boss, HUD, Vorschau)
│   ├── glide_ui.rs     Zeichencode von Glide (Gleiter, Röhren, Ringe, Skyline, HUD, Vorschau)
│   └── …               Stage (1280×720), Text, Vektor-Ziffern, Shader, Synth, Musik, Audio
├── titles/          5) Die Spiele als ArcadeTitle-Adapter. default_registry() ist die
│   ├── blocks/         einzige Stelle, die konkrete Spiele kennt.
│   ├── snake/
│   ├── armada/
│   └── glide/
├── infrastructure/  6) Save (Key-Value-Highscores), Uhr, CLI, Smoke-Test
└── main.rs          Einstiegspunkt
```

`#![forbid(unsafe_code)]` gilt crate-weit. `tests/architecture.rs` erzwingt: kein `macroquad` in `core` und `engine`, `core::shared` kennt kein Spiel, Blocks, Snake, Armada und Glide kennen sich nicht, und Controller, Menü, Widgets und Save nennen nie einen konkreten Titel. `tests/plugin.rs` registriert ein drittes Spiel nur über die öffentliche API und lässt Smoke-Test, Menü-Layout und Highscores darüber laufen.

### Robustheit unter Windows

- **Keine Konsole:** `windows_subsystem = "windows"` in Release-Builds.
- **Kein Audiogerät:** Das Windows-Audio-Backend öffnet das Gerät in einem eigenen Thread und bricht dort per `assert!` ab, wenn keins da ist. Ein Panic-Hook fängt Panics in Nicht-Haupt-Threads ab und schaltet auf „NO DEVICE“ (lautlos). Das Laden der Sounds läuft zusätzlich in `catch_unwind`. Kein `.unwrap()` im Audiopfad.
- **Defekte/fehlende Save-Datei:** Es wird mit Defaults weitergespielt. Werte werden auf gültige Bereiche geklemmt, geschrieben wird atomar (Temp-Datei + Rename). Alte Dateien werden automatisch migriert.
- **Shader-Fehler:** Kompiliert der Shader nicht, zeichnet ein CPU-Fallback Gradient und Grid.
- **High-DPI:** Viewport in physischen Pixeln, Maus über dieselbe Letterbox-Mathematik (unit-getestet für 100/125/150 %).

## Build, Test, Qualität

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                   # Unit-, Architektur-, Plugin- und Smoke-Tests
cargo build --release                        # target\release\VibeCodedGame.exe
```

Die Release-Exe ist ein GUI-Programm (ohne Konsole). Deshalb wartet man in PowerShell explizit auf das Ende und prüft den Exit-Code:

```powershell
$p = Start-Process .\target\release\VibeCodedGame.exe -ArgumentList '--smoke-test' -Wait -PassThru; $p.ExitCode   # 0
```

Release-Profil: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`.

### CI: GitHub Actions

`.github/workflows/build-release.yml` läuft auf `windows-latest` und führt `fmt --check`, `clippy --all-targets -D warnings`, `cargo test`, `cargo build --release` und den Smoke-Test der fertigen Exe (`VibeCodedGame.exe --smoke-test`, alle Spiele) aus. Danach lädt er `VibeCodedGame.exe` als Artifact hoch. Bei einem Tag `v*` wird zusätzlich ein GitHub-Release erstellt:

```powershell
git tag v2.0.0; git push origin v2.0.0
```

Falls der Workflow nicht mit gepusht werden konnte (fehlender `workflow`-Scope des Tokens), lässt er sich mit diesem Einzeiler nachträglich aktivieren:

```powershell
gh auth refresh -s workflow; git add .github/workflows/build-release.yml; git commit -m "ci: windows build"; git push
```

### Testabdeckung (Auswahl)

- **Blocks:** Wand- und Floor-Kicks inkl. y-Inversion, I-Kicks, Lock Delay (500 ms, 15 Resets, neue tiefste Zeile), T-Spin (3-Corner-Rule), Hold, Block Out / Lock Out, Zen, SplitMix64-Referenzwerte, 7-Bag, Flow-Tiers, Rising Edge, DAS/ARR, SOCD
- **Snake:** 180°-TurnBuffer (auch exhaustiv über alle Richtungsfolgen), Tail-Vacating und der Gegenfall bei ausstehendem Wachstum, Portal-Wrap an allen vier Kanten, Victory bei vollem Brett, Daily-Seed (Determinismus, Symmetrie, keine Sackgassen, zusammenhängend), Pulse-Apfel (Timer, +250, +2 Länge, +25 Flow), Geschwindigkeitskurve (monoton, 7,5–18), Zen-Trim, Interpolation ohne Wrap-Streifen, Bot überlebt in jedem Modus
- **Armada:** AABB-Kollision (Kanten, Enthaltensein, kein Tunneln), Bunker-Kuppel und Micro-Zellen-Zerstörung von beiden Seiten, Formations-Edge-Bounce (kein Steckenbleiben nach Abschuss der Randspalte), Column-Bottom-Shooter, Phase-Dash-i-Frames und Cooldowns, Graze genau einmal (nie bei einem Treffer), alle 5 Power-Ups, Boss-Telegraph (0,9 s), Zen-EMP, Daily-Determinismus unabhängig von den Eingaben
- **Glide:** 120-Hz-Determinismus (60 vs. 144 Hz bitgenau), Flap setzt `vy`, Dive, Kreis-gegen-Rechteck (Tangente, Ecke, Lippenüberstand), `max_delta_y` auch für bewegte Lücken, Vibe-Ring/Skim-Wertung (genau einmal, nie nach Kontakt), Dash durch eine geschlossene Röhre, Overdrive-Verbreiterung ohne Verengung unter dem Spieler, Zen ohne Bounce-Sturm, Daily-Determinismus unabhängig von den Eingaben
- **Plattform:** Registry (Duplikat-IDs, Arcade-Score), Navigation, Menü-Layout für 1–9 Spiele, Maus-/Tastatur-Menü, Settings-Logik, Widgets, generische Audio-Cues, Migration alter Save-Dateien, Save-Roundtrip/Korruption, Plugin-Test mit drittem Spiel
- **Smoke-Test** über alle Spiele und Modi, WAV-Header Byte für Byte, Letterbox- und Maus-Mapping, CLI

## Speicherort

`%APPDATA%\VibeCodedGame\save.json` enthält die Einstellungen (DAS/ARR, Lautstärke, Mute, Theme, Farbenblind, Snake-Boost) und alle Highscores als generische Schlüssel/Wert-Paare:

```json
{ "highscores": { "blocks:marathon": 5000, "blocks:sprint": 61000, "snake:classic": 1200, "daily:snake:2026-11-16": 800 } }
```

Neue Spiele brauchen **keine neuen Felder**. Zeit-Modi speichern Millisekunden (kleiner ist besser). Dateien aus Version 1 (mit `best_scores`, `best_sprint_ms`, `daily`) werden beim Laden automatisch in dieses Format übernommen und beim nächsten Speichern im neuen Format geschrieben. Löschen setzt alles zurück.

## Lizenz

MIT
