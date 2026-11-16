//! Reusable overlay for the pause screen and the end-of-session results,
//! including an automatic statistics table, record badge and the standard
//! `Restart [R]` / `Main Menu [Q]` buttons.

use macroquad::prelude::*;

use crate::presentation::digits;
use crate::presentation::stage::{Stage, VIRTUAL_H, VIRTUAL_W};
use crate::presentation::text::{self, Align, Size};
use crate::presentation::theme::{Theme, with_alpha};

use super::button::ActionButton;
use super::panel::RoundedPanel;

/// What a modal button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalAction {
    Resume,
    Restart,
    MainMenu,
}

/// Content of the overlay.
pub enum ModalKind<'a> {
    /// Pause: control legend of the running game.
    Pause {
        game_title: &'a str,
        legend: &'a [(&'static str, &'static str)],
    },
    /// Game over / victory with a statistics table.
    Results {
        title: &'a str,
        won: bool,
        headline: (&'a str, &'a str),
        rows: &'a [(String, String)],
        new_record: bool,
        /// Preformatted personal best, if any.
        best: Option<&'a str>,
    },
}

pub struct ModalOverlay<'a> {
    pub kind: ModalKind<'a>,
    pub accent: Color,
    /// Seconds since the overlay opened (fade-in).
    pub age: f32,
}

const PANEL_W: f32 = 600.0;
const ROW_H: f32 = 26.0;
const BTN_H: f32 = 40.0;

