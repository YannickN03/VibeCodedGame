//! Settings modal of the arcade menu (DAS/ARR for Blocks, boost for Snake,
//! volume, sound, theme, colour-blind mode). Logic is pure and testable.

use macroquad::prelude::*;

use crate::infrastructure::save::Settings;

use super::stage::{Stage, VIRTUAL_H, VIRTUAL_W};
use super::text::{self, Align, Size};
use super::theme::{Theme, with_alpha};
use super::widgets::shapes::fill_rounded;
use super::widgets::{ActionButton, RoundedPanel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingRow {
    Das,
    Arr,
    Volume,
    Sound,
    Theme,
    Colorblind,
    SnakeBoost,
}

pub const ROWS: [SettingRow; 7] = [
    SettingRow::Das,
    SettingRow::Arr,
    SettingRow::Volume,
    SettingRow::Sound,
    SettingRow::Theme,
    SettingRow::Colorblind,
    SettingRow::SnakeBoost,
];

impl SettingRow {
    pub fn label(self) -> &'static str {
        match self {
            SettingRow::Das => "DAS (BLOCKS)",
            SettingRow::Arr => "ARR (BLOCKS)",
            SettingRow::Volume => "MASTER VOLUME",
            SettingRow::Sound => "SOUND",
            SettingRow::Theme => "THEME",
            SettingRow::Colorblind => "COLOUR-BLIND MODE",
            SettingRow::SnakeBoost => "SNAKE SPRINT BOOST",
        }
    }

    /// `true` for rows with `-` / `+` buttons (the others toggle or cycle).
    pub fn is_numeric(self) -> bool {
        matches!(self, SettingRow::Das | SettingRow::Arr | SettingRow::Volume)
    }

    pub fn value_text(self, s: &Settings, theme_name: &str) -> String {
        let on = |b: bool| if b { "ON" } else { "OFF" }.to_string();
        match self {
            SettingRow::Das => format!("{} MS", s.das_ms),
            SettingRow::Arr => format!("{} MS", s.arr_ms),
            SettingRow::Volume => format!("{}%", s.volume),
            SettingRow::Sound => on(!s.muted),
            SettingRow::Theme => theme_name.to_string(),
            SettingRow::Colorblind => on(s.colorblind),
            SettingRow::SnakeBoost => on(s.snake_boost),
        }
    }

    /// Changes the value by one step in direction `dir` (`-1` / `+1`).
    /// Returns `true` if something changed.
    pub fn adjust(self, s: &mut Settings, dir: i32, theme_count: usize) -> bool {
        let before = s.clone();
        let step =
            |v: u32, d: i32, lo: u32, hi: u32| (v as i32 + d).clamp(lo as i32, hi as i32) as u32;
        match self {
            SettingRow::Das => {
                s.das_ms = step(
                    s.das_ms,
                    dir * 10,
                    Settings::DAS_RANGE.0,
                    Settings::DAS_RANGE.1,
                )
            }
            SettingRow::Arr => {
                s.arr_ms = step(
                    s.arr_ms,
                    dir * 5,
                    Settings::ARR_RANGE.0,
                    Settings::ARR_RANGE.1,
                )
            }
            SettingRow::Volume => s.volume = step(s.volume, dir * 10, 0, 100),
            SettingRow::Sound => s.muted = !s.muted,
            SettingRow::Theme => {
                let n = theme_count.max(1) as i32;
                s.theme = (s.theme as i32 + dir).rem_euclid(n) as usize;
            }
            SettingRow::Colorblind => s.colorblind = !s.colorblind,
            SettingRow::SnakeBoost => s.snake_boost = !s.snake_boost,
        }
        *s != before
    }
}

const PANEL: Rect = Rect {
    x: 320.0,
    y: 96.0,
    w: 640.0,
    h: 528.0,
};
const ROW_H: f32 = 48.0;
const ROW_TOP: f32 = PANEL.y + 92.0;

/// `(minus, plus, value area)` of settings row `i`.
pub fn row_rects(i: usize) -> (Rect, Rect, Rect) {
    let y = ROW_TOP + i as f32 * ROW_H;
    let right = PANEL.right() - 28.0;
    let plus = Rect::new(right - 36.0, y, 36.0, 34.0);
    let minus = Rect::new(right - 36.0 - 150.0 - 8.0 - 36.0, y, 36.0, 34.0);
    let value = Rect::new(minus.right() + 4.0, y, 150.0, 34.0);
    (minus, plus, value)
}

