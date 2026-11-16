//! "Juice": particles, hard-drop light trails, board impact spring, lock
//! flashes, line-clear flashes and floating callouts.
//!
//! Simulation of the effects is pure math (testable); drawing uses macroquad.

use macroquad::prelude::*;

use crate::core::PieceKind;
use crate::core::rng::SplitMix64;

use super::blocks;
use super::stage::Stage;
use super::text::{self, Align, Size};
use super::theme::with_alpha;

/// Hard-drop trail fade time.
pub const TRAIL_SECS: f32 = 0.18;
/// Lock flash duration.
pub const LOCK_FLASH_SECS: f32 = 0.08;
/// Line-clear row flash duration.
pub const ROW_FLASH_SECS: f32 = 0.22;
/// Callout lifetime.
pub const CALLOUT_SECS: f32 = 1.25;

/// Damped spring used for the board impact bounce.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub offset: f32,
    pub velocity: f32,
}

impl Spring {
    pub const STIFFNESS: f32 = 600.0;
    pub const DAMPING: f32 = 14.0;

    /// Kick for a hard drop over `distance` cells (peak ≈ 4–6 px).
    pub fn impact(&mut self, distance: u32) {
        self.velocity += 150.0 + distance.min(20) as f32 * 3.3;
    }

    pub fn update(&mut self, dt: f32) {
        // semi-implicit Euler in small sub-steps for stability
        let steps = ((dt / 0.004).ceil() as usize).clamp(1, 30);
        let h = dt / steps as f32;
        for _ in 0..steps {
            let acc = -Self::STIFFNESS * self.offset - Self::DAMPING * self.velocity;
            self.velocity += acc * h;
            self.offset += self.velocity * h;
        }
        if self.offset.abs() < 0.01 && self.velocity.abs() < 0.05 {
            *self = Spring::default();
        }
    }
}

#[derive(Clone, Debug)]
struct Particle {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max_life: f32,
    size: f32,
    color: Color,
}

#[derive(Clone, Debug)]
struct Trail {
    x: f32,
    y_top: f32,
    y_bottom: f32,
    w: f32,
    color: Color,
    age: f32,
}

#[derive(Clone, Debug)]
struct Flash {
    rect: Rect,
    age: f32,
    dur: f32,
    color: Color,
}

/// A block gently dissolving upward (Zen reset).
#[derive(Clone, Debug)]
struct Dissolve {
    pos: Vec2,
    color: Color,
    kind: PieceKind,
    age: f32,
}

/// Dissolve duration (after its individual delay).
pub const DISSOLVE_SECS: f32 = 0.9;

#[derive(Clone, Debug)]
struct Callout {
    text: String,
    pos: Vec2,
    age: f32,
    color: Color,
    size: Size,
}

