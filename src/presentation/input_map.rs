//! Physical keyboard -> abstract gameplay buttons (QWERTZ + QWERTY).

use macroquad::prelude::*;

use crate::engine::{Button, ButtonSet};

/// Key bindings. CCW rotation is bound to both `Y` and `Z` so the same
/// physical key works on QWERTZ and QWERTY layouts.
pub const BINDINGS: &[(Button, &[KeyCode])] = &[
    (Button::Left, &[KeyCode::Left]),
    (Button::Right, &[KeyCode::Right]),
    (Button::SoftDrop, &[KeyCode::Down]),
    (Button::HardDrop, &[KeyCode::Space]),
    (Button::RotateCw, &[KeyCode::Up, KeyCode::X]),
    (
        Button::RotateCcw,
        &[
            KeyCode::Y,
            KeyCode::Z,
            KeyCode::LeftControl,
            KeyCode::RightControl,
        ],
    ),
    (Button::Rotate180, &[KeyCode::A]),
    (
        Button::Hold,
        &[KeyCode::C, KeyCode::LeftShift, KeyCode::RightShift],
    ),
    (Button::Pause, &[KeyCode::P, KeyCode::Escape]),
];

/// Currently held gameplay buttons. A key pressed and released within one
/// frame still counts as held for that frame (so no tap is ever lost).
pub fn held_buttons() -> ButtonSet {
    let mut set = ButtonSet::EMPTY;
    for (button, keys) in BINDINGS {
        let down = keys.iter().any(|&k| is_key_down(k) || is_key_pressed(k));
        set.set(*button, down);
    }
    set
}

/// Rising edge of any of the keys (menus / global toggles).
pub fn pressed_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|&k| is_key_pressed(k))
}
