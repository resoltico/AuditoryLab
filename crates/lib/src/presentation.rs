//! Shared human-readable sound descriptions.
/// Format seconds as hours, minutes and seconds; `~` marks nanosecond rounding.
pub fn human_duration(value: f64) -> String {
    fn scalar(value: f64) -> String {
        if value.is_finite() && value != 0.0 && (value.abs() >= 1e12 || value.abs() < 1e-6) {
            format!("{value:e}")
        } else {
            value.to_string()
        }
    }
    let Ok(time) = std::time::Duration::try_from_secs_f64(value) else {
        return format!("{} s", scalar(value));
    };
    if value > 0.0 && time.is_zero() {
        return format!("{} s", scalar(value));
    }
    let mut parts = Vec::new();
    let secs = time.as_secs();
    if secs / 3600 != 0 {
        parts.push(format!("{} h", secs / 3600));
    }
    if (secs % 3600) / 60 != 0 {
        parts.push(format!("{} min", (secs % 3600) / 60));
    }
    if secs % 60 != 0 || time.subsec_nanos() != 0 || parts.is_empty() {
        let fraction = if time.subsec_nanos() == 0 {
            String::new()
        } else {
            format!(".{:09}", time.subsec_nanos())
                .trim_end_matches('0')
                .into()
        };
        parts.push(format!("{}{fraction} s", secs % 60));
    }
    format!(
        "{}{}",
        if time.as_secs_f64() == value { "" } else { "~" },
        parts.join(" ")
    )
}
/// One-decimal peak bound; silence uses the minus-infinity symbol.
pub fn format_dbfs(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "−∞".into()
    } else {
        format!("{value:.1}")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_keep_the_published_carry_tiny_and_invalid_cases() {
        for (seconds, expected) in [
            (0.0, "0 s"),
            (45.0, "45 s"),
            (60.0, "1 min"),
            (3600.0, "1 h"),
            (5400.0, "1 h 30 min"),
            (307.2, "5 min 7.2 s"),
            (59.9999999, "59.9999999 s"),
            (3599.9999999, "59 min 59.9999999 s"),
            (59.9999999999, "~1 min"),
            (3599.9999999999, "~1 h"),
            (1.0 / 768000.0, "~0.000001302 s"),
            (1e-12, "1e-12 s"),
            (1e300, "1e300 s"),
            (-1.0, "-1 s"),
            (f64::NAN, "NaN s"),
            (f64::INFINITY, "inf s"),
            (f64::NEG_INFINITY, "-inf s"),
            (18446744073709551616.0, "1.8446744073709552e19 s"),
            (68_719_476_736.0, "19088743 h 32 min 16 s"),
        ] {
            assert_eq!(human_duration(seconds), expected, "{seconds:?}");
        }
        assert_eq!(format_dbfs(-19.999999999999996), "-20.0");
        assert_eq!(format_dbfs(f64::NEG_INFINITY), "−∞");
    }
}
