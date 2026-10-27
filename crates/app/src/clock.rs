//! One observed UTC clock for document and membership publication.
use crate::{core::Timestamp, error::ClockError};
pub(crate) fn observed_now() -> Result<Timestamp, ClockError> {
    from_system_time(std::time::SystemTime::now())
}
fn from_system_time(observed: std::time::SystemTime) -> Result<Timestamp, ClockError> {
    let invalid = || ClockError { observed };
    let (seconds, nanos) = match observed.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => (
            i64::try_from(d.as_secs()).map_err(|_| invalid())?,
            d.subsec_nanos(),
        ),
        Err(e) => {
            let d = e.duration();
            let seconds = i64::try_from(d.as_secs()).map_err(|_| invalid())?;
            if d.subsec_nanos() == 0 {
                (-seconds, 0)
            } else {
                (-seconds - 1, 1_000_000_000 - d.subsec_nanos())
            }
        }
    };
    Timestamp::from_unix(seconds, nanos).ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_subseconds_and_the_portable_year_limit_use_the_observed_value() {
        use std::time::{Duration, SystemTime};
        let before = SystemTime::UNIX_EPOCH - Duration::from_millis(500);
        let value = from_system_time(before).unwrap();
        assert_eq!(
            (value.unix_seconds(), value.nanoseconds()),
            (-1, 500_000_000)
        );
        let beyond = SystemTime::UNIX_EPOCH + Duration::from_secs(253_402_300_800);
        assert_eq!(from_system_time(beyond).unwrap_err().observed, beyond);
    }
}
