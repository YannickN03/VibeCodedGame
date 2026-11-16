//! Abstract gameplay buttons and rising-edge detection.
//!
//! The presentation layer translates physical keys into a [`ButtonSet`] of
//! currently *held* buttons. Edge detection happens here, so one physical key
//! press can only ever trigger a single hard drop / rotation / hold.

/// Logical gameplay buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    SoftDrop,
    HardDrop,
    RotateCw,
    RotateCcw,
    Rotate180,
    Hold,
    Pause,
}

impl Button {
    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// A small bit set of buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonSet(u16);

impl ButtonSet {
    pub const EMPTY: ButtonSet = ButtonSet(0);

    pub fn with(mut self, b: Button) -> Self {
        self.insert(b);
        self
    }

    pub fn insert(&mut self, b: Button) {
        self.0 |= b.bit();
    }

    pub fn set(&mut self, b: Button, on: bool) {
        if on {
            self.insert(b);
        } else {
            self.0 &= !b.bit();
        }
    }

    pub fn contains(self, b: Button) -> bool {
        self.0 & b.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn union(self, other: ButtonSet) -> ButtonSet {
        ButtonSet(self.0 | other.0)
    }

    /// Buttons in `self` that are not in `other`.
    pub fn difference(self, other: ButtonSet) -> ButtonSet {
        ButtonSet(self.0 & !other.0)
    }
}

/// Converts a stream of held-states into rising edges.
#[derive(Clone, Debug, Default)]
pub struct EdgeDetector {
    prev: ButtonSet,
}

impl EdgeDetector {
    /// Returns the buttons that went from released to held since last call.
    pub fn update(&mut self, held: ButtonSet) -> ButtonSet {
        let pressed = held.difference(self.prev);
        self.prev = held;
        pressed
    }

    /// Treat everything currently held as already pressed (e.g. after unpause).
    pub fn latch(&mut self, held: ButtonSet) {
        self.prev = held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_produces_single_edge() {
        let mut e = EdgeDetector::default();
        let held = ButtonSet::EMPTY.with(Button::HardDrop);
        assert!(e.update(held).contains(Button::HardDrop));
        for _ in 0..100 {
            assert!(!e.update(held).contains(Button::HardDrop));
        }
        assert!(e.update(ButtonSet::EMPTY).is_empty());
        assert!(e.update(held).contains(Button::HardDrop));
    }
}
