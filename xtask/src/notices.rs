//! Per-product dependency license coverage and formatting.
use serde_json::Value;
use std::fmt::Write as _;
use std::{collections::BTreeSet, fs, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing string {key}").into())
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .ok_or_else(|| format!("missing array {key}").into())
}
pub(crate) fn identities(metadata: &Value, tree: &str) -> Result<BTreeSet<String>> {
    let mut ids = BTreeSet::new();
    for line in tree.lines().filter(|line| !line.is_empty()) {
        let mut words = line.split_whitespace();
        let name = words.next().ok_or("missing tree package name")?;
        let version = words
            .next()
            .and_then(|s| s.strip_prefix('v'))
            .ok_or("missing tree version")?;
        let packages: Vec<_> = array(metadata, "packages")?
            .iter()
            .filter(|p| p["name"] == name && p["version"] == version)
            .collect();
        if packages.len() != 1 {
            return Err(format!(
                "package {name} {version} has {} metadata identities",
                packages.len()
            )
            .into());
        }
        ids.insert(string(packages[0], "id")?.to_owned());
    }
    if ids.is_empty() {
        return Err("empty product dependency graph".into());
    }
    Ok(ids)
}
pub(crate) fn assemble(
    about: &Value,
    metadata: &Value,
    expected: &BTreeSet<String>,
    normal: &BTreeSet<String>,
    rust_version: &str,
    windows: bool,
) -> Result<String> {
    let crates = array(about, "crates")?;
    let actual: BTreeSet<_> = crates
        .iter()
        .map(|c| string(&c["package"], "id").map(str::to_owned))
        .collect::<Result<_>>()?;
    if &actual != expected {
        return Err(format!(
            "notice dependency mismatch; absent from notices: {:?}; absent from metadata: {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(expected).collect::<Vec<_>>()
        )
        .into());
    }
    let mut covered = BTreeSet::new();
    let mut text = String::from("Third-party notices\n===================\n\n");
    for license in array(about, "licenses")? {
        let mut names = Vec::new();
        for user in array(license, "used_by")? {
            let package = &user["crate"];
            covered.insert(string(package, "id")?.to_owned());
            if normal.contains(string(package, "id")?) {
                names.push(format!(
                    "{} {}",
                    string(package, "name")?,
                    string(package, "version")?
                ));
            }
        }
        names.sort();
        if !names.is_empty() {
            let _ = write!(
                text,
                "{}\nUsed by: {}\n\n{}\n\n",
                string(license, "name")?,
                names.join(", "),
                string(license, "text")?
            );
        }
    }
    let mut slint = Vec::new();
    for c in crates {
        let package = &c["package"];
        let id = string(package, "id")?;
        if !covered.contains(id)
            && package["license"]
                .as_str()
                .is_some_and(|s| s.contains("LicenseRef-Slint-Royalty-free-2.0"))
        {
            covered.insert(id.to_owned());
            if normal.contains(id) {
                slint.push(format!(
                    "{} {}",
                    string(package, "name")?,
                    string(package, "version")?
                ));
            }
        }
    }
    if !slint.is_empty() {
        let package = array(metadata, "packages")?
            .iter()
            .find(|p| p["name"] == "slint")
            .ok_or("Slint package missing")?;
        let manifest = Path::new(string(package, "manifest_path")?);
        let license = manifest
            .parent()
            .ok_or("Slint directory missing")?
            .join("LICENSES/LicenseRef-Slint-Royalty-free-2.0.md");
        slint.sort();
        let _ = write!(
            text,
            "Slint Royalty-free Desktop, Mobile, and Web Applications License \
            2.0\nUsed by: {}\n\n{}\n\n",
            slint.join(", "),
            fs::read_to_string(license)?
        );
    }
    let missing: Vec<_> = actual.difference(&covered).collect();
    if !missing.is_empty() {
        return Err(format!("crates without license text: {missing:?}").into());
    }
    text.push_str(
        &include_str!("../templates/rust-std-notice.txt").replace("{rust_version}", rust_version),
    );
    if windows {
        text.push('\n');
        text.push_str(include_str!("../templates/microsoft-runtime-notice.txt"));
    }
    Ok(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixtures() -> (Value, Value) {
        let package = json!({"id":"p","name":"product","version":"1","license":"MIT"});
        (
            json!({"crates":[{"package":package}],"licenses":[{"name":"MIT","text":"License text",
                "used_by":[{"crate":package}]}]}),
            json!({"packages":[package],"resolve":{"nodes":[{"id":"p","deps":[]}]}}),
        )
    }
    #[test]
    fn metadata_identity_lookup_rejects_missing_and_ambiguous_packages() {
        let (_, mut metadata) = fixtures();
        assert_eq!(
            identities(&metadata, "product v1\n").unwrap(),
            BTreeSet::from(["p".into()])
        );
        assert!(identities(&metadata, "missing v1\n").is_err());
        let duplicate = metadata["packages"][0].clone();
        metadata["packages"].as_array_mut().unwrap().push(duplicate);
        assert!(identities(&metadata, "product v1\n").is_err());
    }
    #[test]
    fn build_only_crates_are_checked_but_not_printed() {
        let (mut about, mut metadata) = fixtures();
        let package =
            serde_json::json!({"id":"build","name":"build-tool","version":"1","license":"MIT"});
        about["crates"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"package":package}));
        about["licenses"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name":"Build-only \
            license","text":"Build-only\
             text","used_by":[{"crate":package}]}));
        metadata["packages"].as_array_mut().unwrap().push(package);
        let full = BTreeSet::from(["p".into(), "build".into()]);
        let normal = BTreeSet::from(["p".into()]);
        let text = assemble(&about, &metadata, &full, &normal, "1.99.0", false).unwrap();
        assert!(!text.contains("Build-only"));
        about["licenses"].as_array_mut().unwrap().pop();
        assert!(assemble(&about, &metadata, &full, &normal, "1.99.0", false).is_err());
    }
    #[test]
    fn license_and_dependency_coverage_have_independent_negative_controls() {
        let (mut about, metadata) = fixtures();
        assert!(
            assemble(
                &about,
                &metadata,
                &BTreeSet::from(["p".into()]),
                &BTreeSet::from(["p".into()]),
                "1.99.0",
                false
            )
            .unwrap()
            .contains("License text")
        );
        about["licenses"] = json!([]);
        assert!(
            assemble(
                &about,
                &metadata,
                &BTreeSet::from(["p".into()]),
                &BTreeSet::from(["p".into()]),
                "1.99.0",
                false
            )
            .unwrap_err()
            .to_string()
            .contains("without license text")
        );
        let (mut about, metadata) = fixtures();
        about["crates"]
            .as_array_mut()
            .unwrap()
            .push(json!({"package":{"id":"extra"}}));
        assert!(
            assemble(
                &about,
                &metadata,
                &BTreeSet::from(["p".into()]),
                &BTreeSet::from(["p".into()]),
                "1.99.0",
                false
            )
            .unwrap_err()
            .to_string()
            .contains("absent from metadata")
        );
        let (about, mut metadata) = fixtures();
        metadata["resolve"]["nodes"][0]["deps"] =
            json!([{"pkg":"missing","dep_kinds":[{"kind":null}]}]);
        metadata["resolve"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"missing","deps":[]}));
        assert!(
            assemble(
                &about,
                &metadata,
                &BTreeSet::from(["p".into(), "missing".into()]),
                &BTreeSet::from(["p".into()]),
                "1.99.0",
                false
            )
            .is_err()
        );
    }
}
