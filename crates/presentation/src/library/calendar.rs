//! Native-calendar wording over application-resolved local instants.
use auditorylab_app::discovery::calendar;
use auditorylab_core::Timestamp;
/// Current native timezone beside inclusive calendar fields.
#[must_use]
pub fn local_context() -> String {
    let now = calendar::local_now();
    format!(
        "Dates (your recipes and open files) · inclusive local days · {} · YYYY-MM-DD",
        zone_label(
            &now.format("%Z").to_string(),
            &now.format("%:z").to_string()
        )
    )
}
/// A zone name with its offset, or the offset alone when the zone has no name.
#[must_use]
pub fn zone_label(name: &str, offset: &str) -> String {
    if name == offset {
        offset.to_owned()
    } else {
        format!("{name} ({offset})")
    }
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
