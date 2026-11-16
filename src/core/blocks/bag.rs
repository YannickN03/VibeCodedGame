//! Classic 7-bag randomizer (Fisher–Yates) on top of the shared `SplitMix64`.

use std::collections::VecDeque;

use crate::core::rng::SplitMix64;

use super::piece::PieceKind;

/// Classic 7-bag randomizer with a look-ahead queue.
#[derive(Clone, Debug)]
pub struct SevenBag {
    rng: SplitMix64,
    queue: VecDeque<PieceKind>,
}

impl SevenBag {
    /// Number of pieces always kept available for previews.
    const MIN_QUEUE: usize = 7;

    pub fn new(seed: u64) -> Self {
        let mut bag = SevenBag {
            rng: SplitMix64::new(seed),
            queue: VecDeque::with_capacity(14),
        };
        bag.refill();
        bag
    }

    fn refill(&mut self) {
        while self.queue.len() <= Self::MIN_QUEUE {
            let mut set = PieceKind::ALL;
            // Fisher–Yates
            for i in (1..set.len()).rev() {
                let j = self.rng.next_below(i as u64 + 1) as usize;
                set.swap(i, j);
            }
            self.queue.extend(set);
        }
    }

    /// Takes the next piece.
    pub fn next_piece(&mut self) -> PieceKind {
        let p = self.queue.pop_front().unwrap_or(PieceKind::T);
        self.refill();
        p
    }

    /// The next `n` pieces without consuming them (`n <= 7`).
    pub fn peek(&self, n: usize) -> Vec<PieceKind> {
        self.queue.iter().take(n).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bag_is_deterministic_for_seed() {
        let mut a = SevenBag::new(20261116);
        let mut b = SevenBag::new(20261116);
        for _ in 0..100 {
            assert_eq!(a.next_piece(), b.next_piece());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = SevenBag::new(1);
        let mut b = SevenBag::new(2);
        let sa: Vec<_> = (0..21).map(|_| a.next_piece()).collect();
        let sb: Vec<_> = (0..21).map(|_| b.next_piece()).collect();
        assert_ne!(sa, sb);
    }

    #[test]
    fn every_bag_contains_each_piece_once() {
        let mut bag = SevenBag::new(42);
        for _ in 0..50 {
            let mut set: Vec<_> = (0..7).map(|_| bag.next_piece()).collect();
            set.sort();
            assert_eq!(set, PieceKind::ALL.to_vec());
        }
    }

    #[test]
    fn peek_matches_next() {
        let mut bag = SevenBag::new(9);
        let preview = bag.peek(4);
        let drawn: Vec<_> = (0..4).map(|_| bag.next_piece()).collect();
        assert_eq!(preview, drawn);
    }
}
