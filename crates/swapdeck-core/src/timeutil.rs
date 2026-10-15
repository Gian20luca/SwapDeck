//! Date e ore in secondi Unix (UTC), condivise da motore, lock e storico.

use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

/// Ora corrente in secondi Unix.
pub fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Data ISO 8601 / RFC 3339 di cswap (`2026-10-15T10:00:00.123456+00:00`) in secondi Unix.
pub fn parse_iso(text: &str) -> Option<i64> {
    OffsetDateTime::parse(text.trim(), &Rfc3339)
        .ok()
        .map(OffsetDateTime::unix_timestamp)
}

/// Secondi Unix in RFC 3339 UTC (`2026-10-15T10:00:00Z`).
pub fn format_iso(ts: i64) -> String {
    OffsetDateTime::from_unix_timestamp(ts)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_default()
}

/// Giorno UTC di un istante.
pub fn utc_date(ts: i64) -> Option<Date> {
    OffsetDateTime::from_unix_timestamp(ts)
        .ok()
        .map(|t| t.date())
}

/// Inizio (00:00 UTC) di un giorno, in secondi Unix.
pub fn day_start(date: Date) -> i64 {
    date.midnight().assume_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cswap_dates() {
        assert_eq!(parse_iso("1970-01-01T00:01:00Z"), Some(60));
        assert_eq!(parse_iso("1970-01-01T01:00:00.123456+01:00"), Some(0));
        assert_eq!(parse_iso("ieri"), None);
        assert_eq!(format_iso(60), "1970-01-01T00:01:00Z");
        let d = utc_date(86_400 + 5).unwrap();
        assert_eq!(day_start(d), 86_400);
    }
}
