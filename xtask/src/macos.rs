//! Self-contained macOS app assembly; legal and icon resources precede signing.
use crate::{Result, archive, capture, native_version, run, source::Source};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const ICON_FILES: [&str; 3] = ["Assets.car", "AppIcon.icns", "icon-partial.plist"];
const SOURCE_MARKER: &str = "source-identity.txt";

fn identity(source: &Source) -> String {
    format!("revision={}\nepoch={}\n", source.revision, source.epoch)
}

/// Compile native resources on a macOS 26 host with the selected full Xcode.
/// Output is independent of executable architecture and retains macOS 15 support.
pub(crate) fn compile_icons(out: &Path, source: &Source) -> Result<()> {
    // A fresh directory prevents a previous compilation from supplying missing outputs.
    fs::create_dir(out)?;
    let partial = out.join("icon-partial.plist");
    run(Command::new(archive::actool()?)
        .env_remove("SDKROOT")
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .arg("artwork/auditorylab-icon/assets/macos/AppIcon.icon")
        .arg("--compile")
        .arg(out)
        .args([
            "--output-format",
            "human-readable-text",
            "--notices",
            "--warnings",
            "--errors",
            "--output-partial-info-plist",
        ])
        .arg(&partial)
        .args([
            "--app-icon",
            "AppIcon",
            "--include-all-app-icons",
            "--enable-on-demand-resources",
            "NO",
            "--development-region",
            "en",
            "--target-device",
            "mac",
            "--minimum-deployment-target",
            "15.0",
            "--platform",
            "macosx",
        ]))?;
    fs::write(out.join(SOURCE_MARKER), identity(source))?;
    validate_icons(out, source)
}

