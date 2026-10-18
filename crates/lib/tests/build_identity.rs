// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Controlled identity validation and actual Cargo cache invalidation.
#[path = "../build_metadata.rs"]
mod build_metadata;
use auditorylab::{BuildIdentity, BuildKind, Product};
use std::{fs, path::Path, process::Command};
#[test]
fn qualification_inputs_reject_partial_malformed_and_contradictory_values() {
    use build_metadata::qualification;
    let sha = "a".repeat(40);
    for kind in ["candidate", "release"] {
        assert_eq!(
            qualification(Some(kind), Some(&sha)).unwrap().1,
            Some(sha.as_str())
        );
    }
    assert_eq!(qualification(None, None).unwrap(), ("local", None));
    assert_eq!(qualification(Some("local"), None).unwrap(), ("local", None));
    for (kind, source) in [
        (None, Some(sha.as_str())),
        (Some("local"), Some(sha.as_str())),
        (Some("candidate"), None),
        (Some("release"), None),
        (Some(""), None),
        (Some("unknown"), None),
        (Some("candidate"), Some("")),
        (Some("candidate"), Some("abc")),
    ] {
        assert!(qualification(kind, source).is_err(), "{kind:?} {source:?}");
    }
    for invalid in [
        "a".repeat(39),
        "a".repeat(41),
        "g".repeat(40),
        format!("{}\n", "a".repeat(39)),
    ] {
        assert!(qualification(Some("candidate"), Some(&invalid)).is_err());
    }
}
#[test]
fn one_compiled_app_selects_distinct_products_at_each_call() {
    let cli = BuildIdentity::for_product(Product::Cli);
    let gui = BuildIdentity::for_product(Product::Gui);
    assert_eq!(cli.product().package(), "auditorylab-cli");
    assert_eq!(cli.product().binary(), "auditorylab");
    assert_eq!(gui.product().package(), "auditorylab-gui");
    assert_eq!(gui.product().binary(), "auditorylab-gui");
    assert_eq!(cli.source_revision(), gui.source_revision());
    assert_eq!(cli.kind(), gui.kind());
    assert_eq!(cli.target(), gui.target());
    assert!(!cli.target().is_empty());
    let json: serde_json::Value = serde_json::from_str(&cli.to_json()).unwrap();
    assert_eq!(json["format_version"], 1);
    assert_eq!(json["product"], "cli");
    assert_eq!(json["package"], "auditorylab-cli");
    assert_eq!(json["bin"], "auditorylab");
    if cli.kind() == BuildKind::Local {
        assert!(json["source_revision"].is_null());
        assert!(cli.summary().contains("unqualified"));
    }
}
fn run_fixture(
    root: &Path,
    kind: Option<&str>,
    sha: Option<&str>,
    product: &str,
) -> serde_json::Value {
    let mut command = Command::new(env!("CARGO"));
    command
        .args(["run", "--quiet", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .arg("--")
        .arg(product)
        .env("CARGO_TARGET_DIR", root.join("cache"))
        .env_remove("AUDITORY_BUILD_KIND")
        .env_remove("AUDITORY_SOURCE_REVISION");
    if let Some(kind) = kind {
        command.env("AUDITORY_BUILD_KIND", kind);
    }
    if let Some(sha) = sha {
        command.env("AUDITORY_SOURCE_REVISION", sha);
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("cannot run identity fixture: {error}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid fixture build identity: {error}"))
}
#[test]
fn real_build_script_refreshes_source_kind_and_product_in_one_cargo_cache() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = app.join("../../target");
    fs::create_dir_all(&target).unwrap();
    let directory = tempfile::tempdir_in(&target).unwrap();
    let root = directory.path();
    fs::create_dir(root.join("src")).unwrap();
    // Compile the production identity implementation and production build inputs,
    // without recursively building the workspace or depending on a host Git state.
    for name in ["build.rs", "build_metadata.rs"] {
        fs::copy(app.join(name), root.join(name)).unwrap();
    }
    fs::copy(app.join("src/identity.rs"), root.join("src/identity.rs")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='identity-cache-check'\nversion='0.2.0'\nedition='2024'\n[workspace]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.rs"),"mod identity;fn main(){let product=if std::env::args().nth(1).as_deref()==Some(\"gui\"){identity::Product::Gui}else{identity::Product::Cli};println!(\"{}\",identity::BuildIdentity::for_product(product).to_json());}").unwrap();
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    for (kind, source, expected_kind, product) in [
        (None, None, "local", "cli"),
        (Some("candidate"), Some(a.as_str()), "candidate", "cli"),
        (Some("candidate"), Some(a.as_str()), "candidate", "gui"),
        (Some("candidate"), Some(b.as_str()), "candidate", "gui"),
        (None, None, "local", "cli"),
        (Some("release"), Some(b.as_str()), "release", "gui"),
    ] {
        let identity = run_fixture(root, kind, source, product);
        assert_eq!(identity["build_kind"], expected_kind);
        assert_eq!(
            identity["source_revision"],
            source.map_or(serde_json::Value::Null, |s| s.into())
        );
        assert_eq!(identity["product"], product);
        assert_eq!(
            identity["bin"],
            if product == "cli" {
                "auditorylab"
            } else {
                "auditorylab-gui"
            }
        );
    }
    // Even with populated cache, a newly partial input cannot reuse the old identity.
    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("cache"))
        .env("AUDITORY_BUILD_KIND", "candidate")
        .env_remove("AUDITORY_SOURCE_REVISION")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exactly 40 hexadecimal"));
}
