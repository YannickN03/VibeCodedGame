//! GLSL background: dark gradient (#0B0E14 → #131926), geometric grid and
//! vignette, pulsing at the BPM of the current flow tier.

use macroquad::prelude::*;

use super::stage::{VIRTUAL_H, VIRTUAL_W};
use super::theme::{Theme, mix};

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec4 color;
varying highp vec2 vpos;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    vpos = position.xy;
}
"#;

const FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying highp vec2 vpos;
uniform vec2 Resolution;
uniform float Time;
uniform float Pulse;
uniform float Intensity;
uniform vec3 ColDark;
uniform vec3 ColLight;
uniform vec3 Accent;

void main() {
    vec2 uv = vpos / Resolution;
    // vertical gradient with a slow breathing drift
    float g = clamp(uv.y * 0.85 + 0.08 * sin(Time * 0.21 + uv.x * 2.0), 0.0, 1.0);
    vec3 col = mix(ColLight, ColDark, g);

    // perspective-ish geometric grid, scrolling gently
    vec2 p = vpos / 48.0 + vec2(0.0, Time * 0.12);
    vec2 f = abs(fract(p) - 0.5);
    float line = smoothstep(0.455, 0.5, max(f.x, f.y));
    // diagonal accent lattice
    float d = abs(fract((vpos.x + vpos.y) / 192.0 + Time * 0.03) - 0.5);
    float diag = smoothstep(0.485, 0.5, d);

    float beat = 0.35 + 0.65 * Pulse;
    float glow = (0.035 + 0.06 * Intensity) * beat;
    col += Accent * line * glow;
    col += Accent * diag * 0.018 * (0.5 + Intensity);

    // central radial bloom following the beat
    float r = length((uv - vec2(0.5, 0.5)) * vec2(1.6, 1.0));
    col += Accent * (0.05 + 0.07 * Intensity) * Pulse * exp(-r * 3.0);

    // vignette
    float v = smoothstep(0.35, 1.05, length((uv - 0.5) * vec2(1.25, 1.0)) * 1.15);
    col *= 1.0 - v * 0.72;

    gl_FragColor = vec4(col, 1.0);
}
"#;

/// Background renderer with a CPU fallback when the shader can't compile.
pub struct Background {
    material: Option<Material>,
    beat_phase: f32,
    time: f32,
}

impl Background {
    pub fn new() -> Self {
        let material = load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("Resolution", UniformType::Float2),
                    UniformDesc::new("Time", UniformType::Float1),
                    UniformDesc::new("Pulse", UniformType::Float1),
                    UniformDesc::new("Intensity", UniformType::Float1),
                    UniformDesc::new("ColDark", UniformType::Float3),
                    UniformDesc::new("ColLight", UniformType::Float3),
                    UniformDesc::new("Accent", UniformType::Float3),
                ],
                ..Default::default()
            },
        )
        .ok();
        Background {
            material,
            beat_phase: 0.0,
            time: 0.0,
        }
    }

    pub fn has_shader(&self) -> bool {
        self.material.is_some()
    }

    /// Advances the beat clock. Phase is integrated so tempo changes are smooth.
    pub fn update(&mut self, dt: f32, bpm: f32) {
        self.time += dt;
        self.beat_phase = (self.beat_phase + dt * bpm / 60.0).fract();
    }

    /// Beat envelope `0..1` (sharp attack, exponential release).
    pub fn pulse(&self) -> f32 {
        (-self.beat_phase * 5.0).exp()
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    /// Draws the full-stage background. `intensity` is `0..1` (flow tier).
    pub fn draw(&self, theme: &Theme, intensity: f32) {
        let pulse = self.pulse();
        match &self.material {
            Some(m) => {
                let v3 = |c: Color| vec3(c.r, c.g, c.b);
                m.set_uniform("Resolution", vec2(VIRTUAL_W, VIRTUAL_H));
                m.set_uniform("Time", self.time);
                m.set_uniform("Pulse", pulse);
                m.set_uniform("Intensity", intensity);
                m.set_uniform("ColDark", v3(theme.bg_dark));
                m.set_uniform("ColLight", v3(theme.bg_light));
                m.set_uniform("Accent", v3(theme.grid));
                gl_use_material(m);
                draw_rectangle(0.0, 0.0, VIRTUAL_W, VIRTUAL_H, WHITE);
                gl_use_default_material();
            }
            None => {
                // Fallback: banded gradient + simple grid.
                let bands = 24;
                for i in 0..bands {
                    let t = i as f32 / (bands - 1) as f32;
                    let h = VIRTUAL_H / bands as f32;
                    draw_rectangle(
                        0.0,
                        i as f32 * h,
                        VIRTUAL_W,
                        h + 1.0,
                        mix(theme.bg_light, theme.bg_dark, t),
                    );
                }
                let a = 0.04 + 0.05 * pulse * (0.4 + intensity);
                let c = Color::new(theme.grid.r, theme.grid.g, theme.grid.b, a);
                let mut x = 0.0;
                while x < VIRTUAL_W {
                    draw_line(x, 0.0, x, VIRTUAL_H, 1.0, c);
                    x += 48.0;
                }
                let mut y = (self.time * 0.12 * 48.0) % 48.0;
                while y < VIRTUAL_H {
                    draw_line(0.0, y, VIRTUAL_W, y, 1.0, c);
                    y += 48.0;
                }
            }
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Self::new()
    }
}
