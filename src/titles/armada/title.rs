//! `ArcadeTitle` adapter of VibeCoded Armada: input mapping, events to
//! sound/particles/callouts, session end, smoke bot.

use macroquad::prelude::*;

use crate::arcade::{
    ArcadeInput, ArcadeTitle, FrameEvents, GameModeInfo, Key, SessionEnd, TitleContext,
    VirtualCanvas, cue,
};
use crate::core::FlowTier;
use crate::core::armada::bot;
use crate::core::armada::game::{
    ArmadaEvent, ArmadaGame, ArmadaInput, ArmadaMode, Cause, HULL_MAX, SHIP_Y, Status,
};
use crate::core::armada::waves::EnemyKind;
use crate::presentation::armada_ui::{self, UiCtx, to_stage};
use crate::presentation::digits;
use crate::presentation::text::Size;
use crate::presentation::theme::{Theme, hex, themes};

const MODES: [GameModeInfo; 4] = [
    GameModeInfo::new(
        "classic",
        "CLASSIC WAVES",
        "Waves, UFOs and bunkers. A Dreadnought attacks every 5th wave.",
    ),
    GameModeInfo::new(
        "swarm",
        "BULLET SWARM",
        "More fire, faster dashes, bigger graze bonuses and more drops.",
    ),
    GameModeInfo::new(
        "daily",
        "DAILY VIBE",
        "Same waves, UFOs and power-up drops for everyone today.",
    )
    .daily(),
    GameModeInfo::new(
        "zen",
        "ZEN",
        "Cannot lose: an EMP wipes enemy fire and resets the wave.",
    ),
];

const LEGEND: [(&str, &str); 5] = [
    ("LEFT / RIGHT / A / D", "Move"),
    ("SPACE / W / UP / J", "Fire (hold for auto-fire)"),
    ("SHIFT / C / K", "Phase dash (i-frames)"),
    ("P / ESC", "Pause"),
    ("T / B / M", "Theme / colour-blind / mute"),
];

/// Arcade keys -> Armada input.
pub fn input_from(input: &ArcadeInput) -> ArmadaInput {
    let left = input.any_down(&[Key::Left, Key::A]);
    let right = input.any_down(&[Key::Right, Key::D]);
    ArmadaInput {
        move_x: right as i32 as f32 - left as i32 as f32,
        fire_held: input.any_down(&[Key::Space, Key::W, Key::Up, Key::J]),
        fire_pressed: input.any_pressed(&[Key::Space, Key::W, Key::Up, Key::J]),
        dash_pressed: input.any_pressed(&[Key::Shift, Key::C, Key::K]),
    }
}

pub struct VibeCodedArmada {
    game: ArmadaGame,
    ctx: TitleContext,
    themes: [Theme; 2],
    time: f32,
    reported: bool,
    last_tier: FlowTier,
}

impl Default for VibeCodedArmada {
    fn default() -> Self {
        Self::new()
    }
}

impl VibeCodedArmada {
    pub fn new() -> Self {
        VibeCodedArmada {
            game: ArmadaGame::new(ArmadaMode::Classic, 1),
            ctx: TitleContext::default(),
            themes: themes(),
            time: 0.0,
            reported: false,
            last_tier: FlowTier::Chill,
        }
    }

    pub fn game(&self) -> &ArmadaGame {
        &self.game
    }

    fn theme(&self) -> &Theme {
        &self.themes[self.ctx.settings.theme % self.themes.len()]
    }

    fn advance(&mut self, dt: f32, input: &ArmadaInput) -> FrameEvents {
        self.time += dt;
        self.game.update(dt, input);
        let mut ev = self.collect_events();
        if !self.game.is_playing() && !self.reported {
            self.reported = true;
            ev.session_end = Some(self.session_end());
        }
        ev
    }