impl ModalOverlay<'_> {
    fn row_count(&self) -> usize {
        match &self.kind {
            ModalKind::Pause { legend, .. } => legend.len(),
            ModalKind::Results { rows, .. } => rows.len(),
        }
    }

    /// Panel height for the current content.
    fn panel_height(&self) -> f32 {
        let head = match &self.kind {
            ModalKind::Pause { .. } => 130.0,
            ModalKind::Results { .. } => 200.0,
        };
        head + self.row_count() as f32 * ROW_H + 24.0 + BTN_H + 28.0
    }

    fn panel_rect(&self) -> Rect {
        let h = self.panel_height();
        Rect::new(
            (VIRTUAL_W - PANEL_W) * 0.5,
            (VIRTUAL_H - h) * 0.5,
            PANEL_W,
            h,
        )
    }

    /// Buttons of this overlay with the action they trigger.
    pub fn buttons(&self) -> Vec<(ModalAction, ActionButton)> {
        let p = self.panel_rect();
        let y = p.bottom() - BTN_H - 24.0;
        let labels: &[(ModalAction, &str)] = match self.kind {
            ModalKind::Pause { .. } => &[
                (ModalAction::Resume, "RESUME [P]"),
                (ModalAction::Restart, "RESTART [R]"),
                (ModalAction::MainMenu, "MAIN MENU [Q]"),
            ],
            ModalKind::Results { .. } => &[
                (ModalAction::Restart, "RESTART [R]"),
                (ModalAction::MainMenu, "MAIN MENU [Q]"),
            ],
        };
        let n = labels.len() as f32;
        let gap = 14.0;
        let w = (PANEL_W - 48.0 - gap * (n - 1.0)) / n;
        labels
            .iter()
            .enumerate()
            .map(|(i, (a, l))| {
                let x = p.x + 24.0 + i as f32 * (w + gap);
                let b = ActionButton::new(Rect::new(x, y, w, BTN_H), *l)
                    .focused(i == 0)
                    .accent(self.accent);
                (*a, b)
            })
            .collect()
    }

    /// The action under a mouse click, if any.
    pub fn click(&self, mouse: Vec2) -> Option<ModalAction> {
        self.buttons()
            .into_iter()
            .find(|(_, b)| b.hovered(mouse))
            .map(|(a, _)| a)
    }

    pub fn draw(&self, stage: &Stage, theme: &Theme, mouse: Vec2, pulse: f32) {
        let fade = (self.age / 0.18).clamp(0.0, 1.0);
        draw_rectangle(
            0.0,
            0.0,
            VIRTUAL_W,
            VIRTUAL_H,
            Color::new(0.0, 0.0, 0.0, 0.62 * fade),
        );
        let p = self.panel_rect();
        RoundedPanel::new(p, self.accent).opaque().draw(theme);
        let cx = p.x + p.w * 0.5;
        let mut y = p.y + 26.0;
        match &self.kind {
            ModalKind::Pause { game_title, legend } => {
                text::label(stage, "PAUSED", cx, y, Size::X3, theme.text, Align::Center);
                y += 56.0;
                text::label(
                    stage,
                    game_title,
                    cx,
                    y,
                    Size::X1,
                    with_alpha(self.accent, 0.95),
                    Align::Center,
                );
                y += 44.0;
                self.draw_rows(stage, theme, p, y, legend.iter().map(|(a, b)| (*a, *b)));
            }
            ModalKind::Results {
                title,
                won,
                headline,
                rows,
                new_record,
                best,
            } => {
                let tc = if *won { theme.accent2 } else { theme.danger };
                text::label(stage, title, cx, y, Size::X3, tc, Align::Center);
                y += 54.0;
                text::label(
                    stage,
                    headline.0,
                    cx,
                    y,
                    Size::X1,
                    theme.text_dim,
                    Align::Center,
                );
                y += 24.0;
                digits::draw_centered(headline.1, cx, y, 40.0, theme.text);
                y += 52.0;
                if *new_record {
                    let a = 0.65 + 0.35 * pulse;
                    text::label(
                        stage,
                        "NEW RECORD!",
                        cx,
                        y,
                        Size::X2,
                        with_alpha(theme.accent2, a),
                        Align::Center,
                    );
                } else if let Some(b) = best {
                    text::label(
                        stage,
                        &format!("BEST  {b}"),
                        cx,
                        y,
                        Size::X1,
                        theme.text_dim,
                        Align::Center,
                    );
                }
                y += 40.0;
                self.draw_rows(
                    stage,
                    theme,
                    p,
                    y,
                    rows.iter().map(|(a, b)| (a.as_str(), b.as_str())),
                );
            }
        }
        for (_, b) in self.buttons() {
            b.draw(stage, theme, mouse, pulse);
        }
    }

    fn draw_rows<'r>(
        &self,
        stage: &Stage,
        theme: &Theme,
        p: Rect,
        mut y: f32,
        rows: impl Iterator<Item = (&'r str, &'r str)>,
    ) {
        for (i, (l, r)) in rows.enumerate() {
            if i % 2 == 0 {
                draw_rectangle(
                    p.x + 20.0,
                    y - 4.0,
                    p.w - 40.0,
                    ROW_H,
                    with_alpha(WHITE, 0.04),
                );
            }
            text::label(
                stage,
                l,
                p.x + 32.0,
                y,
                Size::X1,
                theme.text_dim,
                Align::Left,
            );
            text::label(
                stage,
                r,
                p.right() - 32.0,
                y,
                Size::X1,
                theme.text,
                Align::Right,
            );
            y += ROW_H;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(rows: &[(String, String)]) -> ModalOverlay<'_> {
        ModalOverlay {
            kind: ModalKind::Results {
                title: "GAME OVER",
                won: false,
                headline: ("SCORE", "100"),
                rows,
                new_record: false,
                best: None,
            },
            accent: WHITE,
            age: 1.0,
        }
    }

    #[test]
    fn panel_grows_with_rows_and_stays_on_stage() {
        let rows: Vec<(String, String)> = (0..6).map(|i| (format!("R{i}"), "1".into())).collect();
        let small = results(&[]);
        let big = results(&rows);
        assert!(big.panel_height() > small.panel_height());
        let r = big.panel_rect();
        assert!(r.y >= 0.0 && r.bottom() <= VIRTUAL_H);
    }

    #[test]
    fn buttons_hit_test() {
        let m = results(&[]);
        let btns = m.buttons();
        assert_eq!(btns.len(), 2);
        let c = btns[1].1.rect.center();
        assert_eq!(m.click(c), Some(ModalAction::MainMenu));
        assert_eq!(m.click(vec2(1.0, 1.0)), None);
        let pause = ModalOverlay {
            kind: ModalKind::Pause {
                game_title: "G",
                legend: &[],
            },
            accent: WHITE,
            age: 0.0,
        };
        assert_eq!(pause.buttons().len(), 3);
        assert_eq!(pause.buttons()[0].0, ModalAction::Resume);
    }
}
