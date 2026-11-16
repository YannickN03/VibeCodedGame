//! Data-driven arcade main menu: one card per registered game, laid out
//! automatically for any number of games. Layout and input handling are pure
//! (testable); only `draw_*` touches macroquad.

use macroquad::prelude::*;

use crate::arcade::{GameRegistry, Selection};
use crate::core::date::Date;
use crate::infrastructure::save::{SaveData, daily_key, score_key};

use super::background::Background;
use super::digits;
use super::stage::{Stage, VIRTUAL_W};
use super::text::{self, Align, Size};
use super::theme::{Theme, with_alpha};
use super::widgets::{ActionButton, ModePillSelector, PillLayout, RoundedPanel};

/// Area available for the cards.
pub const CONTENT: Rect = Rect {
    x: 40.0,
    y: 112.0,
    w: 1200.0,
    h: 508.0,
};
const GAP: f32 = 24.0;
/// Cards per page when there are more than three games (2x2 grid).
pub const PAGE_SIZE: usize = 4;
/// Settings button in the global bar.
pub const SETTINGS_BTN: Rect = Rect {
    x: 1080.0,
    y: 650.0,
    w: 160.0,
    h: 36.0,
};
const ARROW_W: f32 = 36.0;

/// Cards of the current page plus paging info.
#[derive(Clone, Debug, PartialEq)]
pub struct CardLayout {
    /// `(game index, card rect)`
    pub cards: Vec<(usize, Rect)>,
    pub pages: usize,
    pub page: usize,
    pub compact: bool,
}

/// Pure card layout for `n` games. Up to three games sit side by side,
/// more are shown as pages of a 2x2 grid.
pub fn layout(n: usize, page: usize) -> CardLayout {
    if n == 0 {
        return CardLayout {
            cards: Vec::new(),
            pages: 0,
            page: 0,
            compact: false,
        };
    }
    if n <= 3 {
        let w = ((CONTENT.w - GAP * (n - 1) as f32) / n as f32).min(580.0);
        let total = w * n as f32 + GAP * (n - 1) as f32;
        let x0 = CONTENT.x + (CONTENT.w - total) * 0.5;
        let cards = (0..n)
            .map(|i| {
                (
                    i,
                    Rect::new(x0 + i as f32 * (w + GAP), CONTENT.y, w, CONTENT.h),
                )
            })
            .collect();
        return CardLayout {
            cards,
            pages: 1,
            page: 0,
            compact: false,
        };
    }
    let pages = n.div_ceil(PAGE_SIZE);
    let page = page.min(pages - 1);
    let inner_w = CONTENT.w - 2.0 * (ARROW_W + 8.0);
    let w = (inner_w - GAP) / 2.0;
    let h = (CONTENT.h - GAP) / 2.0;
    let x0 = CONTENT.x + ARROW_W + 8.0;
    let cards = (page * PAGE_SIZE..n.min((page + 1) * PAGE_SIZE))
        .enumerate()
        .map(|(slot, i)| {
            let (col, row) = ((slot % 2) as f32, (slot / 2) as f32);
            (
                i,
                Rect::new(x0 + col * (w + GAP), CONTENT.y + row * (h + GAP), w, h),
            )
        })
        .collect();
    CardLayout {
        cards,
        pages,
        page,
        compact: true,
    }
}

/// Previous / next page arrow rectangles (only with more than one page).
pub fn arrow_rects() -> (Rect, Rect) {
    let y = CONTENT.y + CONTENT.h * 0.5 - 40.0;
    (
        Rect::new(CONTENT.x, y, ARROW_W, 80.0),
        Rect::new(CONTENT.right() - ARROW_W, y, ARROW_W, 80.0),
    )
}

/// Interior rectangles of one card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardParts {
    pub title_y: f32,
    pub tagline_y: f32,
    pub preview: Rect,
    pub pills: Rect,
    pub desc_y: f32,
    pub best: Rect,
    pub play: Rect,
}

