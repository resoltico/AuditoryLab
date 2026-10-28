//! Frozen commit identity and deterministic archive time inputs.
use crate::{Result, capture};
use std::{env, process::Command};
// Identity generation and packaging share validation of the canonical workspace inputs.
#[path = "../../crates/app/identity_input.rs"]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Packaging and native-resource readers share the full identity validation module"
    )
)]
pub(crate) mod identity_input;

// ZIP's civil date range is the common representable range for every product.
pub(crate) fn epoch(value: &str) -> Result<u64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("SOURCE_DATE_EPOCH must be decimal Unix seconds".into());
    }
    let value: u64 = value.parse()?;
    if !(315_532_800..=4_354_819_199).contains(&value) {
        return Err(
            "SOURCE_DATE_EPOCH must be within 1980-01-01..2107-12-31 UTC (ZIP range)".into(),
        );
    }
    Ok(value)
}

pub(crate) struct Source {
    pub(crate) revision: String,
    pub(crate) epoch: u64,
}

impl Source {
    pub(crate) fn resolve() -> Result<Self> {
        Self::at(
            &env::current_dir()?,
            env::var("AUDITORYLAB_SOURCE_REVISION").ok(),
            env::var("SOURCE_DATE_EPOCH").ok(),
        )
    }
    fn at(
        directory: &std::path::Path,
        supplied_revision: Option<String>,
        supplied_epoch: Option<String>,
    ) -> Result<Self> {
        let git = if directory.join(".git").try_exists()? {
            Some(
                Command::new("git")
                    .current_dir(directory)
                    .args(["rev-parse", "--verify", "HEAD"])
                    .output()?,
            )
        } else {
            None
        };
        if let Some(git) = git {
            if !git.status.success() {
                return Err("Cannot resolve source HEAD from existing .git".into());
            }

            let revision = String::from_utf8(git.stdout)?.trim().to_owned();
            identity_input::revision(&revision)?;
            if supplied_revision
                .as_ref()
                .is_some_and(|value| !value.eq_ignore_ascii_case(&revision))
            {
                return Err("AUDITORYLAB_SOURCE_REVISION differs from checked-out HEAD".into());
            }
            let seconds =
                String::from_utf8(capture(Command::new("git").current_dir(directory).args([
                    "show",
                    "-s",
                    "--format=%ct",
                    &revision,
                ]))?)?;
            let resolved = epoch(seconds.trim())?;
            if supplied_epoch
                .as_ref()
                .map(|v| epoch(v))
                .transpose()?
                .is_some_and(|v| v != resolved)
            {
                return Err("SOURCE_DATE_EPOCH differs from the source commit timestamp".into());
            }
            Ok(Self {
                revision,
                epoch: resolved,
            })
        } else {
            let revision = supplied_revision.ok_or(
                "Source tree without Git requires \
                AUDITORYLAB_SOURCE_REVISION and SOURCE_DATE_EPOCH from the release \
                commit",
            )?;
            identity_input::revision(&revision)?;
            let seconds = supplied_epoch.ok_or(
                "Source tree without Git requires SOURCE_DATE_EPOCH from the release commit",
            )?;
            Ok(Self {
                revision,
                epoch: epoch(&seconds)?,
            })
        }
    }
}

/// Convert Unix seconds to UTC civil fields; DOS ZIP stores seconds rounded down to even.
pub(crate) fn zip_time(seconds: u64) -> Result<zip::DateTime> {
    epoch(&seconds.to_string())?;
    let mut days = seconds / 86_400;
    let mut year: u16 = 1970;
    loop {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let length = if leap { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let lengths = [
        31,
        28 + u64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: u8 = 1;
    for length in lengths {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    Ok(zip::DateTime::from_date_and_time(
        year,
        month,
        u8::try_from(days + 1)?,
        u8::try_from(seconds / 3600 % 24)?,
        u8::try_from(seconds / 60 % 60)?,
        u8::try_from(seconds % 60 / 2 * 2)?,
    )?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_archives_require_both_validated_identity_inputs() {
        let temp = tempfile::tempdir().unwrap();
        assert!(super::Source::at(temp.path(), None, None).is_err());
        assert!(super::Source::at(temp.path(), Some("a".repeat(40)), None).is_err());
        assert!(
            super::Source::at(temp.path(), Some("short".into()), Some("1709251199".into()))
                .is_err()
        );
        assert!(super::Source::at(temp.path(), Some("a".repeat(40)), Some("0".into())).is_err());
        let source =
            super::Source::at(temp.path(), Some("a".repeat(40)), Some("1709251199".into()))
                .unwrap();
        assert_eq!(source.epoch, 1_709_251_199);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        assert!(super::Source::at(root, Some("0".repeat(40)), None).is_err());
        assert!(super::Source::at(root, None, Some("1709251199".into())).is_err());
    }
    #[test]
    fn epoch_and_utc_zip_dates_reject_defaults_and_preserve_leap_boundaries() {
        for value in [
            "",
            "-1",
            "0",
            "315532799",
            "4354819200",
            "1.5",
            "+315532800",
        ] {
            assert!(super::epoch(value).is_err());
        }
        let time = super::zip_time(1_709_251_199).unwrap(); // 2024-02-29 23:59:59 UTC
        assert_eq!(
            (
                time.year(),
                time.month(),
                time.day(),
                time.hour(),
                time.minute(),
                time.second()
            ),
            (2024, 2, 29, 23, 59, 58)
        );
        let time = super::zip_time(315_532_800).unwrap();
        assert_eq!((time.year(), time.month(), time.day()), (1980, 1, 1));
    }
}
