//! System clock access (kept out of `core` so the rules stay pure).

use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::date::Date;

/// Today's date in UTC.
pub fn today_utc() -> Date {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Date::from_unix_seconds(secs)
}

/// A seed for non-daily modes (time based, different every run).
pub fn session_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED)
}
