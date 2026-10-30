//! Small strict-table decoding helpers; values are decoded directly to preserve source locations.
pub(crate) fn required<T, E: serde::de::Error>(
    value: Option<T>,
    field: &'static str,
) -> Result<T, E> {
    value.ok_or_else(|| E::missing_field(field))
}
pub(crate) fn only<E: serde::de::Error>(
    present: &[(&'static str, bool)],
    allowed: &'static [&'static str],
) -> Result<(), E> {
    for &(field, present) in present {
        if present && !allowed.contains(&field) {
            return Err(E::unknown_field(field, allowed));
        }
    }
    Ok(())
}
