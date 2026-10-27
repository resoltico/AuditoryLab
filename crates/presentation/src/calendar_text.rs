//! Native-calendar wording over application-resolved local instants.
use auditorylab_app::discovery::calendar;
use auditorylab_core::Timestamp;
/// Current native timezone beside inclusive calendar fields.
#[must_use]
pub fn local_context() -> String {
    format!(
        "Inclusive local calendar dates · {} · YYYY-MM-DD",
        calendar::local_now().format("%Z (%:z)")
    )
}
/// A stored instant with local offset; canonical UTC remains available separately.
#[must_use]
pub fn local_timestamp(date: Timestamp) -> String {
    calendar::local_time(date).map_or_else(
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