    fn collect_events(&mut self) -> FrameEvents {
        let theme = self.theme().clone();
        let mut ev = FrameEvents::default();
        let tier = self.game.tier();
        let ti = tier.index();
        for e in self.game.drain_events() {
            match e {
                ArmadaEvent::Fired { x, rail, .. } => {
                    let p = to_stage(x, SHIP_Y - 14.0);
                    ev.burst(p, theme.accent2, if rail { 12 } else { 3 }, 150.0);
                    ev.sound(cue::ACTION_TICK, if rail { 0.5 } else { 0.22 });
                }
                ArmadaEvent::EnemyHit { x, y, shield_broke } => {
                    ev.burst(to_stage(x, y), theme.text, 7, 220.0);
                    // Metal ping on a cracked shield.
                    ev.sound(cue::ACTION_TICK, if shield_broke { 0.5 } else { 0.3 });
                }
                ArmadaEvent::EnemyKilled { x, y, kind, points } => {
                    let col = match kind {
                        EnemyKind::Scout => theme.pieces[3],
                        EnemyKind::Interceptor => theme.pieces[4],
                        EnemyKind::Aegis => theme.pieces[5],
                        EnemyKind::Weaver => theme.pieces[2],
                    };
                    let p = to_stage(x, y);
                    ev.burst(p, col, 16, 280.0);
                    ev.callout(format!("+{points}"), p, theme.text, Size::X1);
                    ev.sound(cue::CLEAR_OR_EAT, 0.35);
                    ev.note(ti, (self.game.combo as usize * 2) % 8, 0.45);
                }
                ArmadaEvent::UfoSpawned => ev.sound(cue::ACTION_TICK, 0.4),
                ArmadaEvent::UfoKilled { x, points } => {
                    let p = to_stage(x, 40.0);
                    ev.callout(
                        format!("UFO BONUS! +{points}"),
                        p + vec2(0.0, 30.0),
                        theme.pieces[1],
                        Size::X2,
                    );
                    ev.burst(p, theme.pieces[1], 30, 340.0);
                    ev.flash(theme.pieces[1], 0.4);
                    ev.sound(cue::CLEAR_OR_EAT, 0.8);
                }
                ArmadaEvent::PlayerHit => {
                    ev.shake = 12.0;
                    ev.flash(theme.danger, 0.8);
                    let p = to_stage(self.game.ship.x, SHIP_Y);
                    ev.burst(p, theme.danger, 26, 320.0);
                    ev.sound(cue::GAME_OVER, 0.4);
                }
                ArmadaEvent::Graze { x, y } => {
                    let p = to_stage(x, y);
                    ev.callout("GRAZE!", p - vec2(0.0, 36.0), theme.accent2, Size::X1);
                    ev.sound(cue::ACTION_TICK, 0.35);
                    ev.note(ti, 6, 0.3);
                }
                ArmadaEvent::Dash => {
                    let p = to_stage(self.game.ship.x, SHIP_Y);
                    ev.burst(p, theme.accent, 10, 200.0);
                    ev.sound(cue::ROTATE_OR_TURN, 0.5);
                }
                ArmadaEvent::PowerUp { kind, x, y } => {
                    let p = to_stage(x, y);
                    ev.callout(
                        kind.name(),
                        p - vec2(0.0, 20.0),
                        armada_ui::power_color(&theme, kind),
                        Size::X2,
                    );
                    ev.burst(p, armada_ui::power_color(&theme, kind), 20, 260.0);
                    ev.flash(armada_ui::power_color(&theme, kind), 0.35);
                    ev.sound(cue::CLEAR_OR_EAT, 0.7);
                }
                ArmadaEvent::CapsuleDropped { .. } => ev.sound(cue::ACTION_TICK, 0.3),
                ArmadaEvent::BunkerHit { x, y } => {
                    ev.burst(to_stage(x, y), theme.pieces[3], 4, 120.0);
                }
                ArmadaEvent::WaveStart { wave, boss } => {
                    let text = if boss {
                        "DREADNOUGHT!".to_string()
                    } else {
                        format!("WAVE {wave}")
                    };
                    ev.callout(text, to_stage(450.0, 340.0), theme.text, Size::X3);
                }
                ArmadaEvent::WaveCleared { .. } => {
                    ev.callout(
                        "WAVE CLEAR!",
                        to_stage(450.0, 260.0),
                        theme.accent2,
                        Size::X3,
                    );
                    ev.flash(theme.accent2, 0.5);
                    ev.sound("levelup", 0.6);
                }
                ArmadaEvent::BossTelegraph { .. } => {
                    ev.callout("LASER!", to_stage(450.0, 220.0), theme.danger, Size::X2);
                    ev.sound(cue::ACTION_TICK, 0.6);
                }
                ArmadaEvent::BossFire => {
                    ev.shake = ev.shake.max(6.0);
                    ev.sound("harddrop", 0.5);
                }
                ArmadaEvent::BossTurretDown { x, y } => {
                    ev.burst(to_stage(x, y), theme.pieces[6], 24, 300.0);
                    ev.sound(cue::CLEAR_OR_EAT, 0.6);
                }
                ArmadaEvent::BossDown => {
                    ev.burst(to_stage(450.0, 120.0), theme.pieces[6], 80, 420.0);
                    ev.flash(theme.accent2, 1.0);
                    ev.shake = 10.0;
                    ev.sound("finish", 0.8);
                    ev.callout(
                        "DREADNOUGHT DOWN!",
                        to_stage(450.0, 300.0),
                        theme.accent2,
                        Size::X3,
                    );
                }
                ArmadaEvent::Emp => {
                    ev.flash(theme.accent, 0.8);
                    ev.callout("EMP!", to_stage(450.0, 320.0), theme.accent, Size::X3);
                    ev.sound(cue::OVERDRIVE_UP, 0.6);
                }
                ArmadaEvent::Died(_) => {
                    ev.shake = 14.0;
                    ev.sound(cue::GAME_OVER, 0.8);
                }
            }
        }
        if tier > self.last_tier {
            let c = theme.tiers[ti];
            if tier == FlowTier::Overdrive {
                ev.callout("OVERDRIVE 2.0x", to_stage(450.0, 400.0), c, Size::X3);
                ev.flash(c, 1.0);
                ev.sound(cue::OVERDRIVE_UP, 0.8);
            } else {
                ev.callout(
                    format!("{} {}x", tier.name(), tier.multiplier()),
                    to_stage(450.0, 400.0),
                    c,
                    Size::X2,
                );
            }
        }
        self.last_tier = tier;
        ev.flow = self.game.flow.flow;
        ev.music_stage = armada_ui::music_stage(&self.game);
        ev
    }

