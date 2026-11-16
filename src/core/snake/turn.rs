//! 2-slot FIFO of planned heading changes.
//!
//! A fast double input (e.g. `Up` then `Left` within one tick while heading
//! `Right`) must be played one turn per tick and must never produce a 180°
//! reversal into the own neck. Every request is therefore validated against
//! the **last queued** direction (or the current heading if the queue is
//! empty), not against the heading alone.

use std::collections::VecDeque;

use super::grid::Direction;

/// Maximum number of planned turns.
pub const TURN_SLOTS: usize = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnBuffer {
    queue: VecDeque<Direction>,
}

impl TurnBuffer {
    pub fn new() -> Self {
        TurnBuffer::default()
    }

    /// Tries to plan a turn to `requested`. Returns `true` if it was queued.
    /// Same-direction and opposite-direction requests (relative to the last
    /// planned heading) and requests on a full queue are rejected.
    pub fn push(&mut self, heading: Direction, requested: Direction) -> bool {
        let reference = self.queue.back().copied().unwrap_or(heading);
        if requested == reference || requested == reference.opposite() {
            return false;
        }
        if self.queue.len() >= TURN_SLOTS {
            return false;
        }
        self.queue.push_back(requested);
        true
    }

    /// The next heading to apply (one per tick).
    pub fn pop(&mut self) -> Option<Direction> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::*;

    #[test]
    fn rejects_same_and_opposite_of_heading() {
        let mut t = TurnBuffer::new();
        assert!(!t.push(Right, Right));
        assert!(!t.push(Right, Left), "180 degree turn");
        assert!(t.is_empty());
    }

    #[test]
    fn double_input_cannot_reverse_into_the_neck() {
        // Heading Right: Up then Down within one tick. Down would be a
        // reversal of the already planned Up, so it must be rejected.
        let mut t = TurnBuffer::new();
        assert!(t.push(Right, Up));
        assert!(!t.push(Right, Down), "opposite of the last queued turn");
        assert_eq!(t.pop(), Some(Up));
        assert_eq!(t.pop(), None);
    }

    #[test]
    fn two_step_u_turn_is_allowed_and_played_in_order() {
        // Right -> Up -> Left is legal: one turn per tick.
        let mut t = TurnBuffer::new();
        assert!(t.push(Right, Up));
        assert!(t.push(Right, Left));
        assert_eq!(t.len(), 2);
        assert_eq!(t.pop(), Some(Up));
        assert_eq!(t.pop(), Some(Left));
    }

    #[test]
    fn capacity_is_two() {
        let mut t = TurnBuffer::new();
        assert!(t.push(Right, Up));
        assert!(t.push(Right, Left));
        assert!(!t.push(Right, Down), "third turn is dropped");
        assert_eq!(t.len(), TURN_SLOTS);
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn every_queued_sequence_is_free_of_reversals() {
        // Exhaustive: all request sequences of length 4 from every heading.
        for heading in Direction::ALL {
            for a in Direction::ALL {
                for b in Direction::ALL {
                    for c in Direction::ALL {
                        let mut t = TurnBuffer::new();
                        t.push(heading, a);
                        t.push(heading, b);
                        t.push(heading, c);
                        let mut prev = heading;
                        while let Some(d) = t.pop() {
                            assert_ne!(d, prev.opposite(), "reversal {prev:?} -> {d:?}");
                            assert_ne!(d, prev, "no-op turn queued");
                            prev = d;
                        }
                    }
                }
            }
        }
    }
}
