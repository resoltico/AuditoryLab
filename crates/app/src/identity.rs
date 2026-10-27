//! Fallible random identities for documents and file publication.
/// Allocate a fallible random portable identity; never substitute clocks or counters.
pub(crate) fn fresh_document() -> Result<auditorylab_core::DocumentIdentity, crate::error::Error> {
    Ok(auditorylab_core::DocumentIdentity {
        id: random_uuid()?,
        created_at: None,
        modified_at: None,
    })
}

pub(crate) fn random_uuid() -> Result<auditorylab_core::DocumentUuid, crate::error::Error> {
    generate_uuid(getrandom::fill)
}
fn generate_uuid(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<auditorylab_core::DocumentUuid, crate::error::Error> {
    let mut bytes = [0_u8; 16];
    fill(&mut bytes).map_err(|e| {
        crate::error::Error::caused(
            crate::error::ErrorKind::Identity,
            format!("cannot allocate a random UUID: {e}"),
            e,
        )
    })?;
    Ok(auditorylab_core::DocumentUuid::from_random_bytes(bytes))
}
#[cfg(test)]
mod tests {
    #[test]
    fn random_failure_does_not_fall_back_to_clock_or_counter() {
        let error = super::generate_uuid(|_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Identity);
        assert!(error.to_string().contains("random UUID"));
        assert!(std::error::Error::source(&error).is_some());
    }
}
