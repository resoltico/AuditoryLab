//! Inspect the extracted deliverable, including exact macOS architecture and identity.
use crate::{Result, capture, run};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn architecture(actual: &str, expected: &str) -> Result<()> {
    if actual.split_whitespace().collect::<Vec<_>>() != [expected] {
        return Err(format!(
            "expected exactly {expected}; extracted Mach-O architectures: {actual:?}"
        )
        .into());
    }
    Ok(())
}
pub(crate) fn inspect(
    archive: &Path,
    out: &Path,
    target: &str,
    version: &str,
    build: u32,
) -> Result<PathBuf> {
    let platform = crate::targets::platform(target)?;
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive filename")?;
    let binary = if name.starts_with("auditorylab-gui-") {
        "auditorylab-gui"
    } else {
        "auditorylab"
    };
    let stem = format!("{binary}-{version}-{platform}");
    let expected = format!(
        "{stem}.{}",
        if target.contains("windows") {
            "zip"
        } else {
            "tar.gz"
        }
    );
    if name != expected {
        return Err(format!("expected archive {expected}; got {name}").into());
    }
    fs::create_dir_all(out)?;
    let root = out.join(&stem);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    if target.contains("windows") {
        zip::ZipArchive::new(fs::File::open(archive)?)?.extract(out)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?)).unpack(out)?;
    }
    for file in ["LICENSE", "README.txt", "THIRD-PARTY-NOTICES.txt"] {
        if fs::read(root.join(file))?.is_empty() {
            return Err(format!("empty package member {file}").into());
        }
    }
    let app = root.join("Auditory Lab.app");
    let executable = if binary == "auditorylab-gui" && target.contains("apple") {
        app.join("Contents/MacOS/auditorylab-gui")
    } else {
        root.join(format!(
            "{binary}{}",
            if target.contains("windows") {
                ".exe"
            } else {
                ""
            }
        ))
    };
    #[cfg(unix)]
    if !target.contains("windows") {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&executable)?.permissions().mode() & 0o111 == 0 {
            return Err("extracted binary has no executable permission".into());
        }
    }
    if target.contains("apple") {
        let arches = String::from_utf8(capture(
            Command::new("lipo").arg("-archs").arg(&executable),
        )?)?;
        architecture(
            arches.trim(),
            if target.starts_with("aarch64") {
                "arm64"
            } else {
                "x86_64"
            },
        )?;
        if binary == "auditorylab-gui" {
            for (key, expected) in [
                ("CFBundleShortVersionString", version.to_owned()),
                ("CFBundleVersion", build.to_string()),
                ("LSMinimumSystemVersion", "15.0".into()),
            ] {
                let actual = String::from_utf8(capture(
                    Command::new("/usr/libexec/PlistBuddy")
                        .args(["-c", &format!("Print :{key}")])
                        .arg(app.join("Contents/Info.plist")),
                )?)?;
                if actual.trim() != expected {
                    return Err(
                        format!("extracted plist {key} differs: {actual:?} vs {expected}").into(),
                    );
                }
            }
            run(Command::new("codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&app))?;
        }
    }
    let actual = String::from_utf8(capture(Command::new(&executable).arg("--version"))?)?;
    if actual.trim() != crate::version_line(binary, version, build)? {
        return Err(format!("extracted executable identity differs: {actual:?}").into());
    }
    println!("Extracted package identity/content/architecture checks passed: {name}");
    Ok(executable)
}
#[cfg(test)]
mod tests {
    #[test]
    fn fat_and_wrong_architecture_binaries_are_rejected() {
        assert!(super::architecture("arm64", "arm64").is_ok());
        assert!(super::architecture("x86_64", "x86_64").is_ok());
        for actual in ["arm64 x86_64", "x86_64 arm64", "x86_64", "arm64 arm64", ""] {
            assert!(super::architecture(actual, "arm64").is_err());
        }
    }
}