/// Admit only the native compiler's complete resources for this source commit.
pub(crate) fn validate_icons(directory: &Path, source: &Source) -> Result<()> {
    if fs::read_to_string(directory.join(SOURCE_MARKER))? != identity(source) {
        return Err(
            "macOS icon resources differ from the distribution source revision/epoch".into(),
        );
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || entry.metadata()?.len() == 0 {
            return Err("macOS icon input must contain only nonempty regular files".into());
        }
        names.push(entry.file_name());
    }
    names.sort();
    let mut expected: Vec<_> = ICON_FILES
        .into_iter()
        .chain([SOURCE_MARKER])
        .map(std::ffi::OsString::from)
        .collect();
    expected.sort();
    if names != expected {
        return Err("macOS icon input must contain exactly Assets.car, AppIcon.icns, icon-partial.plist and source-identity.txt".into());
    }
    let icns = fs::read(directory.join("AppIcon.icns"))?;
    if icns.len() <= 8
        || &icns[..4] != b"icns"
        || usize::try_from(u32::from_be_bytes(icns[4..8].try_into()?))? != icns.len()
    {
        return Err("macOS icon input has an invalid ICNS header/length".into());
    }
    let plist = capture(
        Command::new("plutil")
            .args(["-convert", "json", "-o", "-", "--"])
            .arg(directory.join("icon-partial.plist")),
    )?;
    let plist: serde_json::Value = serde_json::from_slice(&plist)?;
    let keys = plist
        .as_object()
        .ok_or("icon partial plist must be a dictionary")?;
    if keys.len() != 2
        || plist["CFBundleIconName"] != "AppIcon"
        || plist["CFBundleIconFile"] != "AppIcon"
    {
        return Err(
            "icon partial plist must declare only CFBundleIconName and CFBundleIconFile as AppIcon"
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn bundle(
    binary: &Path,
    root: &Path,
    version: &str,
    build: u32,
    icons: Option<&Path>,
    source: &Source,
) -> Result<PathBuf> {
    native_version(version)?;
    let app = root.join("Auditory Lab.app");
    let contents = app.join("Contents");
    let resources = contents.join("Resources");
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(&resources)?;
    fs::copy(binary, contents.join("MacOS/auditorylab-gui"))?;
    for name in crate::LEGAL_FILES {
        fs::copy(root.join(name), resources.join(name))?;
    }
    let local_icons = root.join("compiled-icons");
    let compiled_locally = icons.is_none();
    let icons = match icons {
        Some(directory) => directory,
        None => {
            compile_icons(&local_icons, source)?;
            &local_icons
        }
    };
    validate_icons(icons, source)?;
    for name in ICON_FILES {
        fs::copy(icons.join(name), resources.join(name))?;
    }
    if compiled_locally {
        fs::remove_dir_all(local_icons)?;
    }
    let partial = resources.join("icon-partial.plist");
    let plist = contents.join("Info.plist");
    fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>org.resoltico.auditorylab</string><key>CFBundleName</key><string>Auditory Lab</string><key>CFBundleDisplayName</key><string>Auditory Lab</string><key>CFBundleExecutable</key><string>auditorylab-gui</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>{version}</string><key>CFBundleVersion</key><string>{build}</string><key>LSMinimumSystemVersion</key><string>15.0</string><key>NSHighResolutionCapable</key><true/></dict></plist>"#
        ),
    )?;
    run(Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Merge '{}'", partial.display()))
        .arg(&plist))?;
    run(Command::new("codesign")
        .args(["--sign", "-", "--force"])
        .arg(&app))?;
    Ok(contents.join("MacOS/auditorylab-gui"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        Source {
            revision: "a".repeat(40),
            epoch: 1_709_251_199,
        }
    }

    fn fixture(directory: &Path) {
        fs::write(
            directory.join(SOURCE_MARKER),
            b"revision=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nepoch=1709251199\n",
        )
        .unwrap();
        fs::write(directory.join("Assets.car"), b"native catalog fixture").unwrap();
        fs::write(directory.join("AppIcon.icns"), b"icns\0\0\0\x09x").unwrap();
        fs::write(directory.join("icon-partial.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIconFile</key><string>AppIcon</string><key>CFBundleIconName</key><string>AppIcon</string></dict></plist>"#).unwrap();
    }

    #[test]
    fn icons_reject_missing_mismatched_and_incomplete_source_inputs() {
        let temp = tempfile::tempdir().unwrap();
        assert!(validate_icons(temp.path(), &source()).is_err());
        fixture(temp.path());
        let mut other = source();
        other.revision = "b".repeat(40);
        assert!(
            validate_icons(temp.path(), &other)
                .unwrap_err()
                .to_string()
                .contains("revision/epoch")
        );
        other = source();
        other.epoch += 1;
        assert!(
            validate_icons(temp.path(), &other)
                .unwrap_err()
                .to_string()
                .contains("revision/epoch")
        );
        fs::remove_file(temp.path().join("Assets.car")).unwrap();
        assert!(
            validate_icons(temp.path(), &source())
                .unwrap_err()
                .to_string()
                .contains("exactly")
        );
        fixture(temp.path());
        fs::write(temp.path().join("unexpected"), b"extra").unwrap();
        assert!(
            validate_icons(temp.path(), &source())
                .unwrap_err()
                .to_string()
                .contains("exactly")
        );
        fs::remove_file(temp.path().join("unexpected")).unwrap();
        fs::write(temp.path().join("Assets.car"), b"").unwrap();
        assert!(
            validate_icons(temp.path(), &source())
                .unwrap_err()
                .to_string()
                .contains("nonempty")
        );
        fixture(temp.path());
        fs::write(temp.path().join("AppIcon.icns"), b"icns\0\0\0\x10x").unwrap();
        assert!(
            validate_icons(temp.path(), &source())
                .unwrap_err()
                .to_string()
                .contains("ICNS")
        );
        assert!(compile_icons(temp.path(), &source()).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn icons_accept_native_plist_keys_and_reject_bundle_identity_overrides() {
        let temp = tempfile::tempdir().unwrap();
        fixture(temp.path());
        validate_icons(temp.path(), &source()).unwrap();
        fs::write(temp.path().join("icon-partial.plist"), br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIconFile</key><string>AppIcon</string><key>CFBundleIconName</key><string>AppIcon</string><key>CFBundleExecutable</key><string>other</string></dict></plist>"#).unwrap();
        assert!(
            validate_icons(temp.path(), &source())
                .unwrap_err()
                .to_string()
                .contains("must declare only")
        );
    }
}
