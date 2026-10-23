//! Self-contained macOS app assembly; legal and icon resources precede signing.
use crate::{Result, archive, native_version, run};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn bundle(binary: &Path, root: &Path, version: &str, build: u32) -> Result<PathBuf> {
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
    let partial = resources.join("icon-partial.plist");
    run(Command::new(archive::actool()?)
        .env_remove("SDKROOT")
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .arg("artwork/auditorylab-icon/assets/macos/AppIcon.icon")
        .arg("--compile")
        .arg(&resources)
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
    for name in ["Assets.car", "AppIcon.icns", "icon-partial.plist"] {
        if fs::metadata(resources.join(name))?.len() == 0 {
            return Err(format!("actool omitted {name}").into());
        }
    }
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
