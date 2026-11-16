//! Deterministic randomness: a hand-written `SplitMix64` PRNG driving a
//! Fisher–Yates shuffled 7-bag. No external `rand` crate.

use std::collections::VecDeque;

use super::piece::PieceKind;

/// SplitMix64 (Steele, Lea & Flood). 64-bit state, full period, tiny & fast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `0..bound` (Lemire's multiply-shift; `bound > 0`).
    pub fn next_below(&mut self, bound: u64) -> u64 {
        debug_assert!(bound > 0);
        ((self.next_u64() as u128 * bound as u128) >> 64) as u64
    }

    /// Uniform float in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

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
    fn splitmix_reference_values() {
        // Reference output for seed 0 (Vigna's reference implementation).
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    }

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

    #[test]
    fn next_below_in_range() {
        let mut r = SplitMix64::new(7);
        for _ in 0..1000 {
            assert!(r.next_below(7) < 7);
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