pub fn card_parts(r: Rect, n_modes: usize, compact: bool) -> CardParts {
    let pad = 16.0;
    let bottom = r.bottom() - pad;
    let play = Rect::new(r.x + pad, bottom - 38.0, (r.w - 2.0 * pad) * 0.46, 38.0);
    let best = Rect::new(
        play.right() + 12.0,
        play.y,
        r.right() - pad - play.right() - 12.0,
        38.0,
    );
    let title_y = r.y + 14.0;
    let tagline_y = r.y + 14.0 + if compact { 26.0 } else { 38.0 };
    let top = tagline_y + 28.0;
    if compact {
        let preview = Rect::new(
            r.x + pad,
            top,
            (r.w - 2.0 * pad) * 0.42,
            play.y - 10.0 - top,
        );
        let pills = Rect::new(
            preview.right() + 14.0,
            top,
            r.right() - pad - preview.right() - 14.0,
            play.y - 10.0 - top,
        );
        return CardParts {
            title_y,
            tagline_y,
            preview,
            pills,
            desc_y: play.y - 8.0,
            best,
            play,
        };
    }
    let desc_h = 40.0;
    let desc_y = play.y - 12.0 - desc_h;
    let preview = Rect::new(r.x + pad, top, r.w - 2.0 * pad, (r.h * 0.36).min(210.0));
    let pills_top = preview.bottom() + 12.0;
    let avail = (desc_y - 8.0 - pills_top).max(0.0);
    let n = n_modes.max(1) as f32;
    let pill_h = ((avail - 6.0 * (n - 1.0)) / n).clamp(14.0, 28.0);
    let pills = Rect::new(
        r.x + pad,
        pills_top,
        r.w - 2.0 * pad,
        pill_h * n + 6.0 * (n - 1.0),
    );
    CardParts {
        title_y,
        tagline_y,
        preview,
        pills,
        desc_y,
        best,
        play,
    }
}

/// What the player did in the menu this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Play(Selection),
    OpenSettings,
}

/// Input snapshot of one menu frame (pure data).
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuInput {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub confirm: bool,
    pub settings: bool,
    /// Zero-based game index of a pressed digit key.
    pub digit: Option<usize>,
    pub mouse: Vec2,
    pub click: bool,
}

#[derive(Clone, Debug, Default)]
pub struct MenuState {
    pub focus: usize,
    /// Selected mode per game.
    pub modes: Vec<usize>,
    pub page: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuOutcome {
    pub action: Option<MenuAction>,
    /// Focus / mode / page changed (for the "tick" sound).
    pub moved: bool,
}

impl MenuState {
    pub fn new(mode_counts: &[usize]) -> Self {
        MenuState {
            focus: 0,
            modes: vec![0; mode_counts.len()],
            page: 0,
        }
    }

    pub fn selection(&self) -> Selection {
        Selection {
            game: self.focus,
            mode: self.modes.get(self.focus).copied().unwrap_or(0),
        }
    }

    fn focus_game(&mut self, g: usize, n: usize) {
        self.focus = g.min(n.saturating_sub(1));
        if n > 3 {
            self.page = self.focus / PAGE_SIZE;
        }
    }

