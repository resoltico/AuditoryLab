//! numbers presentation.
/// Format seconds as hours, minutes and seconds; `~` marks nanosecond rounding.
#[must_use]
pub(crate) fn human_duration(value: f64) -> String {
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

/// A transport clock of whole seconds: `m:ss`, or `h:mm:ss` from one hour.
/// Partial seconds are dropped, so the clock advances once per second.
#[must_use]
pub(crate) fn clock(seconds: f64) -> String {
    let whole = std::time::Duration::try_from_secs_f64(seconds.max(0.0))
        .map_or(u64::MAX, |time| time.as_secs());
    let (hours, minutes, seconds) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours == 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("{hours}:{minutes:02}:{seconds:02}")
    }
}

/// A waveform axis limit with three significant digits, for tick labels.
#[must_use]
pub(crate) fn axis_limit(limit: f64) -> String {
    if !limit.is_finite() || limit <= 0.0 {
        return compact_number(limit);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "A positive finite magnitude's decimal exponent is clamped to 0..=12 \
            before use as a digit count"
    )]
    let decimals = (2.0 - limit.log10().floor()).clamp(0.0, 12.0) as usize;
    let text = format!("{limit:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// One-decimal digital level; silence uses the minus-infinity symbol.
#[must_use]
/// A file size in decimal units, as file managers show it.
pub(crate) fn file_size(bytes: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Sizes are shown to three significant digits"
    )]
    let value = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} bytes"),
        1_000..1_000_000 => format!("{:.1} kB", value / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", value / 1e6),
        _ => format!("{:.2} GB", value / 1e9),
    }
}
/// Per-ear peak level, or "silent" when neither ear has any signal.
pub(crate) fn peak_level(peaks: [f64; 2]) -> String {
    if peaks.iter().all(|peak| *peak == f64::NEG_INFINITY) {
        "Peak level: silent".into()
    } else {
        format!(
            "Peak level: left {} dBFS · right {} dBFS",
            format_dbfs(peaks[0]),
            format_dbfs(peaks[1])
        )
    }
}
pub(crate) fn format_dbfs(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "−∞".into()
    } else {
        format!("{value:.1}")
    }
}

/// Compact six-significant-digit number; approximation is explicit and
/// positive tiny values never become zero.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The decimal count is clamped to the finite integer range 0..=12 \
        before casting to usize; rounding formats presentation text only"
)]
#[must_use]
pub(crate) fn compact_number(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return value.to_string();
    }
    let decimals = (5.0 - value.abs().log10().floor()).clamp(0.0, 12.0) as usize;
    let text = if value.abs() < 1e-6 || value.abs() >= 1e6 {
        format!("{value:.5e}")
    } else if decimals == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.decimals$}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    let rounded = text.parse::<f64>().unwrap_or(value);
    format!(
        "{}{text}",
        if (rounded - value).abs() > value.abs() * 1e-12 {
            "≈"
        } else {
            ""
        }
    )
}

pub fn format(style: NumberStyle, value: f64) -> String {
    match style {
        NumberStyle::Duration => human_duration(value),
        NumberStyle::Clock => clock(value),
        NumberStyle::AxisLimit => axis_limit(value),
        NumberStyle::Decibels => format_dbfs(value),
        NumberStyle::Compact => compact_number(value),
        NumberStyle::CapturedMilliseconds => format!("{value:.2} ms"),
        NumberStyle::Milliseconds => format!("{value} ms"),
        NumberStyle::Hertz => format!("{value} Hz"),
    }
}

/// Numeric display policy, separate from exact model values and input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// Requested durations with meaningful units and explicit rounding.
    Duration,
    /// Whole elapsed transport time.
    Clock,
    /// Three significant digits for a waveform scale.
    AxisLimit,
    /// One-decimal digital level, with a silence symbol.
    Decibels,
    /// Compact significant digits with explicit approximation.
    Compact,
    /// Captured monitor window with two decimal places and its unit.
    CapturedMilliseconds,
    /// Available monitor time-window choices and their unit.
    Milliseconds,
    /// A sample-rate choice and its unit.
    Hertz,
}
