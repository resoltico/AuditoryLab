//! Actionable, nonblocking orientation preference notices.
use super::{Explain, IoOperation, path_text, publication};

impl Explain for auditorylab_app::onboarding::Problem {
    fn explain(&self) -> String {
        use auditorylab_app::onboarding::Problem as P;
        let problem = match self {
            P::ConfigurationUnavailable => {
                "The application configuration location is unavailable.".into()
            }
            P::Unsupported { path } => format!(
                "{} contains an unsupported preference format. It was left intact. \
                Inspect that file before moving it aside to allow a fresh preference.",
                path_text(path)
            ),
            P::File {
                path,
                operation,
                error,
            } => {
                let action = match operation {
                    IoOperation::Read | IoOperation::Inspect | IoOperation::OpenDirectory => "read",
                    _ => "write",
                };
                format!(
                    "Could not {action} {}: {}. Check the file and its parent directory's \
                    accessibility and permissions; existing preference bytes were not replaced.",
                    path_text(path),
                    error.explain()
                )
            }
            P::Published { path, warnings } => {
                return format!(
                    "Start here preference {} was saved with a storage warning: {}. \
                You can continue working.",
                    path_text(path),
                    warnings
                        .iter()
                        .map(publication::warning)
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
        };
        format!("Could not remember Start here. {problem} You can continue working.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_app::onboarding::{Preference, Problem};
    use std::path::Path;

    fn scratch() -> std::io::Result<tempfile::TempDir> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base)?;
        tempfile::tempdir_in(base)
    }

    #[test]
    fn actual_unsupported_and_oversized_notices_preserve_path_and_bytes() {
        let root = scratch().unwrap();
        let preference = Preference::at_config_directory(root.path().to_owned());
        let path = root.path().join("onboarding-seen");
        for (bytes, phrase) in [
            (
                b"unsupported future flag".to_vec(),
                "unsupported preference format",
            ),
            (vec![0; 129], "exceeds the bounded read limit"),
        ] {
            std::fs::write(&path, &bytes).unwrap();
            for problem in [
                preference.seen().unwrap_err(),
                preference.mark_seen().unwrap_err(),
            ] {
                let notice = problem.explain();
                assert!(notice.contains(path.to_str().unwrap()), "{notice}");
                assert!(notice.contains(phrase), "{notice}");
                assert!(notice.contains("continue working"), "{notice}");
                assert!(!notice.contains("file.read_limit"), "{notice}");
            }
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn actual_nonregular_and_unavailable_notices_offer_recovery_without_repair() {
        let root = scratch().unwrap();
        let path = root.path().join("onboarding-seen");
        std::fs::create_dir(&path).unwrap();
        let preference = Preference::at_config_directory(root.path().to_owned());
        let notice = preference.seen().unwrap_err().explain();
        assert!(notice.contains(path.to_str().unwrap()), "{notice}");
        assert!(notice.contains("expected a regular file"), "{notice}");
        assert!(path.is_dir());
        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, b"keep").unwrap();
        let preference = Preference::at_config_directory(blocked.clone());
        for problem in [
            preference.seen().unwrap_err(),
            preference.mark_seen().unwrap_err(),
        ] {
            let notice = problem.explain();
            assert!(notice.contains(blocked.to_str().unwrap()), "{notice}");
            assert!(notice.contains("permissions"), "{notice}");
            assert!(notice.contains("continue working"), "{notice}");
            assert!(matches!(problem, Problem::File { .. }));
        }
        assert_eq!(std::fs::read(blocked).unwrap(), b"keep");
        assert!(
            Problem::ConfigurationUnavailable
                .explain()
                .contains("location is unavailable")
        );
    }

    #[cfg(unix)]
    #[test]
    fn actual_read_write_and_symlink_failures_retain_original_causes() {
        use auditorylab_app::services::FileError;
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = scratch().unwrap();
        let directory = root.path().join("config");
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("onboarding-seen");
        let preference = Preference::at_config_directory(directory.clone());
        std::fs::write(&path, b"preserve bytes").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let problem = preference.seen().unwrap_err();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let notice = problem.explain();
        assert!(notice.contains(path.to_str().unwrap()) && notice.contains("Could not read"));
        assert!(
            matches!(problem, Problem::File { error: FileError::Io(ref io), .. }
            if io.cause.error().kind() == std::io::ErrorKind::PermissionDenied)
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
        let problem = preference.mark_seen().unwrap_err();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let notice = problem.explain();
        assert!(notice.contains(path.to_str().unwrap()) && notice.contains("Could not write"));
        assert!(
            matches!(problem, Problem::File { error: FileError::Io(ref io), .. }
            if io.cause.error().kind() == std::io::ErrorKind::PermissionDenied)
        );
        let original = root.path().join("original");
        std::fs::write(&original, b"keep original").unwrap();
        symlink(&original, &path).unwrap();
        let notice = preference.mark_seen().unwrap_err().explain();
        assert!(notice.contains(path.to_str().unwrap()) && notice.contains("regular file"));
        assert_eq!(std::fs::read(original).unwrap(), b"keep original");
    }
}
