//! Time parsing and file times. Everything inside Kilno is UTC; local time is for display only.

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use std::path::Path;

/// ISO-8601 timestamp: with offset or `Z`, or without (assumed UTC). Fractions of any length.
pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f%:z"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Some(Utc.from_utc_datetime(&n));
        }
    }
    None
}

/// Unix seconds to UTC.
pub fn from_unix(secs: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(secs, 0).single()
}

/// A file's last-write time (UTC), if it exists.
pub fn mtime(p: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Utc>::from)
}

/// Midnight (local) of the day `t` falls on, as UTC; used for the "Today" bucket.
pub fn local_date(t: DateTime<Utc>) -> chrono::NaiveDate {
    t.with_timezone(&Local).date_naive()
}

/// The minimum time, for "never".
pub fn min() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let a = parse_iso("2026-10-03T13:39:59.976874+00:00").unwrap();
        assert_eq!(a.timestamp_subsec_micros(), 976874);
        assert!(parse_iso("2026-10-08T10:00:00Z").is_some());
        assert!(parse_iso("2026-10-08T10:00:00").is_some());
        assert!(parse_iso("nope").is_none());
    }
}
