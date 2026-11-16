//! Shared particle system; titles request bursts through `FrameEvents`.

use macroquad::prelude::*;

use crate::core::rng::SplitMix64;
use crate::presentation::theme::with_alpha;

/// Upper bound of live particles.
pub const MAX_PARTICLES: usize = 900;

#[derive(Clone, Debug)]
struct Particle {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max_life: f32,
    size: f32,
    color: Color,
}

pub struct ParticleSystem {
    rng: SplitMix64,
    particles: Vec<Particle>,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    pub fn new() -> Self {
        ParticleSystem {
            rng: SplitMix64::new(0xF00D),
            particles: Vec::with_capacity(512),
        }
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    fn rand(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rng.next_f32()
    }

    pub fn burst(&mut self, center: Vec2, color: Color, count: usize, speed: f32) {
        for _ in 0..count {
            if self.particles.len() >= MAX_PARTICLES {
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

    pub fn update(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.vel.y += 900.0 * dt;
            p.vel *= 1.0 - 1.8 * dt;
            p.pos += p.vel * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    pub fn draw(&self) {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_spawn_expire_and_are_capped() {
        let mut ps = ParticleSystem::new();
        ps.burst(Vec2::ZERO, WHITE, 100, 200.0);
        assert_eq!(ps.len(), 100);
        for _ in 0..200 {
            ps.update(0.02);
        }
        assert!(ps.is_empty(), "all particles die within a few seconds");
        for _ in 0..50 {
            ps.burst(Vec2::ZERO, WHITE, 100, 200.0);
        }
        assert_eq!(ps.len(), MAX_PARTICLES);
        ps.clear();
        assert!(ps.is_empty());
    }
}
