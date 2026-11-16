//! Floating text callouts ("QUAD!", "COMBO x3", "+250").

use macroquad::prelude::*;

use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::with_alpha;

/// Lifetime of a callout in seconds.
pub const CALLOUT_SECS: f32 = 1.25;

#[derive(Clone, Debug)]
struct Callout {
    text: String,
    pos: Vec2,
    age: f32,
    color: Color,
    size: Size,
}

#[derive(Default)]
pub struct FloatingCallouts {
    items: Vec<Callout>,
}

impl FloatingCallouts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn add(&mut self, text: impl Into<String>, pos: Vec2, color: Color, size: Size) {
        // Stack callouts so simultaneous ones do not overlap.
        let live = self.items.iter().filter(|c| c.age < 0.5).count() as f32;
        self.items.push(Callout {
            text: text.into(),
            pos: pos + vec2(0.0, live * size.px() * 1.25),
            age: 0.0,
            color,
            size,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for c in self.items.iter_mut() {
            c.age += dt;
        }
        self.items.retain(|c| c.age < CALLOUT_SECS);
    }

    pub fn draw(&self, stage: &Stage) {
        for c in &self.items {
            let t = c.age / CALLOUT_SECS;
            let rise = 46.0 * (1.0 - (1.0 - t).powi(3));
            let alpha = if t < 0.08 {
                t / 0.08
            } else {
                (1.0 - (t - 0.6).max(0.0) / 0.4).clamp(0.0, 1.0)
            };
            let y = c.pos.y - rise;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callouts_stack_and_expire() {
        let mut c = FloatingCallouts::new();
        c.add("A", Vec2::ZERO, WHITE, Size::X2);
        c.add("B", Vec2::ZERO, WHITE, Size::X2);
        assert_eq!(c.len(), 2);
        assert!(c.items[1].pos.y > c.items[0].pos.y, "second one sits lower");
        c.update(CALLOUT_SECS + 0.1);
        assert!(c.is_empty());
    }
}
