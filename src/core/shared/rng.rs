//! Deterministic randomness: a hand-written `SplitMix64` PRNG shared by all
//! games. No external `rand` crate.

/// The seed everybody shares on a given UTC day: `YYYYMMDD`
/// (`Daily Vibe` of every game).
pub fn daily_seed(date: &super::date::Date) -> u64 {
    date.seed()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_seed_is_the_date_as_yyyymmdd() {
        use super::super::date::Date;
        assert_eq!(
            daily_seed(&Date {
                year: 2026,
                month: 11,
                day: 16
            }),
            20261116
        );
    }

    #[test]
    fn splitmix_reference_values() {
        // Reference output for seed 0 (Vigna's reference implementation).
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
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
