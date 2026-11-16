//! Button with mouse hover/click and a keyboard-focus highlight.

use macroquad::prelude::*;

use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::{Theme, lighten, mix, with_alpha};

use super::shapes::{fill_rounded, outline_rounded};

#[derive(Clone, Debug)]
pub struct ActionButton {
    pub rect: Rect,
    pub label: String,
    /// Keyboard focus (e.g. the default action of a dialog).
    pub focused: bool,
    pub accent: Option<Color>,
}

impl ActionButton {
    pub fn new(rect: Rect, label: impl Into<String>) -> Self {
        ActionButton {
            rect,
            label: label.into(),
            focused: false,
            accent: None,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn accent(mut self, c: Color) -> Self {
        self.accent = Some(c);
        self
    }

    pub fn hovered(&self, mouse: Vec2) -> bool {
        self.rect.contains(mouse)
    }

    /// `true` when the left button was pressed over the button this frame.
    pub fn clicked(&self, mouse: Vec2, mouse_pressed: bool) -> bool {
        mouse_pressed && self.hovered(mouse)
    }

    pub fn draw(&self, stage: &Stage, theme: &Theme, mouse: Vec2, pulse: f32) {
        let hover = self.hovered(mouse);
        let accent = self.accent.unwrap_or(theme.accent2);
        let r = self.rect;
        let fill = if hover {
            with_alpha(lighten(accent, 0.1), 0.38)
        } else if self.focused {
            with_alpha(accent, 0.24)
        } else {
            with_alpha(theme.panel_border, 0.55)
        };
        fill_rounded(r, 9.0, fill);
        let border = if self.focused {
            with_alpha(lighten(accent, 0.3), 0.55 + 0.45 * pulse)
        } else if hover {
            accent
        } else {
            theme.panel_border
        };
        outline_rounded(r, 9.0, if self.focused { 2.5 } else { 1.5 }, border);
        let col = if hover || self.focused {
            theme.text
        } else {
            mix(theme.text, theme.text_dim, 0.4)
        };
        text::label(
            stage,
            &self.label,
            r.x + r.w * 0.5,
            r.y + (r.h - Size::X1.px()) * 0.5,
            Size::X1,
            col,
            Align::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_needs_hover_and_press() {
        let b = ActionButton::new(Rect::new(10.0, 10.0, 100.0, 30.0), "OK");
        assert!(b.clicked(vec2(20.0, 20.0), true));
        assert!(!b.clicked(vec2(20.0, 20.0), false));
        assert!(!b.clicked(vec2(500.0, 20.0), true));
    }
}
