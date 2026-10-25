//! Reproduce maintained patches from authenticated published archives.
use crate::{Result, capture, run};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn digest(bytes: &[u8], expected: &str) -> Result<()> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        return Err(format!("archive SHA-256 mismatch: {actual}, expected {expected}").into());
    }
    Ok(())
}

fn files(root: &Path, relative: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!("unexpected vendor symlink {path:?}").into());
        }
        if kind.is_dir() {
            // Cargo's own compiler outputs are not dependency source.
            if path != Path::new("target") {
                files(root, &path, out)?;
            }
        } else if path != Path::new(".cargo_vcs_info.json") {
            out.insert(
                path.to_str()
                    .ok_or("non-UTF8 vendor path")?
                    .replace('\\', "/"),
                fs::read(entry.path())?,
            );
        }
    }
    Ok(())
}

fn same_tree(upstream: &Path, vendor: &Path) -> Result<()> {
    let mut expected = BTreeMap::new();
    let mut actual = BTreeMap::new();
    files(upstream, Path::new(""), &mut expected)?;
    files(vendor, Path::new(""), &mut actual)?;
    if expected != actual {
        let changed: Vec<_> = expected
            .keys()
            .chain(actual.keys())
            .filter(|key| expected.get(*key) != actual.get(*key))
            .collect();
        return Err(format!("vendor source differs from reviewed patch: {changed:?}").into());
    }
    Ok(())
}

fn resolved_source(package: &serde_json::Value, version: &str, vendor: &Path) -> bool {
    package["version"] == version
        && package["source"].is_null()
        && package["manifest_path"]
            .as_str()
            .is_some_and(|path| Path::new(path).parent() == Some(vendor))
}

pub(crate) fn verify() -> Result<()> {
    let record: toml::Table = toml::from_str(&fs::read_to_string("third_party/patches.toml")?)?;
    let patches = record["patch"].as_array().ok_or("patch records missing")?;
    fs::create_dir_all("target/tmp")?;
    fs::create_dir_all("target/vendor-review")?;
    let scratch = tempfile::tempdir_in("target/tmp")?;
    let meta = crate::metadata(None)?;
    for patch in patches {
        let field = |key| {
            patch
                .get(key)
                .and_then(toml::Value::as_str)
                .ok_or("invalid patch record")
        };
        let name = field("name")?;
        let version = field("version")?;
        let archive = Path::new("target/vendor-review").join(format!("{name}-{version}.crate"));
        if !archive.exists() {
            run(Command::new("curl")
                .args(["--fail", "--location", "--retry", "3", "--output"])
                .arg(&archive)
                .arg(field("url")?))?;
        }
        let bytes = fs::read(&archive)?;
        digest(&bytes, field("sha256")?)?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
        for entry in tar.entries()? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
                return Err("nonregular upstream archive member".into());
            }
            if !entry.unpack_in(scratch.path())? {
                return Err("unsafe upstream archive path".into());
            }
        }
        let upstream = scratch.path().join(format!("{name}-{version}"));
        let diff =
            fs::canonicalize(Path::new("third_party/patches").join(format!("{name}.patch")))?;
        run(Command::new("git")
            .current_dir(&upstream)
            .args(["apply", "--check"])
            .arg(&diff))?;
        run(Command::new("git")
            .current_dir(&upstream)
            .arg("apply")
            .arg(&diff))?;
        let vendor = fs::canonicalize(Path::new("third_party").join(name))?;
        same_tree(&upstream, &vendor)?;
        let packages = meta["packages"]
            .as_array()
            .ok_or("metadata packages missing")?;
        let matches: Vec<_> = packages.iter().filter(|p| p["name"] == name).collect();
        if matches.len() != 1 || !resolved_source(matches[0], version, &vendor) {
            return Err(format!("wrong resolved Cargo source for {name}").into());
        }
        println!(
            "Verified {name} {version}: archive, exact reviewed patch, source tree and Cargo source"
        );
    }
    for (target, _) in crate::targets::TARGETS {
        for (product, _) in crate::targets::PRODUCTS {
            let package = format!("auditorylab-{product}");
            let tree = String::from_utf8(capture(Command::new("cargo").args([
                "tree",
                "--locked",
                "-p",
                &package,
                "--target",
                target,
                "-e",
                "normal,build",
                "--prefix",
                "none",
                "--format",
                "{p}",
            ]))?)?;
            for patch in patches {
                let name = patch["name"].as_str().ok_or("patch name missing")?;
                let vendor = fs::canonicalize(Path::new("third_party").join(name))?;
                for line in tree
                    .lines()
                    .filter(|line| line.starts_with(&format!("{name} v")))
                {
                    if !line.contains(vendor.to_str().ok_or("vendor path not UTF8")?) {
                        return Err(format!("{package} {target}: patch bypassed: {line}").into());
                    }
                }
            }
            println!("Verified resolved graph {package} {target}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_or_other_checkout_cannot_replace_the_reviewed_source() {
        let mut package = serde_json::json!({"version":"1.0.0", "source":null, "manifest_path":"vendor/Cargo.toml"});
        assert!(resolved_source(&package, "1.0.0", Path::new("vendor")));
        package["source"] = "registry+https://github.com/rust-lang/crates.io-index".into();
        assert!(!resolved_source(&package, "1.0.0", Path::new("vendor")));
        package["source"] = serde_json::Value::Null;
        package["manifest_path"] = "other/Cargo.toml".into();
        assert!(!resolved_source(&package, "1.0.0", Path::new("vendor")));
    }
    #[test]
    fn incorrect_archive_and_unexpected_or_missing_source_fail_closed() {
        assert!(digest(b"changed", &format!("{:x}", Sha256::digest(b"original"))).is_err());
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fs::write(a.path().join("patch.rs"), "reviewed").unwrap();
        assert!(same_tree(a.path(), b.path()).is_err());
        fs::write(b.path().join("patch.rs"), "reviewed").unwrap();
        same_tree(a.path(), b.path()).unwrap();
        fs::write(b.path().join("unexpected.rs"), "unreviewed").unwrap();
        assert!(same_tree(a.path(), b.path()).is_err());
    }
}