    fn session_end(&self) -> SessionEnd {
        let g = &self.game;
        let title = match g.status {
            Status::Dead(Cause::DefenseBreach) => "DEFENSES BREACHED",
            Status::Dead(Cause::HullDestroyed) => "SHIP DESTROYED",
            Status::Playing => "GAME OVER",
        };
        SessionEnd {
            title,
            won: false,
            headline: ("SCORE", g.score.to_string()),
            value: g.score,
            lower_is_better: false,
            counts_for_record: true,
            summary: vec![
                ("WAVE".into(), g.wave.to_string()),
                ("KILLS".into(), g.kills.to_string()),
                ("GRAZES".into(), g.grazes.to_string()),
                ("TIME".into(), digits::format_time(g.elapsed)),
            ],
        }
    }

    /// Invariants checked on every headless bot step.
    fn check_invariants(&self) {
        let g = &self.game;
        assert!((30.0..=870.0).contains(&g.ship.x), "ship left its lane");
        assert!((0.0..=100.0).contains(&g.flow.flow), "flow out of range");
        assert!(g.hull >= 1 && g.hull <= HULL_MAX, "hull out of range");
        assert!(g.enemy_bolts.len() <= 8, "too many enemy bolts");
        for e in g.enemies.iter().filter(|e| e.alive) {
            assert!(e.hp >= 1 && e.hp <= e.kind.hp());
        }
        for b in &g.bunkers {
            assert!(b.alive() <= crate::core::armada::bunker::Bunker::full_count());
        }
    }
}

