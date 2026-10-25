//! Validation shared by identity generation and its unit tests.
pub(crate) fn revision(value: &str) -> Result<(), &'static str> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(
            "AUDITORYLAB_SOURCE_REVISION must contain a full 40- or 64-character ASCII hexadecimal commit ID",
        )
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn revision_validation_rejects_partial_and_non_ascii_values() {
        for value in [
            "",
            "abc",
            "éééééééééééééééééééé",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert!(super::revision(value).is_err());
        }
        assert!(super::revision(&"a".repeat(40)).is_ok());
        assert!(super::revision(&"F".repeat(64)).is_ok());
    }
}