    pub fn update(&mut self, input: &MenuInput, mode_counts: &[usize]) -> MenuOutcome {
        let n = mode_counts.len();
        let mut out = MenuOutcome::default();
        if n == 0 {
            return out;
        }
        if self.modes.len() != n {
            *self = MenuState::new(mode_counts);
        }
        let before = (self.focus, self.modes[self.focus], self.page);

        if input.right {
            self.focus_game((self.focus + 1) % n, n);
        }
        if input.left {
            self.focus_game((self.focus + n - 1) % n, n);
        }
        let m = mode_counts[self.focus].max(1);
        if input.down {
            self.modes[self.focus] = (self.modes[self.focus] + 1) % m;
        }
        if input.up {
            self.modes[self.focus] = (self.modes[self.focus] + m - 1) % m;
        }
        if input.settings {
            out.action = Some(MenuAction::OpenSettings);
        }
        if input.confirm {
            out.action = Some(MenuAction::Play(self.selection()));
        }
        if let Some(d) = input.digit.filter(|&d| d < n) {
            self.focus_game(d, n);
            out.action = Some(MenuAction::Play(self.selection()));
        }

        // Mouse.
        let lay = layout(n, self.page);
        if lay.pages > 1 {
            let (prev, next) = arrow_rects();
            if input.click && prev.contains(input.mouse) {
                self.page = (lay.page + lay.pages - 1) % lay.pages;
                self.focus = self.page * PAGE_SIZE;
            } else if input.click && next.contains(input.mouse) {
                self.page = (lay.page + 1) % lay.pages;
                self.focus = self.page * PAGE_SIZE;
            }
        }
        let lay = layout(n, self.page);
        for &(g, rect) in &lay.cards {
            if !rect.contains(input.mouse) {
                continue;
            }
            if self.focus != g {
                self.focus = g;
            }
            let parts = card_parts(rect, mode_counts[g], lay.compact);
            let labels = vec![""; mode_counts[g]];
            let pills = ModePillSelector {
                area: parts.pills,
                layout: PillLayout::Vertical,
                labels: &labels,
                selected: 0,
                accent: WHITE,
                gap: 6.0,
            };
            if input.click {
                if let Some(i) = pills.hit(input.mouse) {
                    self.modes[g] = i;
                } else if parts.play.contains(input.mouse) || parts.preview.contains(input.mouse) {
                    out.action = Some(MenuAction::Play(self.selection()));
                }
            }
        }
        if input.click && SETTINGS_BTN.contains(input.mouse) {
            out.action = Some(MenuAction::OpenSettings);
        }
        out.moved = before != (self.focus, self.modes[self.focus], self.page);
        out
    }
}

/// Greedy word wrap on measured width.
fn wrap(stage: &Stage, s: &str, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if !cur.is_empty() && text::measure(stage, &cand, Size::X1) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Everything the menu needs to draw.
pub struct MenuView<'a> {
    pub stage: &'a Stage,
    pub theme: &'a Theme,
    pub registry: &'a GameRegistry,
    pub state: &'a MenuState,
    pub save: &'a SaveData,
    pub today: &'a Date,
    pub bg: &'a Background,
    pub time: f32,
}

/// Personal best of a mode as display text.
pub fn best_text(
    save: &SaveData,
    game: &str,
    mode: &crate::arcade::GameModeInfo,
    today_iso: &str,
) -> Option<String> {
    let key = if mode.is_daily {
        daily_key(game, today_iso)
    } else {
        score_key(game, mode.id)
    };
    save.highscore(&key).map(|v| {
        if mode.ranked_by_time {
            digits::format_time(v as f64 / 1000.0)
        } else {
            v.to_string()
        }
    })
}

pub fn draw(v: &MenuView) {
    let (st, th) = (v.stage, v.theme);
    let pulse = v.bg.pulse();
    let mouse = st.mouse();
    let n = v.registry.len();

    // ---- global arcade bar
    text::label(
        st,
        "VIBECODED ARCADE",
        VIRTUAL_W * 0.5,
        26.0,
        Size::X3,
        th.text,
        Align::Center,
    );
    text::label(
        st,
        "pick a game. find your flow.",
        VIRTUAL_W * 0.5,
        80.0,
        Size::X1,
        with_alpha(th.accent2, 0.9),
        Align::Center,
    );
    text::label(
        st,
        &format!("DAILY #{}", v.today.iso()),
        40.0,
        34.0,
        Size::X1,
        th.accent2,
        Align::Left,
    );
    let total = v.registry.arcade_total(v.save);
    text::label(
        st,
        "ARCADE SCORE",
        VIRTUAL_W - 40.0,
        22.0,
        Size::X1,
        th.text_dim,
        Align::Right,
    );
    digits::draw_right(&total.to_string(), VIRTUAL_W - 40.0, 44.0, 28.0, th.text);

    // ---- cards
    let lay = layout(n, v.state.page);
    for &(g, rect) in &lay.cards {
        let Some(game) = v.registry.get(g) else {
            continue;
        };
        let sel = g == v.state.focus;
        let accent = game.accent_color();
        let modes = game.modes();
        let mode_i = v
            .state
            .modes
            .get(g)
            .copied()
            .unwrap_or(0)
            .min(modes.len() - 1);
        let mode = &modes[mode_i];
        let parts = card_parts(rect, modes.len(), lay.compact);

        RoundedPanel::new(rect, if sel { accent } else { th.panel_border }).draw(th);
        if sel {
            let a = 0.35 + 0.35 * pulse;
            super::widgets::shapes::outline_rounded(
                Rect::new(rect.x - 3.0, rect.y - 3.0, rect.w + 6.0, rect.h + 6.0),
                14.0,
                2.0,
                with_alpha(accent, a),
            );
        }
        text::label(
            st,
            game.title(),
            rect.x + 16.0,
            parts.title_y,
            if lay.compact { Size::X1 } else { Size::X2 },
            if sel { th.text } else { th.text_dim },
            Align::Left,
        );
        text::label(
            st,
            game.tagline(),
            rect.x + 16.0,
            parts.tagline_y,
            Size::X1,
            with_alpha(accent, 0.9),
            Align::Left,
        );
        // Live preview.
        draw_rectangle(
            parts.preview.x,
            parts.preview.y,
            parts.preview.w,
            parts.preview.h,
            with_alpha(BLACK, 0.25),
        );
        game.draw_preview(parts.preview, v.time + g as f32 * 1.7, th);

        let labels: Vec<&str> = modes.iter().map(|m| m.label).collect();
        ModePillSelector {
            area: parts.pills,
            layout: PillLayout::Vertical,
            labels: &labels,
            selected: mode_i,
            accent,
            gap: 6.0,
        }
        .draw(st, th, mouse, pulse);

        let desc = wrap(st, mode.description, rect.w - 32.0);
        for (i, l) in desc.iter().take(2).enumerate() {
            text::label(
                st,
                l,
                rect.x + 16.0,
                parts.desc_y + i as f32 * 20.0,
                Size::X1,
                th.text_dim,
                Align::Left,
            );
        }
        ActionButton::new(parts.play, "PLAY [ENTER]")
            .focused(sel)
            .accent(accent)
            .draw(st, th, mouse, pulse);
        let best = best_text(v.save, game.id(), mode, &v.today.iso());
        text::label(
            st,
            if mode.ranked_by_time {
                "BEST TIME"
            } else {
                "BEST"
            },
            parts.best.x,
            parts.best.y,
            Size::X1,
            th.text_dim,
            Align::Left,
        );
        let txt = best.unwrap_or_else(|| "-".into());
        digits::draw(&txt, parts.best.x, parts.best.y + 20.0, 18.0, th.text);
    }

    // ---- paging arrows
    if lay.pages > 1 {
        let (prev, next) = arrow_rects();
        for (r, label) in [(prev, "<"), (next, ">")] {
            let hover = r.contains(mouse);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                with_alpha(th.panel, if hover { 0.95 } else { 0.7 }),
            );
            text::label(
                st,
                label,
                r.x + r.w * 0.5,
                r.y + 32.0,
                Size::X2,
                th.text,
                Align::Center,
            );
        }
        text::label(
            st,
            &format!("PAGE {}/{}", lay.page + 1, lay.pages),
            VIRTUAL_W * 0.5,
            CONTENT.bottom() + 4.0,
            Size::X1,
            th.text_dim,
            Align::Center,
        );
    }

