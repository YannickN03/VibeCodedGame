//! Physical keyboard -> arcade input snapshot (QWERTZ + QWERTY).
//!
//! The mapping to *actions* is done by each title; this file only translates
//! `KeyCode`s into the framework-free [`Key`] set.

use macroquad::prelude::*;

use crate::arcade::{ArcadeInput, Key};

/// Every physical key a title may read. `Y` and `Z` are both listed so the
/// same physical key works on QWERTZ and QWERTY layouts.
pub const KEYMAP: [(Key, &[KeyCode]); 17] = [
    (Key::Left, &[KeyCode::Left]),
    (Key::Right, &[KeyCode::Right]),
    (Key::Up, &[KeyCode::Up]),
    (Key::Down, &[KeyCode::Down]),
    (Key::W, &[KeyCode::W]),
    (Key::A, &[KeyCode::A]),
    (Key::S, &[KeyCode::S]),
    (Key::D, &[KeyCode::D]),
    (Key::Space, &[KeyCode::Space]),
    (Key::X, &[KeyCode::X]),
    (Key::Y, &[KeyCode::Y]),
    (Key::Z, &[KeyCode::Z]),
    (Key::C, &[KeyCode::C]),
    (Key::Ctrl, &[KeyCode::LeftControl, KeyCode::RightControl]),
    (Key::Shift, &[KeyCode::LeftShift, KeyCode::RightShift]),
    (Key::J, &[KeyCode::J]),
    (Key::K, &[KeyCode::K]),
];

/// Current keyboard state. A key pressed and released within one frame
/// still counts as held for that frame, so no tap is ever lost.
pub fn poll_arcade_input() -> ArcadeInput {
    let mut held = Vec::new();
    let mut pressed = Vec::new();
    for (key, codes) in KEYMAP {
        if codes.iter().any(|&k| is_key_down(k)) {
            held.push(key);
        }
        if codes.iter().any(|&k| is_key_pressed(k)) {
            pressed.push(key);
        }
    }
    ArcadeInput::new(held, pressed)
}

/// Rising edge of any of the keys (menus / global toggles).
pub fn pressed_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|&k| is_key_pressed(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_covers_every_key_once() {
        for key in Key::ALL {
            assert_eq!(
                KEYMAP.iter().filter(|(k, _)| *k == key).count(),
                1,
                "{key:?}"
            );
        }
    }

    #[test]
    fn original_keycodes_are_still_bound() {
        let codes =
            |k: Key| -> &'static [KeyCode] { KEYMAP.iter().find(|(x, _)| *x == k).unwrap().1 };
        // Rotate-left works on QWERTZ (Y) and QWERTY (Z) and with either Ctrl.
        assert!(codes(Key::Y).contains(&KeyCode::Y) && codes(Key::Z).contains(&KeyCode::Z));
        assert!(codes(Key::Ctrl).contains(&KeyCode::LeftControl));
        assert!(codes(Key::Ctrl).contains(&KeyCode::RightControl));
        assert!(codes(Key::Shift).contains(&KeyCode::LeftShift));
        assert!(codes(Key::Shift).contains(&KeyCode::RightShift));
        assert!(codes(Key::Space).contains(&KeyCode::Space));
        assert!(codes(Key::Up).contains(&KeyCode::Up) && codes(Key::X).contains(&KeyCode::X));
        assert!(codes(Key::C).contains(&KeyCode::C) && codes(Key::A).contains(&KeyCode::A));
    }
}
