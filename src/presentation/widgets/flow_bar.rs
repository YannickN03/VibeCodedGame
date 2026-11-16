//! Glowing flow meter (vertical or horizontal) with a tier badge.

use macroquad::prelude::*;

use crate::core::FlowTier;
use crate::core::flow::FLOW_MAX;
use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::{Theme, lighten, with_alpha};

use super::shapes::{fill_rounded, outline_rounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

pub struct FlowMeterBar {
    pub rect: Rect,
    pub orientation: Orientation,
    /// `0..=100`
    pub flow: f32,
    pub pulse: f32,
}

impl FlowMeterBar {
    /// Fraction `0..=1` of the bar that is filled.
    pub fn fraction(&self) -> f32 {
        (self.flow / FLOW_MAX).clamp(0.0, 1.0)
    }

    pub fn draw(&self, stage: &Stage, theme: &Theme) {
        let tier = FlowTier::from_flow(self.flow);
        let col = theme.tiers[tier.index()];
        let r = self.rect;
        let rad = r.w.min(r.h) * 0.5;
        fill_rounded(r, rad, with_alpha(theme.well, 0.9));
        let f = self.fraction();
        if f > 0.0 {
            let fill = match self.orientation {
                Orientation::Horizontal => Rect::new(r.x, r.y, (r.w * f).max(rad * 2.0), r.h),
                Orientation::Vertical => {
                    let h = (r.h * f).max(rad * 2.0);
                    Rect::new(r.x, r.bottom() - h, r.w, h)
                }
            };
            let glow = 0.15 + 0.2 * self.pulse;
            fill_rounded(
                Rect::new(fill.x - 3.0, fill.y - 3.0, fill.w + 6.0, fill.h + 6.0),
                rad + 3.0,
                with_alpha(col, glow),
            );
            fill_rounded(fill, rad, col);
            fill_rounded(
                Rect::new(
                    fill.x + 2.0,
                    fill.y + 2.0,
                    (fill.w - 4.0).max(1.0),
                    (fill.h * 0.35).max(2.0),
                ),
                rad * 0.5,
                with_alpha(lighten(col, 0.6), 0.35),
            );
        }
        outline_rounded(r, rad, 1.5, with_alpha(col, 0.5));
        let badge = format!("{} {}x", tier.name(), tier.multiplier());
        let (bx, by, align) = match self.orientation {
            Orientation::Horizontal => (r.x, r.y - Size::X1.px() - 6.0, Align::Left),
            Orientation::Vertical => (r.x + r.w * 0.5, r.bottom() + 8.0, Align::Center),
        };
        text::label(stage, &badge, bx, by, Size::X1, col, align);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_clamped() {
        let mut b = FlowMeterBar {
            rect: Rect::new(0.0, 0.0, 10.0, 100.0),
            orientation: Orientation::Vertical,
            flow: 150.0,
            pulse: 0.0,
        };
        assert_eq!(b.fraction(), 1.0);
        b.flow = -5.0;
        assert_eq!(b.fraction(), 0.0);
        b.flow = 50.0;
        assert!((b.fraction() - 0.5).abs() < 1e-6);
    }
}