    // ---- bottom bar
    text::label(
        st,
        "LEFT/RIGHT game   UP/DOWN mode   ENTER play   1-9 quick start   T theme   B colour-blind   M mute",
        40.0,
        660.0,
        Size::X1,
        th.text_dim,
        Align::Left,
    );
    text::label(
        st,
        &format!("THEME: {}", th.name),
        40.0,
        684.0,
        Size::X1,
        with_alpha(th.text_dim, 0.8),
        Align::Left,
    );
    ActionButton::new(SETTINGS_BTN, "SETTINGS [TAB]").draw(st, th, mouse, pulse);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x - 0.01
            && inner.y >= outer.y - 0.01
            && inner.right() <= outer.right() + 0.01
            && inner.bottom() <= outer.bottom() + 0.01
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.right() - 0.01
            && b.x < a.right() - 0.01
            && a.y < b.bottom() - 0.01
            && b.y < a.bottom() - 0.01
    }

    #[test]
    fn layout_places_every_game_for_1_to_9_games() {
        for n in 1..=9 {
            let first = layout(n, 0);
            let mut seen = Vec::new();
            for page in 0..first.pages {
                let l = layout(n, page);
                assert_eq!(l.page, page);
                for (i, a) in l.cards.iter().enumerate() {
                    assert!(inside(CONTENT, a.1), "n={n} card {} outside content", a.0);
                    assert!(a.1.w > 200.0 && a.1.h > 200.0, "n={n} card too small");
                    for b in l.cards.iter().skip(i + 1) {
                        assert!(!overlap(a.1, b.1), "n={n} cards overlap");
                    }
                    seen.push(a.0);
                }
            }
            seen.sort();
            assert_eq!(
                seen,
                (0..n).collect::<Vec<_>>(),
                "n={n}: every game on some page"
            );
        }
        assert!(layout(0, 0).cards.is_empty());
    }

    #[test]
    fn two_and_three_games_sit_side_by_side_four_plus_use_a_grid() {
        let l2 = layout(2, 0);
        assert_eq!(l2.cards.len(), 2);
        assert!(l2.cards[0].1.right() < l2.cards[1].1.x);
        assert_eq!(l2.cards[0].1.y, l2.cards[1].1.y);
        let l3 = layout(3, 0);
        assert_eq!(l3.cards.len(), 3);
        assert_eq!(l3.pages, 1);
        let l5 = layout(5, 0);
        assert_eq!((l5.cards.len(), l5.pages, l5.compact), (4, 2, true));
        assert_eq!(layout(5, 1).cards.len(), 1);
        assert_eq!(layout(5, 99).page, 1, "page is clamped");
    }

    #[test]
    fn card_parts_stay_inside_the_card() {
        for n in 1..=9 {
            let l = layout(n, 0);
            for modes in 1..=6 {
                for &(_, r) in &l.cards {
                    let p = card_parts(r, modes, l.compact);
                    for (name, part) in [
                        ("preview", p.preview),
                        ("pills", p.pills),
                        ("play", p.play),
                        ("best", p.best),
                    ] {
                        assert!(inside(r, part), "n={n} modes={modes}: {name} outside card");
                    }
                    assert!(
                        p.pills.bottom() <= p.play.y + 0.01,
                        "pills overlap play row"
                    );
                }
            }
        }
    }

    #[test]
    fn keyboard_navigation_and_play() {
        let counts = [4, 4, 3];
        let mut m = MenuState::new(&counts);
        let o = m.update(
            &MenuInput {
                right: true,
                ..Default::default()
            },
            &counts,
        );
        assert!(o.moved);
        assert_eq!(m.focus, 1);
        m.update(
            &MenuInput {
                down: true,
                ..Default::default()
            },
            &counts,
        );
        m.update(
            &MenuInput {
                down: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(m.modes[1], 2);
        m.update(
            &MenuInput {
                up: true,
                ..Default::default()
            },
            &counts,
        );
        m.update(
            &MenuInput {
                up: true,
                ..Default::default()
            },
            &counts,
        );
        m.update(
            &MenuInput {
                up: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(m.modes[1], 3, "wraps around");
        let o = m.update(
            &MenuInput {
                confirm: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(
            o.action,
            Some(MenuAction::Play(Selection { game: 1, mode: 3 }))
        );
        m.update(
            &MenuInput {
                left: true,
                ..Default::default()
            },
            &counts,
        );
        m.update(
            &MenuInput {
                left: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(m.focus, 2, "wraps to the last game");
        let o = m.update(
            &MenuInput {
                digit: Some(0),
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(
            o.action,
            Some(MenuAction::Play(Selection { game: 0, mode: 0 }))
        );
        let o = m.update(
            &MenuInput {
                digit: Some(8),
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(o.action, None, "digit beyond the game count is ignored");
    }

    #[test]
    fn mouse_picks_mode_and_plays() {
        let counts = [4, 4];
        let mut m = MenuState::new(&counts);
        let l = layout(2, 0);
        let (_, card) = l.cards[1];
        let parts = card_parts(card, 4, l.compact);
        let pill = crate::presentation::widgets::pills::pill_rects(
            parts.pills,
            4,
            PillLayout::Vertical,
            6.0,
        )[2];
        let o = m.update(
            &MenuInput {
                mouse: pill.center(),
                click: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!((m.focus, m.modes[1]), (1, 2));
        assert_eq!(o.action, None, "picking a mode does not start the game");
        let o = m.update(
            &MenuInput {
                mouse: parts.play.center(),
                click: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(
            o.action,
            Some(MenuAction::Play(Selection { game: 1, mode: 2 }))
        );
        let o = m.update(
            &MenuInput {
                mouse: SETTINGS_BTN.center(),
                click: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!(o.action, Some(MenuAction::OpenSettings));
    }

    #[test]
    fn paging_follows_focus_with_many_games() {
        let counts = [2; 6];
        let mut m = MenuState::new(&counts);
        for _ in 0..4 {
            m.update(
                &MenuInput {
                    right: true,
                    ..Default::default()
                },
                &counts,
            );
        }
        assert_eq!((m.focus, m.page), (4, 1));
        m.update(
            &MenuInput {
                right: true,
                ..Default::default()
            },
            &counts,
        );
        m.update(
            &MenuInput {
                right: true,
                ..Default::default()
            },
            &counts,
        );
        assert_eq!((m.focus, m.page), (0, 0), "wraps to the first page");
    }

    #[test]
    fn best_text_formats_scores_and_times() {
        use crate::arcade::GameModeInfo;
        let mut s = SaveData::default();
        s.highscores.insert("g:score".into(), 1234);
        s.highscores.insert("g:race".into(), 61_500);
        s.highscores.insert("daily:g:2026-11-16".into(), 99);
        let score = GameModeInfo::new("score", "S", "");
        let race = GameModeInfo::new("race", "R", "").timed();
        let daily = GameModeInfo::new("d", "D", "").daily();
        assert_eq!(
            best_text(&s, "g", &score, "2026-11-16").as_deref(),
            Some("1234")
        );
        assert_eq!(
            best_text(&s, "g", &race, "2026-11-16").as_deref(),
            Some("1:01.50")
        );
        assert_eq!(
            best_text(&s, "g", &daily, "2026-11-16").as_deref(),
            Some("99")
        );
        assert_eq!(
            best_text(&s, "g", &daily, "2026-11-17"),
            None,
            "per-date best"
        );
    }
}
