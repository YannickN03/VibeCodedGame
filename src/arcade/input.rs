//! Framework-free input snapshot handed to every [`ArcadeTitle`](super::ArcadeTitle).
//!
//! The presentation layer fills it from the keyboard; titles interpret the
//! keys themselves (`A` is "rotate 180" in Blocks but "left" in Snake), so
//! this type deliberately stays a plain set of physical keys.

/// Physical keys a game may react to. (`P`/`Esc` pause is handled by the
/// arcade controller and never reaches a title.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    W,
    A,
    S,
    D,
    Space,
    X,
    Y,
    Z,
    C,
    Ctrl,
    Shift,
}

impl Key {
    pub const ALL: [Key; 15] = [
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::W,
        Key::A,
        Key::S,
        Key::D,
        Key::Space,
        Key::X,
        Key::Y,
        Key::Z,
        Key::C,
        Key::Ctrl,
        Key::Shift,
    ];

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// Held keys plus the keys that went down this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArcadeInput {
    held: u32,
    pressed: u32,
}

impl ArcadeInput {
    pub const EMPTY: ArcadeInput = ArcadeInput {
        held: 0,
        pressed: 0,
    };

    /// A key pressed and released within one frame still counts as held
    /// for that frame, so no tap is ever lost.
    pub fn new(
        held: impl IntoIterator<Item = Key>,
        pressed: impl IntoIterator<Item = Key>,
    ) -> Self {
        let mut i = ArcadeInput::EMPTY;
        for k in pressed {
            i.pressed |= k.bit();
            i.held |= k.bit();
        }
        for k in held {
            i.held |= k.bit();
        }
        i
    }

    /// Input with only these keys newly pressed (and therefore held).
    pub fn tap(keys: &[Key]) -> Self {
        Self::new([], keys.iter().copied())
    }

    pub fn down(&self, k: Key) -> bool {
        self.held & k.bit() != 0
    }

    pub fn pressed(&self, k: Key) -> bool {
        self.pressed & k.bit() != 0
    }

    pub fn any_down(&self, keys: &[Key]) -> bool {
        keys.iter().any(|&k| self.down(k))
    }

    pub fn any_pressed(&self, keys: &[Key]) -> bool {
        keys.iter().any(|&k| self.pressed(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_implies_held() {
        let i = ArcadeInput::tap(&[Key::Space]);
        assert!(i.pressed(Key::Space) && i.down(Key::Space));
        assert!(!i.down(Key::A));
        let i = ArcadeInput::new([Key::Left], []);
        assert!(i.down(Key::Left) && !i.pressed(Key::Left));
        assert!(i.any_down(&[Key::Right, Key::Left]));
    }

    #[test]
    fn all_keys_have_distinct_bits() {
        let mut seen = 0u32;
        for k in Key::ALL {
            assert_eq!(seen & k.bit(), 0);
            seen |= k.bit();
        }
    }
}