impl ArcadeTitle for VibeCodedArmada {
    fn id(&self) -> &'static str {
        "armada"
    }

    fn title(&self) -> &'static str {
        "VIBECODED ARMADA"
    }

    fn tagline(&self) -> &'static str {
        "dash, graze, defend the line"
    }

    fn accent_color(&self) -> Color {
        hex(0xFF9A3C)
    }

    fn modes(&self) -> &'static [GameModeInfo] {
        &MODES
    }

    fn controls_legend(&self) -> &'static [(&'static str, &'static str)] {
        &LEGEND
    }

    fn start(&mut self, mode_id: &str, seed: u64) {
        self.game = ArmadaGame::new(ArmadaMode::from_id(mode_id), seed);
        self.time = 0.0;
        self.reported = false;
        self.last_tier = FlowTier::Chill;
    }

    fn dev_preset(&mut self, name: &str) {
        use crate::core::armada::game::{UFO_Y, Ufo};
        use crate::core::armada::waves::{PowerKind, UfoFlight};
        let (mode, seed) = (self.game.mode, self.game.seed);
        match name {
            "boss" => self.game = ArmadaGame::new_at_wave(mode, seed, 5),
            "breach" => self.game.formation.y = 600.0,
            // Overdrive wings, every capsule, a UFO and a few active effects.
            "showcase" => {
                let g = &mut self.game;
                g.flow.flow = 95.0;
                g.weapon = Some((PowerKind::TriSpread, 8.0));
                g.overclock = 8.0;
                g.combo = 4;
                g.combo_timer = 2.0;
                g.hull = 4;
                for (i, k) in PowerKind::ALL.iter().enumerate() {
                    g.capsules.push(crate::core::armada::game::Capsule {
                        x: 250.0 + i as f32 * 100.0,
                        y: 300.0 + (i % 2) as f32 * 60.0,
                        kind: *k,
                    });
                }
                g.ufo = Some(Ufo {
                    x: 450.0,
                    dir: 0.0,
                    flight: UfoFlight {
                        points: 1000,
                        from_left: true,
                        drop: PowerKind::VibeSurge,
                    },
                });
                let _ = UFO_Y;
                g.ship.x = 450.0;
            }
            _ => {}
        }
    }

    fn set_context(&mut self, ctx: &TitleContext) {
        self.ctx = ctx.clone();
    }

    fn update(&mut self, dt: f32, input: &ArcadeInput) -> FrameEvents {
        let i = input_from(input);
        self.advance(dt, &i)
    }

    fn demo_update(&mut self, dt: f32) -> Option<FrameEvents> {
        let i = bot::choose(&self.game);
        Some(self.advance(dt, &i))
    }

    fn draw(&self, canvas: &VirtualCanvas, theme: &Theme, colorblind: bool) {
        let label = MODES
            .iter()
            .find(|m| m.id == self.game.mode.id())
            .map(|m| m.label)
            .unwrap_or("");
        let ctx = UiCtx {
            stage: canvas,
            theme,
            colorblind,
            time: self.time,
            pulse: self.ctx.pulse,
            best: self.ctx.best,
            mode_label: label,
        };
        armada_ui::draw_arena(&self.game, &ctx);
        armada_ui::draw_hud(&self.game, &ctx);
    }

    fn draw_preview(&self, rect: Rect, time: f32, theme: &Theme) {
        armada_ui::draw_preview(rect, time, theme);
    }

    fn simulate_smoke_step(&mut self, tick: usize) {
        // A death of the bot is a regression and must fail loudly.
        assert!(
            self.game.is_playing(),
            "armada bot died at tick {tick}: {:?}",
            self.game.status
        );
        let i = bot::choose(&self.game);
        let ev = self.advance(1.0 / 60.0, &i);
        assert!(ev.flow.is_finite());
        self.check_invariants();
        if tick >= 280 {
            let g = &self.game;
            assert!(
                g.shots > 0 && g.kills > 0 && g.score > 0,
                "bot made no progress"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::armada::bunker::Bunker;

    fn tap(keys: &[Key]) -> ArcadeInput {
        ArcadeInput::tap(keys)
    }

    #[test]
    fn every_documented_key_maps_to_its_action() {
        for k in [Key::Space, Key::W, Key::Up, Key::J] {
            let i = input_from(&tap(&[k]));
            assert!(i.fire_held && i.fire_pressed, "{k:?} fires");
            assert!(!i.dash_pressed);
        }
        for k in [Key::Shift, Key::C, Key::K] {
            let i = input_from(&tap(&[k]));
            assert!(i.dash_pressed && !i.fire_held, "{k:?} dashes");
        }
        for (k, dir) in [
            (Key::Left, -1.0),
            (Key::A, -1.0),
            (Key::Right, 1.0),
            (Key::D, 1.0),
        ] {
            assert_eq!(input_from(&ArcadeInput::new([k], [])).move_x, dir, "{k:?}");
        }
        let both = ArcadeInput::new([Key::Left, Key::Right], []);
        assert_eq!(input_from(&both).move_x, 0.0, "opposite keys cancel");
        // Holding fire does not press it again.
        let held = input_from(&ArcadeInput::new([Key::Space], []));
        assert!(held.fire_held && !held.fire_pressed);
    }

    #[test]
    fn smoke_bot_plays_every_mode_with_progress() {
        for m in MODES {
            let mut t = VibeCodedArmada::new();
            t.start(m.id, 20261116);
            for tick in 0..300 {
                t.simulate_smoke_step(tick);
            }
            assert!(t.game().kills > 0 && t.game().shots > 0, "{}", m.id);
        }
    }

    #[test]
    fn breach_reports_one_session_end_with_stats_but_zen_never_ends() {
        let mut t = VibeCodedArmada::new();
        t.start("classic", 5);
        t.game.formation.y = 600.0;
        let mut ends = Vec::new();
        for _ in 0..30 {
            if let Some(e) = t.update(1.0 / 30.0, &ArcadeInput::EMPTY).session_end {
                ends.push(e);
            }
        }
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].title, "DEFENSES BREACHED");
        assert!(ends[0].counts_for_record && !ends[0].lower_is_better);
        assert!(ends[0].summary.iter().any(|(k, _)| k == "WAVE"));

        let mut z = VibeCodedArmada::new();
        z.start("zen", 5);
        z.game.formation.y = 600.0;
        for _ in 0..90 {
            assert!(
                z.update(1.0 / 30.0, &ArcadeInput::EMPTY)
                    .session_end
                    .is_none()
            );
        }
        assert!(z.game().is_playing());
    }

    #[test]
    fn events_become_sound_particles_and_callouts() {
        let mut t = VibeCodedArmada::new();
        t.start("classic", 5);
        // Fire, then take a graze-free kill by placing a bolt on an enemy.
        let ev = t.update(0.02, &tap(&[Key::Space]));
        assert!(
            !ev.audio.is_empty() && !ev.particles.is_empty(),
            "muzzle flash + sound"
        );
        // Wave start produced a callout on the very first frame.
        let mut t = VibeCodedArmada::new();
        t.start("classic", 5);
        let ev = t.update(0.0, &ArcadeInput::EMPTY);
        assert!(ev.callouts.iter().any(|c| c.text == "WAVE 1"));
        assert!(ev.music_stage == 0 && ev.flow == 0.0);
    }

    #[test]
    fn every_sound_the_title_requests_exists_in_the_bank() {
        let bank = crate::presentation::synth::build_bank();
        let mut names = std::collections::HashSet::new();
        for (preset, mode) in [
            ("boss", "classic"),
            ("showcase", "classic"),
            ("breach", "zen"),
            ("", "swarm"),
        ] {
            let mut t = VibeCodedArmada::new();
            t.start(mode, 11);
            t.dev_preset(preset);
            for _ in 0..1500 {
                let ev = t.demo_update(1.0 / 60.0).unwrap();
                for c in ev.audio {
                    if let crate::arcade::AudioCue::Sound { name, .. } = c {
                        names.insert(name);
                    }
                }
                assert!(ev.music_stage < crate::presentation::music::GAME_STAGES);
            }
        }
        assert!(
            names.len() >= 4,
            "the scenarios exercise several sounds: {names:?}"
        );
        for n in names {
            assert!(
                bank.iter().any(|(b, _)| b == n),
                "sound '{n}' missing in the bank"
            );
        }
    }

    #[test]
    fn demo_bot_plays_and_metadata_is_complete() {
        let mut t = VibeCodedArmada::new();
        t.start("classic", 9);
        for _ in 0..600 {
            let ev = t.demo_update(1.0 / 60.0).expect("demo supported");
            assert!(ev.session_end.is_none());
        }
        assert!(t.game().kills > 0);
        assert_eq!(t.id(), "armada");
        assert_eq!(t.title(), "VIBECODED ARMADA");
        assert_eq!(t.modes().len(), 4);
        assert!(t.modes().iter().any(|m| m.is_daily));
        assert_eq!(Bunker::full_count(), 23);
    }
}
