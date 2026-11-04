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
/// Where a third-party component's published source can be obtained.
fn source_location(package: &Value, name: &str, version: &str) -> Result<String> {
    match package["source"].as_str() {
        Some("registry+https://github.com/rust-lang/crates.io-index") => {
            Ok(format!("https://crates.io/crates/{name}/{version}"))
        }
        // Patched dependency sources are part of this repository's corresponding source.
        None => Ok("Auditory Lab corresponding source (SOURCE.txt)".into()),
        Some(other) => {
            Err(format!("{name} {version} comes from unsupported source {other}").into())
        }
    }
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
    // Auditory Lab's own crates are covered by LICENSE, not listed as third parties.
    let own: BTreeSet<_> = array(metadata, "workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut covered = BTreeSet::new();
    let mut text = String::from("Third-party notices\n===================\n\n");
    for license in array(about, "licenses")? {
        let mut names = Vec::new();
        let mut sources = Vec::new();
        for user in array(license, "used_by")? {
            let package = &user["crate"];
            let id = string(package, "id")?;
            covered.insert(id.to_owned());
            if normal.contains(id) && !own.contains(id) {
                let (name, version) = (string(package, "name")?, string(package, "version")?);
                names.push(format!("{name} {version}"));
                sources.push(source_location(package, name, version)?);
            }
        }
        names.sort();
        sources.sort();
        if !names.is_empty() {
            // MPL-2.0 §3.2(a): say where each component's Source Code Form is available.
            let source = if license["id"] == "MPL-2.0" {
                format!("Source code: {}\n", sources.join(", "))
            } else {
                String::new()
            };
            let _ = write!(
                text,
                "{}\nUsed by: {}\n{source}\n{}\n\n",
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
            json!({"packages":[package],"workspace_members":[],
                "resolve":{"nodes":[{"id":"p","deps":[]}]}}),
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
    #[test]
    fn own_crates_are_omitted_and_mpl_components_name_their_source() {
        let own = json!({"id":"own","name":"auditorylab-core","version":"1","license":"MPL-2.0"});
        let mpl = json!({"id":"m","name":"option-ext","version":"0.2.0","license":"MPL-2.0",
            "source":"registry+https://github.com/rust-lang/crates.io-index"});
        let about = json!({"crates":[{"package":own},{"package":mpl}],
            "licenses":[{"name":"Mozilla Public License 2.0","id":"MPL-2.0","text":"MPL text",
                "used_by":[{"crate":own},{"crate":mpl}]}]});
        let metadata = json!({"packages":[own,mpl],"workspace_members":["own"]});
        let all = BTreeSet::from(["own".into(), "m".into()]);
        let text = assemble(&about, &metadata, &all, &all, "1.99.0", false).unwrap();
        assert!(text.contains(
            "Used by: option-ext 0.2.0\nSource code: https://crates.io/crates/option-ext/0.2.0\n"
        ));
        assert!(!text.contains("auditorylab-core"), "{text}");
        let mut git = about.clone();
        git["licenses"][0]["used_by"][1]["crate"]["source"] = json!("git+https://example.org/x");
        assert!(assemble(&git, &metadata, &all, &all, "1.99.0", false).is_err());
        let only_own = json!({"crates":[{"package":own}],"licenses":[{"name":"MPL","id":"MPL-2.0",
            "text":"MPL text","used_by":[{"crate":own}]}]});
        let own_set = BTreeSet::from(["own".into()]);
        let text = assemble(&only_own, &metadata, &own_set, &own_set, "1.99.0", false).unwrap();
        assert!(!text.contains("MPL text"), "{text}");
    }
}
