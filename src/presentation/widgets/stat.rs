//! Label with a big vector-digit value (`0-9 : . x % # -`).

use macroquad::prelude::*;

use crate::presentation::digits;
use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::Theme;

pub struct StatBox<'a> {
    pub label: &'a str,
    pub value: &'a str,
    /// Top-left corner.
    pub pos: Vec2,
    pub width: f32,
    /// Digit height.
    pub digit_h: f32,
    pub color: Option<Color>,
}

impl StatBox<'_> {
    /// Height of the whole box.
    pub fn height(&self) -> f32 {
        Size::X1.px() + 6.0 + self.digit_h
    }

    /// Draws the label on the left and the value right-aligned beneath it.
    pub fn draw(&self, stage: &Stage, theme: &Theme) {
        text::label(
            stage,
            self.label,
            self.pos.x,
            self.pos.y,
            Size::X1,
            theme.text_dim,
            Align::Left,
        );
        digits::draw_right(
            self.value,
            self.pos.x + self.width,
            self.pos.y + Size::X1.px() + 6.0,
            self.digit_h,
            self.color.unwrap_or(theme.text),
        );
    }
}