/// All live effects.
pub struct Fx {
    rng: SplitMix64,
    particles: Vec<Particle>,
    trails: Vec<Trail>,
    flashes: Vec<Flash>,
    callouts: Vec<Callout>,
    dissolves: Vec<Dissolve>,
    pub spring: Spring,
    /// Screen-wide flash `0..1` (overdrive, quads).
    pub screen_flash: f32,
    pub screen_flash_color: Color,
}

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        Fx {
            rng: SplitMix64::new(0xF00D),
            particles: Vec::with_capacity(512),
            trails: Vec::new(),
            flashes: Vec::new(),
            callouts: Vec::new(),
            dissolves: Vec::new(),
            spring: Spring::default(),
            screen_flash: 0.0,
            screen_flash_color: WHITE,
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
        self.trails.clear();
        self.flashes.clear();
        self.callouts.clear();
        self.dissolves.clear();
        self.spring = Spring::default();
        self.screen_flash = 0.0;
    }

    fn rand(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rng.next_f32()
    }

    pub fn burst(&mut self, center: Vec2, color: Color, count: usize, speed: f32) {
        for _ in 0..count {
            if self.particles.len() >= 900 {
                break;
            }
            let ang = self.rand(0.0, std::f32::consts::TAU);
            let sp = self.rand(0.25, 1.0) * speed;
            let life = self.rand(0.35, 0.8);
            let size = self.rand(2.0, 5.0);
            self.particles.push(Particle {
                pos: center,
                vel: vec2(ang.cos() * sp, ang.sin() * sp - speed * 0.35),
                life,
                max_life: life,
                size,
                color,
            });
        }
    }

    pub fn trail(&mut self, x: f32, y_top: f32, y_bottom: f32, w: f32, color: Color) {
        self.trails.push(Trail {
            x,
            y_top,
            y_bottom,
            w,
            color,
            age: 0.0,
        });
    }

    pub fn flash(&mut self, rect: Rect, dur: f32, color: Color) {
        self.flashes.push(Flash {
            rect,
            age: 0.0,
            dur,
            color,
        });
    }

    pub fn callout(&mut self, text: impl Into<String>, pos: Vec2, color: Color, size: Size) {
        // Stack callouts so simultaneous ones don't overlap.
        let live = self.callouts.iter().filter(|c| c.age < 0.5).count() as f32;
        self.callouts.push(Callout {
            text: text.into(),
            pos: pos + vec2(0.0, live * size.px() * 1.25),
            age: 0.0,
            color,
            size,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.vel.y += 900.0 * dt;
            p.vel *= 1.0 - 1.8 * dt;
            p.pos += p.vel * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for t in self.trails.iter_mut() {
            t.age += dt;
        }
        self.trails.retain(|t| t.age < TRAIL_SECS);
        for f in self.flashes.iter_mut() {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < f.dur);
        for c in self.callouts.iter_mut() {
            c.age += dt;
        }
        self.callouts.retain(|c| c.age < CALLOUT_SECS);
        for d in self.dissolves.iter_mut() {
            d.age += dt;
        }
        self.dissolves.retain(|d| d.age < DISSOLVE_SECS);
        self.spring.update(dt);
        self.screen_flash = (self.screen_flash - dt * 2.5).max(0.0);
    }

    /// Effects that live *inside* the board (drawn with board offset).
    /// Schedules a block to fade out and float up after `delay` seconds.
    pub fn dissolve(&mut self, pos: Vec2, color: Color, kind: PieceKind, delay: f32) {
        self.dissolves.push(Dissolve {
            pos,
            color,
            kind,
            age: -delay,
        });
    }

    pub fn draw_board_layer(&self) {
        for d in &self.dissolves {
            let t = (d.age / DISSOLVE_SECS).max(0.0);
            let ease = t * t * (3.0 - 2.0 * t);
            let a = 1.0 - ease;
            let s = 32.0 * (1.0 - 0.35 * ease);
            let off = (32.0 - s) * 0.5;
            blocks::mino(
                d.pos.x + off,
                d.pos.y + off - 26.0 * ease,
                s,
                with_alpha(d.color, a),
                Some(d.kind),
                false,
            );
        }
        for t in &self.trails {
            let k = 1.0 - t.age / TRAIL_SECS;
            // vertical gradient: brighter near the landing spot
            let segs = 8;
            let h = (t.y_bottom - t.y_top) / segs as f32;
            for i in 0..segs {
                let a = k * k * 0.55 * (i as f32 + 1.0) / segs as f32;
                draw_rectangle(
                    t.x,
                    t.y_top + i as f32 * h,
                    t.w,
                    h + 0.5,
                    with_alpha(t.color, a),
                );
            }
        }
        for f in &self.flashes {
            let k = 1.0 - f.age / f.dur;
            draw_rectangle(
                f.rect.x,
                f.rect.y,
                f.rect.w,
                f.rect.h,
                with_alpha(f.color, f.color.a * k),
            );
        }
    }

    /// Effects on top of everything.
    pub fn draw_overlay(&self, stage: &Stage) {
        for p in &self.particles {
            let k = p.life / p.max_life;
            let s = p.size * (0.4 + 0.6 * k);
            draw_rectangle(
                p.pos.x - s * 0.5,
                p.pos.y - s * 0.5,
                s,
                s,
                with_alpha(p.color, k),
            );
        }
        for c in &self.callouts {
            let t = c.age / CALLOUT_SECS;
            let rise = 46.0 * (1.0 - (1.0 - t).powi(3));
            let alpha = if t < 0.08 {
                t / 0.08
            } else {
                (1.0 - (t - 0.6).max(0.0) / 0.4).clamp(0.0, 1.0)
            };
            let y = c.pos.y - rise;
            // pop-in glow ring
            if t < 0.25 {
                let w = text::measure(stage, &c.text, c.size) + 30.0;
                let g = 1.0 - t / 0.25;
                draw_rectangle(
                    c.pos.x - w * 0.5,
                    y - 6.0,
                    w,
                    c.size.px() + 12.0,
                    with_alpha(c.color, 0.18 * g),
                );
            }
            text::label(
                stage,
                &c.text,
                c.pos.x,
                y,
                c.size,
                with_alpha(c.color, alpha),
                Align::Center,
            );
        }
        if self.screen_flash > 0.0 {
            draw_rectangle(
                0.0,
                0.0,
                super::stage::VIRTUAL_W,
                super::stage::VIRTUAL_H,
                with_alpha(self.screen_flash_color, self.screen_flash * 0.18),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_peaks_between_4_and_6_px_and_settles() {
        for dist in [1u32, 10, 20] {
            let mut s = Spring::default();
            s.impact(dist);
            let mut peak: f32 = 0.0;
            for _ in 0..240 {
                s.update(1.0 / 120.0);
                peak = peak.max(s.offset.abs());
            }
            assert!((4.0..=6.0).contains(&peak), "dist {dist}: peak {peak}");
            assert!(s.offset.abs() < 0.05, "settled");
        }
    }
}
