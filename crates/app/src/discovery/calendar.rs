//! Native calendar ranges normalized to the shared half-open instant query.
use crate::core::Timestamp;
use crate::discovery::collection::DateRange;
use crate::error::QueryError as Error;
use chrono::{Local, NaiveDate, TimeZone};

/// The visible calendar-date field pair being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarField {
    /// Recipe creation dates.
    Created,
    /// Recipe modification dates.
    Modified,
    /// Library membership dates.
    Added,
}
impl CalendarField {
    /// Every calendar-date choice, in display order.
    pub const ALL: [Self; 3] = [Self::Created, Self::Modified, Self::Added];
    const fn terms(self) -> (&'static str, &'static str) {
        match self {
            Self::Created => ("Created", "created"),
            Self::Modified => ("Modified", "modified"),
            Self::Added => ("Added here", "added here"),
        }
    }
    /// Domain name for a heading or choice.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.terms().0
    }
    /// Domain name within a sentence.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        self.terms().1
    }
}

/// Convert inclusive YYYY-MM-DD calendar bounds in the current native timezone.
/// The upper bound is the next local midnight, including DST offset changes.
///
/// # Errors
/// Refuses invalid/inverted dates and ambiguous or nonexistent local midnights.
pub fn local_range(field: CalendarField, from: &str, through: &str) -> Result<DateRange, Error> {
    in_zone(&Local, field, from, through)
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
fn in_zone<T: TimeZone>(
    zone: &T,
    field: CalendarField,
    from: &str,
    through: &str,
) -> Result<DateRange, Error> {
    let boundary = |text: &str, next: bool| -> Result<Option<Timestamp>, Error> {
        if text.trim().is_empty() {
            return Ok(None);
        }
        let invalid = || Error::LocalDate { value: text.into() };
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
    range.validate().map_err(|_| Error::LocalOrder {
        field,
        from: from.into(),
        through: through.into(),
    })?;
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
        let spring = in_zone(
            &Transition,
            CalendarField::Created,
            "2026-03-29",
            "2026-03-29",
        )
        .unwrap();
        assert_eq!(
            spring.from.unwrap().to_string(),
            "2026-03-28T22:00:00.000000000Z"
        );
        assert_eq!(
            spring.before.unwrap().to_string(),
            "2026-03-29T21:00:00.000000000Z"
        );
        let fall = in_zone(
            &Transition,
            CalendarField::Created,
            "2026-10-25",
            "2026-10-25",
        )
        .unwrap();
        assert_eq!(
            fall.before.unwrap().unix_seconds() - fall.from.unwrap().unix_seconds(),
            25 * 3600
        );
        assert!(in_zone(&Transition, CalendarField::Created, "2026-3-29", "").is_err());
        assert!(
            in_zone(
                &Transition,
                CalendarField::Created,
                "2026-05-03",
                "2026-05-01"
            )
            .is_err()
        );
        assert!(!spring.contains(None));
        assert_eq!(
            local_range(CalendarField::Created, "", "").unwrap(),
            DateRange::default()
        );
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;
    #[test]
    fn calendar_order_errors_name_visible_fields_and_same_day_is_inclusive() {
        for (field, label) in [
            (CalendarField::Created, "Created"),
            (CalendarField::Modified, "Modified"),
            (CalendarField::Added, "Added here"),
        ] {
            let error = local_range(field, "2026-10-26", "2026-10-25")
                .unwrap_err()
                .to_string();
            assert_eq!(
                error,
                format!(
                    "{label} through (2026-10-25) must be on or after {label} from (2026-10-26)."
                )
            );
            assert!(!error.contains("before"));
            let range = local_range(field, "2026-10-25", "2026-10-25").unwrap();
            assert!(range.from.unwrap() < range.before.unwrap());
            assert!(range.contains(range.from));
            assert!(!range.contains(range.before));
        }
    }
}
