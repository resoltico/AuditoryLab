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
#[expect(
    clippy::unnecessary_debug_formatting,
    reason = "Native diagnostic paths retain escaped, quoted bytes, including non-UTF-8 names"
)]
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

/// Check the infrastructure map and dependency policy against packaging's target owner.
pub(crate) fn check() -> Result<()> {
    let matrix = serde_json::from_str(&fs::read_to_string(".github/native-targets.json")?)?;
    let deny = toml::from_str(&fs::read_to_string("deny.toml")?)?;
    check_inputs(&matrix, &deny)
}
fn check_inputs(matrix: &serde_json::Value, deny: &toml::Value) -> Result<()> {
    let rows = matrix["include"]
        .as_array()
        .ok_or("native target matrix missing")?;
    let mut pairs = BTreeSet::new();
    for row in rows {
        let target = row["target"].as_str().ok_or("target missing")?;
        let platform = row["platform"].as_str().ok_or("platform missing")?;
        if row["runner"].as_str().is_none_or(str::is_empty) {
            return Err("runner missing".into());
        }
        if !pairs.insert((target, platform)) {
            return Err("duplicate native target/platform pair".into());
        }
    }
    if pairs != TARGETS.into_iter().collect() {
        return Err("CI target/platform set differs from packaging".into());
    }
    let values = deny["graph"]["targets"]
        .as_array()
        .ok_or("deny targets missing")?;
    let mut targets = BTreeSet::new();
    for value in values {
        if !targets.insert(value.as_str().ok_or("deny target must be a string")?) {
            return Err("duplicate deny target".into());
        }
    }
    if targets != TARGETS.iter().map(|(target, _)| *target).collect() {
        return Err("deny target set differs from packaging".into());
    }
    if PRODUCTS != [("cli", "auditorylab"), ("gui", "auditorylab-gui")]
        || expected("qualification").len() != 12
    {
        return Err("qualification requires two products and twelve archives".into());
    }
    Ok(())
}
#[cfg(test)]
mod consistency_tests {
    use super::*;
    #[test]
    fn checked_owners_agree_and_reject_mismatches() {
        let matrix: serde_json::Value =
            serde_json::from_str(include_str!("../../.github/native-targets.json")).unwrap();
        let deny: toml::Value = toml::from_str(include_str!("../../deny.toml")).unwrap();
        check_inputs(&matrix, &deny).unwrap();
        let mut duplicate = matrix.clone();
        duplicate["include"]
            .as_array_mut()
            .unwrap()
            .push(matrix["include"][0].clone());
        assert!(check_inputs(&duplicate, &deny).is_err());
        let mut missing = matrix.clone();
        missing["include"].as_array_mut().unwrap().remove(0);
        assert!(check_inputs(&missing, &deny).is_err());
        let mut mismatched = matrix;
        mismatched["include"][0]["platform"] = "macos-x86_64".into();
        assert!(check_inputs(&mismatched, &deny).is_err());
        let mut duplicate = deny.clone();
        let first = duplicate["graph"]["targets"][0].clone();
        duplicate["graph"]["targets"]
            .as_array_mut()
            .unwrap()
            .push(first);
        assert!(
            check_inputs(
                &serde_json::from_str(include_str!("../../.github/native-targets.json")).unwrap(),
                &duplicate
            )
            .is_err()
        );
    }
}
