//! Crisp text labels: macroquad's default font at integer multiples of the
//! base size only (1×, 2×, 3×), always with a drop shadow.

use macroquad::prelude::*;

use super::stage::Stage;

/// Base glyph height in stage units.
pub const BASE: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Integer text size multiplier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    X1 = 1,
    X2 = 2,
    X3 = 3,
}

impl Size {
    pub fn px(self) -> f32 {
        BASE * self as u8 as f32
    }
}

/// Width of `text` at `size`, in stage units.
pub fn measure(stage: &Stage, text: &str, size: Size) -> f32 {
    let (fs, sc) = stage.font(size.px());
    measure_text(text, None, fs, sc).width
}

/// Draws a label with its top edge at `y`. Returns the drawn width.
pub fn label(
    stage: &Stage,
    text: &str,
    x: f32,
    y: f32,
    size: Size,
    color: Color,
    align: Align,
) -> f32 {
    let (fs, sc) = stage.font(size.px());
    let dims = measure_text(text, None, fs, sc);
    let x = match align {
        Align::Left => x,
        Align::Center => x - dims.width * 0.5,
        Align::Right => x - dims.width,
    };
    let baseline = y + dims.offset_y;
    let shadow = (size as u8) as f32;
    let params = |c: Color| TextParams {
        font: None,
        font_size: fs,
        font_scale: sc,
        font_scale_aspect: 1.0,
        rotation: 0.0,
        color: c,
    };
    draw_text_ex(
        text,
        x.round() + shadow,
        baseline.round() + shadow,
        params(Color::new(0.0, 0.0, 0.0, 0.55 * color.a)),
    );
    draw_text_ex(text, x.round(), baseline.round(), params(color));
    dims.width
}