fn close_button() -> ActionButton {
    ActionButton::new(
        Rect::new(
            PANEL.x + PANEL.w * 0.5 - 90.0,
            PANEL.bottom() - 62.0,
            180.0,
            40.0,
        ),
        "CLOSE [TAB]",
    )
    .focused(true)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SettingsInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub close: bool,
    pub mouse: Vec2,
    pub click: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsOutcome {
    pub changed: bool,
    pub close: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsState {
    pub row: usize,
}

impl SettingsState {
    pub fn update(
        &mut self,
        i: &SettingsInput,
        s: &mut Settings,
        theme_count: usize,
    ) -> SettingsOutcome {
        let mut out = SettingsOutcome::default();
        let n = ROWS.len();
        if i.down {
            self.row = (self.row + 1) % n;
        }
        if i.up {
            self.row = (self.row + n - 1) % n;
        }
        let row = ROWS[self.row];
        if i.left {
            out.changed |= row.adjust(s, -1, theme_count);
        }
        if i.right {
            out.changed |= row.adjust(s, 1, theme_count);
        }
        if i.close {
            out.close = true;
        }
        if i.click {
            if close_button().hovered(i.mouse) || !PANEL.contains(i.mouse) {
                out.close = true;
            }
            for (idx, row) in ROWS.iter().enumerate() {
                let (minus, plus, value) = row_rects(idx);
                let dir = if row.is_numeric() {
                    if minus.contains(i.mouse) {
                        -1
                    } else if plus.contains(i.mouse) {
                        1
                    } else {
                        continue;
                    }
                } else if value.contains(i.mouse)
                    || minus.contains(i.mouse)
                    || plus.contains(i.mouse)
                {
                    1
                } else {
                    continue;
                };
                self.row = idx;
                out.changed |= row.adjust(s, dir, theme_count);
            }
        }
        out
    }
}

pub fn draw(stage: &Stage, theme: &Theme, state: &SettingsState, s: &Settings, pulse: f32) {
    let mouse = stage.mouse();
    draw_rectangle(
        0.0,
        0.0,
        VIRTUAL_W,
        VIRTUAL_H,
        Color::new(0.0, 0.0, 0.0, 0.66),
    );
    RoundedPanel::new(PANEL, theme.accent2).opaque().draw(theme);
    text::label(
        stage,
        "SETTINGS",
        PANEL.x + PANEL.w * 0.5,
        PANEL.y + 24.0,
        Size::X3,
        theme.text,
        Align::Center,
    );
    text::label(
        stage,
        "UP/DOWN select   LEFT/RIGHT change   changes are saved at once",
        PANEL.x + PANEL.w * 0.5,
        PANEL.y + 68.0,
        Size::X1,
        theme.text_dim,
        Align::Center,
    );
    for (i, row) in ROWS.iter().enumerate() {
        let y = ROW_TOP + i as f32 * ROW_H;
        let sel = i == state.row;
        if sel {
            fill_rounded(
                Rect::new(PANEL.x + 16.0, y - 6.0, PANEL.w - 32.0, 44.0),
                10.0,
                with_alpha(theme.accent2, 0.10 + 0.08 * pulse),
            );
        }
        text::label(
            stage,
            row.label(),
            PANEL.x + 32.0,
            y + 8.0,
            Size::X1,
            if sel { theme.text } else { theme.text_dim },
            Align::Left,
        );
        let (minus, plus, value) = row_rects(i);
        if row.is_numeric() {
            ActionButton::new(minus, "-").draw(stage, theme, mouse, pulse);
            ActionButton::new(plus, "+").draw(stage, theme, mouse, pulse);
        } else {
            ActionButton::new(Rect::new(minus.x, y, plus.right() - minus.x, 34.0), "")
                .draw(stage, theme, mouse, pulse);
        }
        text::label(
            stage,
            &row.value_text(s, theme.name),
            value.x + value.w * 0.5,
            y + 8.0,
            Size::X1,
            theme.accent2,
            Align::Center,
        );
    }
    close_button().draw(stage, theme, mouse, pulse);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_rows_clamp_to_their_ranges() {
        let mut s = Settings::default();
        for _ in 0..100 {
            SettingRow::Das.adjust(&mut s, -1, 2);
        }
        assert_eq!(s.das_ms, Settings::DAS_RANGE.0);
        for _ in 0..100 {
            SettingRow::Das.adjust(&mut s, 1, 2);
            SettingRow::Arr.adjust(&mut s, 1, 2);
            SettingRow::Volume.adjust(&mut s, 1, 2);
        }
        assert_eq!(s.das_ms, Settings::DAS_RANGE.1);
        assert_eq!(s.arr_ms, Settings::ARR_RANGE.1);
        assert_eq!(s.volume, 100);
        assert!(
            !SettingRow::Volume.adjust(&mut s, 1, 2),
            "no change at the limit"
        );
    }

    #[test]
    fn toggles_and_theme_cycle() {
        let mut s = Settings::default();
        assert!(s.snake_boost);
        assert!(SettingRow::SnakeBoost.adjust(&mut s, 1, 2));
        assert!(!s.snake_boost);
        assert!(SettingRow::Sound.adjust(&mut s, 1, 2));
        assert!(s.muted);
        SettingRow::Theme.adjust(&mut s, -1, 2);
        assert_eq!(s.theme, 1, "wraps backwards");
        SettingRow::Theme.adjust(&mut s, 1, 2);
        assert_eq!(s.theme, 0);
    }

    #[test]
    fn keyboard_and_mouse_changes_are_reported() {
        let mut st = SettingsState::default();
        let mut s = Settings::default();
        let o = st.update(
            &SettingsInput {
                right: true,
                ..Default::default()
            },
            &mut s,
            2,
        );
        assert!(o.changed && !o.close);
        assert_eq!(s.das_ms, 140);
        st.update(
            &SettingsInput {
                down: true,
                ..Default::default()
            },
            &mut s,
            2,
        );
        assert_eq!(st.row, 1);
        let (_, plus, _) = row_rects(2);
        let o = st.update(
            &SettingsInput {
                mouse: plus.center(),
                click: true,
                ..Default::default()
            },
            &mut s,
            2,
        );
        assert!(o.changed);
        assert_eq!((st.row, s.volume), (2, 80));
        let o = st.update(
            &SettingsInput {
                close: true,
                ..Default::default()
            },
            &mut s,
            2,
        );
        assert!(o.close);
        let o = st.update(
            &SettingsInput {
                mouse: vec2(2.0, 2.0),
                click: true,
                ..Default::default()
            },
            &mut s,
            2,
        );
        assert!(o.close, "click outside the panel closes it");
    }

    #[test]
    fn all_rows_fit_into_the_panel() {
        for i in 0..ROWS.len() {
            let (m, p, v) = row_rects(i);
            for r in [m, p, v] {
                assert!(r.x >= PANEL.x && r.right() <= PANEL.right());
                assert!(r.bottom() < close_button().rect.y);
            }
        }
    }
}
