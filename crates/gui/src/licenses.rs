//! Resolve installed notices relative to the executable, independent of cwd.
use std::path::{Path, PathBuf};

fn beside_executable(executable: &Path) -> Result<PathBuf, String> {
    let parent = executable
        .parent()
        .ok_or(auditorylab::vocabulary::Caption::ExecutableDirectoryUnavailable.label())?;
    let directory = if parent.file_name().is_some_and(|name| name == "MacOS")
        && parent
            .parent()
            .is_some_and(|path| path.file_name().is_some_and(|name| name == "Contents"))
    {
        parent
            .parent()
            .ok_or(auditorylab::vocabulary::Caption::BundleResourcesUnavailable.label())?
            .join("Resources")
    } else {
        parent.to_owned()
    };
    for name in [
        "LICENSE",
        "THIRD-PARTY-NOTICES.txt",
        "COPYRIGHT-library.html",
        "SOURCE.txt",
    ] {
        let path = directory.join(name);
        if !path.is_file() || std::fs::metadata(&path).map_err(|e| e.to_string())?.len() == 0 {
            return Err(auditorylab::vocabulary::Caption::missing_notices(&path));
        }
    }
    Ok(directory)
}

pub(crate) fn notices_directory() -> Result<PathBuf, String> {
    beside_executable(&std::env::current_exe().map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn installed_bundle_is_self_contained_and_missing_notices_are_visible() {
        let temp = tempfile::tempdir().unwrap();
        for relative in [
            "Auditory Lab.app/Contents/MacOS/auditorylab-gui",
            "portable/auditorylab-gui",
        ] {
            let executable = temp.path().join(relative);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            assert!(
                super::beside_executable(&executable)
                    .unwrap_err()
                    .contains("missing or empty")
            );
            let resources = if relative.starts_with("Auditory") {
                temp.path().join("Auditory Lab.app/Contents/Resources")
            } else {
                temp.path().join("portable")
            };
            std::fs::create_dir_all(&resources).unwrap();
            for name in [
                "LICENSE",
                "THIRD-PARTY-NOTICES.txt",
                "COPYRIGHT-library.html",
                "SOURCE.txt",
            ] {
                std::fs::write(resources.join(name), "complete fixture").unwrap();
            }
            assert_eq!(super::beside_executable(&executable).unwrap(), resources);
            std::fs::write(resources.join("THIRD-PARTY-NOTICES.txt"), "").unwrap();
            assert!(super::beside_executable(&executable).is_err());
        }
    }
}
