//! Colour themes (`T` toggles).

use macroquad::color::Color;

use crate::core::PieceKind;

/// `0xRRGGBB` -> opaque colour (const-friendly).
pub const fn hex(rgb: u32) -> Color {
    Color::new(
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
        1.0,
    )
}

/// Same colour with a different alpha.
pub fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

/// Linear blend.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub fn lighten(c: Color, t: f32) -> Color {
    mix(c, Color::new(1.0, 1.0, 1.0, c.a), t)
}

pub fn darken(c: Color, t: f32) -> Color {
    mix(c, Color::new(0.0, 0.0, 0.0, c.a), t)
}

/// A complete palette.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg_dark: Color,
    pub bg_light: Color,
    pub grid: Color,
    pub panel: Color,
    pub panel_border: Color,
    pub well: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub accent2: Color,
    pub danger: Color,
    pub pieces: [Color; 7],
    /// Flow meter colours per tier.
    pub tiers: [Color; 4],
}

impl Theme {
    pub fn piece(&self, kind: PieceKind) -> Color {
        self.pieces[kind.index()]
    }
}

pub fn themes() -> [Theme; 2] {
    [
        Theme {
            name: "MIDNIGHT NEON",
            bg_dark: hex(0x0B0E14),
            bg_light: hex(0x131926),
            grid: hex(0x5B6CFF),
            panel: Color::new(0.07, 0.085, 0.13, 0.82),
            panel_border: hex(0x2A3350),
            well: Color::new(0.03, 0.04, 0.065, 0.92),
            text: hex(0xE8ECFF),
            text_dim: hex(0x7C86A8),
            accent: hex(0x7C5CFF),
            accent2: hex(0x3EE6F5),
            danger: hex(0xFF3B5C),
            pieces: [
                hex(0x3EE6F5), // I
                hex(0xFFD84A), // O
                hex(0xB46CFF), // T
                hex(0x5CF08A), // S
                hex(0xFF4D7A), // Z
                hex(0x4C7DFF), // J
                hex(0xFF9A3C), // L
            ],
            tiers: [hex(0x4C7DFF), hex(0x3EE6F5), hex(0xB46CFF), hex(0xFF4DA6)],
        },
        Theme {
            name: "NORDIC SLATE",
            bg_dark: hex(0x0E1218),
            bg_light: hex(0x1B2230),
            grid: hex(0x88C0D0),
            panel: Color::new(0.11, 0.13, 0.17, 0.85),
            panel_border: hex(0x3B4252),
            well: Color::new(0.06, 0.07, 0.09, 0.92),
            text: hex(0xECEFF4),
            text_dim: hex(0x8892A6),
            accent: hex(0x88C0D0),
            accent2: hex(0xA3BE8C),
            danger: hex(0xBF616A),
            pieces: [
                hex(0x88C0D0),
                hex(0xEBCB8B),
                hex(0xB48EAD),
                hex(0xA3BE8C),
                hex(0xBF616A),
                hex(0x5E81AC),
                hex(0xD08770),
            ],
            tiers: [hex(0x5E81AC), hex(0x88C0D0), hex(0xA3BE8C), hex(0xEBCB8B)],
        },
    ]
}
