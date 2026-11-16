//! Board-space "juice" of Blocks: hard-drop light trails, board impact
//! spring, lock flashes, line-clear flashes and the Zen dissolve.
//!
//! Particles, callouts and screen flashes are shared arcade systems driven
//! by `FrameEvents`; only effects that live inside the board stay here.
//! Simulation is pure math (testable); drawing uses macroquad.

use macroquad::prelude::*;

use crate::core::PieceKind;
use crate::presentation::theme::with_alpha;

use super::mino;

/// Hard-drop trail fade time.
pub const TRAIL_SECS: f32 = 0.18;
/// Lock flash duration.
pub const LOCK_FLASH_SECS: f32 = 0.08;
/// Line-clear row flash duration.
pub const ROW_FLASH_SECS: f32 = 0.22;

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

/// All live effects.
pub struct Fx {
    trails: Vec<Trail>,
    flashes: Vec<Flash>,
    dissolves: Vec<Dissolve>,
    pub spring: Spring,
}

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        Fx {
            trails: Vec::new(),
            flashes: Vec::new(),
            dissolves: Vec::new(),
            spring: Spring::default(),
        }
    }

    pub fn clear(&mut self) {
        self.trails.clear();
        self.flashes.clear();
        self.dissolves.clear();
        self.spring = Spring::default();
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

    pub fn update(&mut self, dt: f32) {
        for t in self.trails.iter_mut() {
            t.age += dt;
        }
        self.trails.retain(|t| t.age < TRAIL_SECS);
        for f in self.flashes.iter_mut() {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < f.dur);
        for d in self.dissolves.iter_mut() {
            d.age += dt;
        }
        self.dissolves.retain(|d| d.age < DISSOLVE_SECS);
        self.spring.update(dt);
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
            mino::mino(
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
