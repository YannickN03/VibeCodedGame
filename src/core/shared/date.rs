//! Pure calendar helpers for the `Daily Vibe` seed (UTC, proleptic Gregorian).

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Converts days since 1970-01-01 into a civil date
    /// (Howard Hinnant's `civil_from_days`).
    pub fn from_unix_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = if m <= 2 { y + 1 } else { y } as i32;
        Date {
            year,
            month: m,
            day: d,
        }
    }

    /// Converts seconds since the Unix epoch (UTC).
    pub fn from_unix_seconds(secs: u64) -> Date {
        Date::from_unix_days((secs / 86_400) as i64)
    }

    /// The `YYYYMMDD` integer used as daily seed.
    pub fn seed(&self) -> u64 {
        self.year as u64 * 10_000 + self.month as u64 * 100 + self.day as u64
    }

    /// ISO formatting `YYYY-MM-DD`.
    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(Date::from_unix_days(0).iso(), "1970-01-01");
    }

    #[test]
    fn known_dates() {
        // 2026-11-16 00:00:00 UTC = 1_794_787_200
        let d = Date::from_unix_seconds(1_794_787_200);
        assert_eq!(d.iso(), "2026-11-16");
        assert_eq!(d.seed(), 20_261_116);
        // leap day
        assert_eq!(Date::from_unix_seconds(951_782_400).iso(), "2000-02-29");
        // last second of a year
        assert_eq!(Date::from_unix_seconds(1_767_225_599).iso(), "2025-12-31");
    }
}
