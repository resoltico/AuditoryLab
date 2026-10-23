//! Supported distribution targets and complete archive-set validation.
use crate::Result;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
pub(crate) const PRODUCTS: [(&str, &str); 2] = [("cli", "auditorylab"), ("gui", "auditorylab-gui")];
pub(crate) const TARGETS: [(&str, &str); 6] = [
    ("aarch64-apple-darwin", "macos-aarch64"),
    ("x86_64-apple-darwin", "macos-x86_64"),
    ("x86_64-unknown-linux-gnu", "linux-x86_64"),
    ("aarch64-unknown-linux-gnu", "linux-aarch64"),
    ("x86_64-pc-windows-msvc", "windows-x86_64"),
    ("aarch64-pc-windows-msvc", "windows-aarch64"),
];
pub(crate) fn platform(target: &str) -> Result<&'static str> {
    TARGETS
        .iter()
        .find(|(triple, _)| *triple == target)
        .map(|(_, platform)| *platform)
        .ok_or_else(|| format!("unsupported distribution target {target}").into())
}
fn expected(version: &str) -> BTreeSet<String> {
    TARGETS
        .iter()
        .flat_map(|(triple, platform)| {
            PRODUCTS.iter().map(move |(product, binary)| {
                format!(
                    "{binary}-{version}-{platform}.{}",
                    crate::archive::Format::for_product(*product == "gui", triple).extension()
                )
            })
        })
        .collect()
}
pub(crate) fn validate_names(version: &str, names: &[String]) -> Result<()> {
    let actual: BTreeSet<_> = names.iter().cloned().collect();
    if actual.len() != names.len() {
        return Err("duplicate distribution archive filename".into());
    }
    let expected = expected(version);
    if expected != actual {
        return Err(format!(
            "distribution archive set mismatch: missing {:?}; unexpected {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(&expected).collect::<Vec<_>>()
        )
        .into());
    }
    Ok(())
}
/// Read every file, including unexpected suffixes; never silently filter stale assets.
pub(crate) fn collect(version: &str, input: &Path, output: &Path) -> Result<()> {
    let mut paths = Vec::<PathBuf>::new();
    for artifact in fs::read_dir(input)? {
        let artifact = artifact?.path();
        if !artifact.is_dir() {
            return Err(format!("expected distribution artifact directory: {artifact:?}").into());
        }
        for file in fs::read_dir(artifact)? {
            let file = file?.path();
            if !file.is_file() {
                return Err(format!("expected archive file: {file:?}").into());
            }
            paths.push(file);
        }
    }
    let names = paths
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
                .ok_or("non-UTF8 archive filename")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    validate_names(version, &names)?;
    if output.exists() && fs::read_dir(output)?.next().is_some() {
        return Err("asset output directory must be empty".into());
    }
    fs::create_dir_all(output)?;
    for (path, name) in paths.iter().zip(names) {
        fs::copy(path, output.join(name))?;
    }
    println!(
        "Verified complete archive set: {} products × {} targets.",
        PRODUCTS.len(),
        TARGETS.len()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_complete_thin_product_set_is_accepted() {
        let names: Vec<_> = expected("0.4.0").into_iter().collect();
        validate_names("0.4.0", &names).unwrap();
        assert_eq!(names.len(), 12);
        for name in [
            "auditorylab-gui-0.4.0-macos-aarch64.zip",
            "auditorylab-gui-0.4.0-macos-x86_64.zip",
            "auditorylab-0.4.0-macos-aarch64.tar.gz",
        ] {
            assert!(names.iter().any(|actual| actual == name));
        }
        let wrong: Vec<_> = names
            .iter()
            .map(|name| {
                name.replace(
                    "auditorylab-gui-0.4.0-macos-aarch64.zip",
                    "auditorylab-gui-0.4.0-macos-aarch64.tar.gz",
                )
            })
            .collect();
        assert!(validate_names("0.4.0", &wrong).is_err());

        for omit in 0..names.len() {
            let mut incomplete = names.clone();
            incomplete.remove(omit);
            assert!(validate_names("0.4.0", &incomplete).is_err());
        }
        for extra in [
            names[0].clone(),
            "auditorylab-0.4.0-macos-universal.tar.gz".into(),
            "old.zip".into(),
        ] {
            let mut wrong = names.clone();
            wrong.push(extra);
            assert!(validate_names("0.4.0", &wrong).is_err());
        }
        assert!(validate_names("0.5.0", &names).is_err());
        assert!(platform("universal-apple-darwin").is_err());
    }
}
