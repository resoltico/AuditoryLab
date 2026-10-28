//! Formatting checks over the repository's tracked Rust and Slint source.
use crate::{Result, capture};
use std::{fs, path::Path, process::Command};

pub(crate) fn check() -> Result<()> {
    let files = capture(Command::new("git").args([
        "ls-files",
        "-z",
        "--",
        "*.rs",
        "*.slint",
        ":(exclude)third_party/**",
    ]))?;
    let paths = files
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(std::str::from_utf8)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !paths.iter().any(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "rs")
    }) || !paths.iter().any(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "slint")
    }) {
        return Err("style discovery must contain tracked Rust and Slint source".into());
    }
    check_formatter_version()?;
    let mut failures = Vec::new();
    for path in paths {
        let text = fs::read_to_string(path)?;
        for (index, line) in text.lines().enumerate() {
            let width = line.chars().count();
            if width > 100 {
                failures.push(format!(
                    "{path}:{}: {width} characters exceeds 100",
                    index + 1
                ));
            }
        }
        if Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "slint")
        {
            let formatted = capture(Command::new("slint-lsp").args(["format", path]))?;
            if formatted != text.as_bytes() {
                failures.push(format!("{path}: slint-lsp format would change this file"));
            }
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    Ok(())
}
fn check_formatter_version() -> Result<()> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string("Cargo.lock")?)?;
    let packages = lock["package"]
        .as_array()
        .ok_or("Cargo.lock packages missing")?;
    let versions = packages
        .iter()
        .filter(|package| package["name"].as_str() == Some("slint"))
        .map(|package| package["version"].as_str().ok_or("Slint version missing"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let [version] = versions.as_slice() else {
        return Err("one resolved Slint version is required".into());
    };
    let installed = String::from_utf8(capture(Command::new("slint-lsp").arg("--version"))?)?;
    if installed.trim() != format!("slint-lsp {version}") {
        return Err(format!(
            "formatter version {} differs from resolved Slint {version}",
            installed.trim()
        )
        .into());
    }
    Ok(())
}
