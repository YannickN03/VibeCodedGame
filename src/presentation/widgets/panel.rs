//! Rounded translucent card with a border and an accent stripe.

use macroquad::prelude::*;

use crate::presentation::theme::{Theme, with_alpha};

use super::shapes::{fill_rounded, outline_rounded};

#[derive(Clone, Copy, Debug)]
pub struct RoundedPanel {
    pub rect: Rect,
    pub radius: f32,
    pub accent: Color,
    /// Paint an opaque backdrop first (dialogs that sit above other UI).
    pub opaque: bool,
}

impl RoundedPanel {
    pub fn new(rect: Rect, accent: Color) -> Self {
        RoundedPanel {
            rect,
            radius: 12.0,
            accent,
            opaque: false,
        }
    }

    /// A dialog panel: the content below must not shine through.
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }

    pub fn draw(&self, theme: &Theme) {
        let r = self.rect;
        fill_rounded(
            Rect::new(r.x + 4.0, r.y + 6.0, r.w, r.h),
            self.radius,
            Color::new(0.0, 0.0, 0.0, 0.28),
        );
        if self.opaque {
            fill_rounded(r, self.radius, theme.bg_dark);
        }
        fill_rounded(r, self.radius, theme.panel);
        outline_rounded(r, self.radius, 1.5, theme.panel_border);
        // Accent stripe along the top edge, inset by the corner radius.
        draw_rectangle(
            r.x + self.radius,
            r.y,
            r.w - 2.0 * self.radius,
            2.5,
            with_alpha(self.accent, 0.9),
        );
    }
}
