//! Native calendar ranges normalized to the shared half-open instant query.
use crate::{Error, ErrorKind, Timestamp, collection::DateRange};
use chrono::{Local, NaiveDate, TimeZone};

/// Convert inclusive YYYY-MM-DD calendar bounds in the current native timezone.
/// The upper bound is the next local midnight, including DST offset changes.
///
/// # Errors
/// Refuses invalid/inverted dates and ambiguous or nonexistent local midnights.
pub fn local_range(from: &str, through: &str) -> Result<DateRange, Error> {
    in_zone(&Local, from, through)
}
/// Native timezone context shown beside calendar fields; no fixed offset is cached.
#[must_use]
pub fn local_context() -> String {
    format!(
        "Inclusive local calendar dates · {} · YYYY-MM-DD",
        Local::now().format("%Z (%:z)")
    )
}
/// Display a stored instant in the native timezone, including its UTC offset.
/// Canonical UTC remains available separately; this never changes the instant.
#[must_use]
pub fn local_timestamp(date: Timestamp) -> String {
    Local
        .timestamp_opt(date.unix_seconds(), date.nanoseconds())
        .single()
        .map_or_else(
            || format!("Local time unavailable · UTC {date}"),
            |value| {
                format!(
                    "{} UTC{}",
                    value.format("%Y-%m-%d %H:%M:%S%.f"),
                    value.offset()
                )
            },
        )
}
fn in_zone<T: TimeZone>(zone: &T, from: &str, through: &str) -> Result<DateRange, Error> {
    let boundary = |text: &str, next: bool| -> Result<Option<Timestamp>, Error> {
        if text.trim().is_empty() {
            return Ok(None);
        }
        let invalid = || {
            Error::new(
                ErrorKind::InvalidArgument,
                "Use YYYY-MM-DD dates with a unique local midnight and an ordered range.",
            )
        };
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| invalid())?;
        if date.format("%Y-%m-%d").to_string() != text {
            return Err(invalid());
        }
        let date = if next {
            date.succ_opt().ok_or_else(invalid)?
        } else {
            date
        };
        let midnight = zone
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or_else(invalid)?)
            .single()
            .ok_or_else(invalid)?;
        Timestamp::from_unix(midnight.timestamp(), 0)
            .map(Some)
            .ok_or_else(invalid)
    };
    let range = DateRange {
        from: boundary(from, false)?,
        before: boundary(through, true)?,
    };
    range.validate()?;
    Ok(range)
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, FixedOffset, LocalResult, NaiveDateTime};
    // Independent fixture models midnight offsets on the European transition
    // dates. It does not derive expected UTC instants from the range helper.
    #[derive(Clone)]
    struct Transition;
    impl TimeZone for Transition {
        type Offset = FixedOffset;
        fn from_offset(_: &FixedOffset) -> Self {
            Self
        }
        fn offset_from_local_date(&self, date: &NaiveDate) -> LocalResult<FixedOffset> {
            LocalResult::Single(self.offset_from_utc_date(date))
        }
        fn offset_from_local_datetime(&self, date: &NaiveDateTime) -> LocalResult<FixedOffset> {
            self.offset_from_local_date(&date.date())
        }
        fn offset_from_utc_date(&self, date: &NaiveDate) -> FixedOffset {
            FixedOffset::east_opt(
                if (date.month(), date.day()) >= (3, 30) && (date.month(), date.day()) < (10, 26) {
                    3 * 3600
                } else {
                    2 * 3600
                },
            )
            .unwrap()
        }
        fn offset_from_utc_datetime(&self, date: &NaiveDateTime) -> FixedOffset {
            self.offset_from_utc_date(&date.date())
        }
    }
    #[test]
    fn inclusive_calendar_days_use_next_local_midnight_not_24_hours() {
        let spring = in_zone(&Transition, "2026-03-29", "2026-03-29").unwrap();
        assert_eq!(
            spring.from.unwrap().to_string(),
            "2026-03-28T22:00:00.000000000Z"
        );
        assert_eq!(
            spring.before.unwrap().to_string(),
            "2026-03-29T21:00:00.000000000Z"
        );
        let fall = in_zone(&Transition, "2026-10-25", "2026-10-25").unwrap();
        assert_eq!(
            fall.before.unwrap().unix_seconds() - fall.from.unwrap().unix_seconds(),
            25 * 3600
        );
        assert!(in_zone(&Transition, "2026-3-29", "").is_err());
        assert!(in_zone(&Transition, "2026-05-03", "2026-05-01").is_err());
        assert!(!spring.contains(None));
        assert_eq!(local_range("", "").unwrap(), DateRange::default());
    }
}
