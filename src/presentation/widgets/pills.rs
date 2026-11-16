//! Mode picker: a column (or row) of pills, one per option.

use macroquad::prelude::*;

use crate::presentation::stage::Stage;
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::{Theme, with_alpha};

use super::shapes::{fill_rounded, outline_rounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillLayout {
    Vertical,
    Horizontal,
}

/// Pure layout: the rectangle of each of `n` pills inside `area`.
pub fn pill_rects(area: Rect, n: usize, layout: PillLayout, gap: f32) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let gaps = gap * (n - 1) as f32;
    match layout {
        PillLayout::Vertical => {
            let h = ((area.h - gaps) / n as f32).max(0.0);
            (0..n)
                .map(|i| Rect::new(area.x, area.y + i as f32 * (h + gap), area.w, h))
                .collect()
        }
        PillLayout::Horizontal => {
            let w = ((area.w - gaps) / n as f32).max(0.0);
            (0..n)
                .map(|i| Rect::new(area.x + i as f32 * (w + gap), area.y, w, area.h))
                .collect()
        }
    }
}

pub struct ModePillSelector<'a> {
    pub area: Rect,
    pub layout: PillLayout,
    pub labels: &'a [&'a str],
    pub selected: usize,
    pub accent: Color,
    pub gap: f32,
}

impl ModePillSelector<'_> {
    pub fn rects(&self) -> Vec<Rect> {
        pill_rects(self.area, self.labels.len(), self.layout, self.gap)
    }

    /// Index of the pill under `mouse`.
    pub fn hit(&self, mouse: Vec2) -> Option<usize> {
        self.rects().iter().position(|r| r.contains(mouse))
    }

    pub fn draw(&self, stage: &Stage, theme: &Theme, mouse: Vec2, pulse: f32) {
        for (i, (r, label)) in self.rects().iter().zip(self.labels).enumerate() {
            let sel = i == self.selected;
            let hover = r.contains(mouse);
            let fill = if sel {
                with_alpha(self.accent, 0.28)
            } else if hover {
                with_alpha(theme.panel_border, 0.7)
            } else {
                with_alpha(theme.panel_border, 0.35)
            };
            fill_rounded(*r, r.h * 0.5, fill);
            if sel {
                outline_rounded(
                    *r,
                    r.h * 0.5,
                    2.0,
                    with_alpha(self.accent, 0.6 + 0.4 * pulse),
                );
            }
            let col = if sel { theme.text } else { theme.text_dim };
            text::label(
                stage,
                label,
                r.x + r.w * 0.5,
                r.y + (r.h - Size::X1.px()) * 0.5,
                Size::X1,
                col,
                Align::Center,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_pills_tile_the_area() {
        let area = Rect::new(10.0, 20.0, 200.0, 100.0);
        let r = pill_rects(area, 4, PillLayout::Vertical, 4.0);
        assert_eq!(r.len(), 4);
        assert!((r[0].y - 20.0).abs() < 1e-4);
        assert!((r[3].bottom() - 120.0).abs() < 1e-3);
        for w in r.windows(2) {
            assert!((w[1].y - w[0].bottom() - 4.0).abs() < 1e-3);
        }
    }

    #[test]
    fn horizontal_pills_and_empty() {
        let r = pill_rects(
            Rect::new(0.0, 0.0, 100.0, 10.0),
            2,
            PillLayout::Horizontal,
            0.0,
        );
        assert_eq!((r[0].w, r[1].x), (50.0, 50.0));
        assert!(pill_rects(Rect::new(0.0, 0.0, 1.0, 1.0), 0, PillLayout::Vertical, 1.0).is_empty());
    }
}
