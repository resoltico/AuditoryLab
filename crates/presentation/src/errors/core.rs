//! Human explanations of portable-document rules.
use auditorylab_core::ContractError;
use auditorylab_core::PairError;
use auditorylab_core::Recipe;
use auditorylab_core::RecipeError;
use auditorylab_core::ReferenceOwner;
use auditorylab_core::Sound;
use auditorylab_core::StructureError;
/// Explain a rejected rate using the core's supported bounds.
#[must_use]
pub fn sample_rate_error(error: auditorylab_core::SampleRateError) -> String {
    format!(
        "sample rate {} Hz must be from {} to {} Hz",
        error.hz,
        auditorylab_core::SampleRate::MIN_HZ,
        auditorylab_core::SampleRate::MAX_HZ
    )
}
/// Explain a portable-document failure without losing its rule or supported version.
#[must_use]
pub fn recipe_error(error: &RecipeError) -> String {
    match error {
        RecipeError::TooLarge => format!("recipe exceeds {} bytes", Recipe::MAX_BYTES),
        RecipeError::NotUtf8 => "recipe is not UTF-8 text".into(),
        RecipeError::Utf16 => "recipe is UTF-16 text; save it as UTF-8".into(),
        RecipeError::Format(error) => {
            format!("recipe does not match the format: {error}")
        }
        RecipeError::Structure(error) => structure_error(error),
        RecipeError::Contract(error) => format!("recipe contract: {}", contract_error(error)),
        RecipeError::Write(error) => format!("cannot serialize recipe: {error}"),
        RecipeError::UnsupportedVersion { found } => format!(
            "recipe has schema_version {found}; this build reads {}",
            Recipe::SCHEMA_VERSION
        ),
    }
}
/// Explain a typed portable-contract rule.
#[must_use]
pub fn contract_error(error: &ContractError) -> String {
    match error {
        ContractError::EmptyTitle => "description title must not be empty".into(),
        ContractError::SourceBudget { count } => format!(
            "{count} sources exceed the limit of {}",
            auditorylab_core::provenance::MAX_SOURCES
        ),
        ContractError::EvidenceBudget { count } => format!(
            "{count} evidence records exceed the limit of {}",
            auditorylab_core::provenance::MAX_EVIDENCE
        ),
        ContractError::LineageBudget { count } => format!(
            "{count} ancestors exceed the limit of {}",
            auditorylab_core::provenance::MAX_LINEAGE
        ),
        ContractError::EmptySourceKey { index } => {
            format!("source {} requires a nonempty key", index + 1)
        }
        ContractError::DuplicateSourceKey { key } => format!("source key {key} is repeated"),
        ContractError::EmptySourceLabel { key } => format!("source {key} requires a label"),
        ContractError::EmptySourceCitation { key } => format!("source {key} requires a citation"),
        ContractError::MissingLocator { key } => {
            format!("source {key} requires a DOI or explicit http(s) URL")
        }
        ContractError::UrlScheme { key, url } => {
            format!("source {key} URL {url} must use explicit http(s)")
        }
        ContractError::InvalidUrl { key, url } => {
            format!("source {key} URL {url} requires a host and no whitespace/control characters")
        }
        ContractError::InvalidDoi { key, doi } => {
            format!("source {key} DOI {doi} must begin with 10. and include a nonempty suffix")
        }
        ContractError::DanglingReference { owner, key } => match owner {
            ReferenceOwner::Evidence(index) => {
                format!("evidence {} refers to missing source {key}", index + 1)
            }
            ReferenceOwner::Lineage(index) => {
                format!("ancestor {} refers to missing source {key}", index + 1)
            }
        },
        ContractError::EmptyLineageTitle { index } => {
            format!("ancestor {} requires a title", index + 1)
        }
        ContractError::EmptyLineageNote { index } => format!(
            "ancestor {} requires an explicit relationship note",
            index + 1
        ),
        ContractError::ReusedCopyIdentity { id } => {
            format!("copy requires a new document UUID; {id} is the original identity")
        }
        ContractError::CopyLineageFull { count } => format!(
            "lineage capacity reached with {count} ancestors; review metadata before making a copy"
        ),
    }
}
/// Explain an authored-owner or capacity failure.
#[must_use]
pub fn structure_error(error: &StructureError) -> String {
    match error {
        StructureError::Identity { id } => format!("two authored components share identity {id}"),
        StructureError::VoiceBudget { count } => format!(
            "{count} expanded voices exceed the limit of {}",
            Sound::MAX_VOICES
        ),
    }
}
/// Explain the coupled quantities of a failed pair.
#[must_use]
pub fn pair_error(error: &PairError) -> String {
    let reason = match error.issue {
        auditorylab_core::PairIssue::Nonfinite => "pair carrier/offset arithmetic is nonfinite",
        auditorylab_core::PairIssue::Nonpositive => "both pair carriers must be positive",
        auditorylab_core::PairIssue::UnrepresentablePairDifference => {
            "nonzero offset collapses to the first carrier"
        }
    };
    format!(
        "Component {}: first/left carrier {} Hz and beat offset {} Hz: {reason}",
        error.id, error.first_hz, error.offset_hz
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn messages_say_what_is_wrong_with_the_recipe() {
        assert_eq!(
            sample_rate_error(auditorylab_core::SampleRateError { hz: 0 }),
            "sample rate 0 Hz must be from 1 to 768000 Hz"
        );
        assert_eq!(
            recipe_error(&RecipeError::TooLarge),
            "recipe exceeds 65536 bytes"
        );
        assert_eq!(
            recipe_error(&RecipeError::NotUtf8),
            "recipe is not UTF-8 text"
        );
        assert_eq!(
            recipe_error(&RecipeError::Utf16),
            "recipe is UTF-16 text; save it as UTF-8"
        );
        let format = Recipe::parse("schema_version=5\nfrequency_hz='bad'").unwrap_err();
        let text = recipe_error(&format);
        assert!(
            text.starts_with("recipe does not match the format: "),
            "{text}"
        );
        assert!(text.contains("frequency_hz"), "{text}");
        assert!(format.source().is_some());
        assert!(RecipeError::TooLarge.source().is_none());
        for found in [0, 1, 2, 3, 4, 999, u32::MAX] {
            let error =
                Recipe::parse(&format!("schema_version={found}\nunknown=true")).unwrap_err();
            assert!(
                matches!(error, RecipeError::UnsupportedVersion { found: value } if value==found)
            );
            assert_eq!(
                recipe_error(&error),
                format!("recipe has schema_version {found}; this build reads 5")
            );
        }
    }
}

#[cfg(test)]
mod authored_tests {
    use super::*;
    use auditorylab_core::ComponentId;
    use auditorylab_core::PairIssue;
    #[test]
    fn lowering_failures_explain_the_rejected_owner_budget_or_pair() {
        let identity = structure_error(&StructureError::Identity {
            id: ComponentId::new(1).unwrap(),
        });
        assert!(identity.contains("identity 1"));
        let budget = structure_error(&StructureError::VoiceBudget { count: 18 });
        assert!(budget.contains("16"));
        assert!(budget.contains("18 expanded voices"));
        for (issue, explanation) in [
            (PairIssue::Nonfinite, "nonfinite"),
            (PairIssue::Nonpositive, "positive"),
            (PairIssue::UnrepresentablePairDifference, "offset collapses"),
        ] {
            let message = pair_error(&PairError {
                issue,
                id: ComponentId::new(42).unwrap(),
                first_hz: 200.0,
                offset_hz: -6.0,
            });
            assert!(message.contains("42"));
            assert!(message.contains("200"));
            assert!(message.contains("-6"));
            assert!(message.contains(explanation), "{message}");
        }
    }
}
