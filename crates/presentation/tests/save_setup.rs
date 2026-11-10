//! Setup recovery names the actual managed path rather than an unpublished saved identity.
use auditorylab_app::{
    services::library::{DiagnosticDetail, LibraryDiagnostic, LibraryError},
    workspace::{
        Refusal,
        status::{FileStatus, LibraryContext, LibraryStatus},
    },
};
use auditorylab_presentation::workspace::status::{
    file_status_description, library_status_description,
};
#[test]
fn setup_refusal_and_action_time_failure_use_the_same_captured_root_and_initialize_recovery() {
    let root = std::path::PathBuf::from("explicit-user-library");
    for error in [
        LibraryError::RootMissing,
        LibraryError::NotInitialized,
        LibraryError::MissingMarkerOnCreate,
    ] {
        let refused = FileStatus::Refused(Refusal::LibraryUnavailable(LibraryDiagnostic {
            storage_id: root.display().to_string(),
            detail: DiagnosticDetail::Failure(Box::new(error.clone())),
        }));
        let failed = LibraryStatus::Failed {
            error: error.into(),
            publishing: true,
            context: LibraryContext {
                path: root.clone(),
                selector: Some("user:00000000-0000-4000-8000-000000000001".parse().unwrap()),
            },
        };
        for text in [
            file_status_description(&refused),
            library_status_description(&failed),
        ] {
            assert!(text.contains("explicit-user-library"));
            assert!(text.contains("Initialize configured library"));
            assert!(text.contains("Draft retained"));
            assert!(!text.contains("reload the saved version"));
            assert!(!text.contains("independent copy"));
            assert!(!text.contains("user:00000000"));
        }
    }
    for error in [
        LibraryError::Busy,
        LibraryError::UnsupportedLayout { found: 99 },
        LibraryError::MissingMarkerOnMutation,
    ] {
        assert!(!error.requires_initialization());
    }
}
