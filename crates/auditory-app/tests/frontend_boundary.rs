// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Required architecture contract: roles, admitted sources, and GUI compiler inputs.
#![expect(clippy::unwrap_used, reason = "test fixtures and assertions")]
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[path = "frontend_boundary/gui.rs"]
mod gui;
#[path = "frontend_boundary/roles.rs"]
mod roles;
#[path = "frontend_boundary/source.rs"]
mod source;
#[test]
fn delivered_frontends_obey_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    gui::system_font_build_policy(&root).unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version=1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    source::workspace_sources(&metadata, &root).unwrap();
    roles::resolved_runtime_profile(&metadata).unwrap();
    let frontends = roles::dependencies(&metadata).unwrap();
    assert!(
        !frontends.is_empty(),
        "workspace unexpectedly has no frontend"
    );
    for package in frontends {
        source::check_sources(package).unwrap();
        if package["name"] == "auditory-gui" {
            gui::build_script(package).unwrap();
            let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
            let workspace: toml::Value =
                toml::from_str(&std::fs::read_to_string(workspace).unwrap()).unwrap();
            let manifest = Path::new(package["manifest_path"].as_str().unwrap());
            let contents: toml::Value =
                toml::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
            gui::lints(&workspace, &contents).unwrap();
            let root = manifest.parent().unwrap().canonicalize().unwrap();
            gui::markup_tree(&root).unwrap();
            gui::compiler_inputs(
                &root,
                Path::new(metadata["target_directory"].as_str().unwrap()),
                Some(&pinned_widget_root(&metadata).unwrap()),
            )
            .unwrap();
        }
    }
}

fn pinned_widget_root(metadata: &Value) -> Result<PathBuf, String> {
    let candidates: Vec<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| {
            package["name"] == "i-slint-compiler"
                && package["version"] == "1.18.1"
                && package["source"]
                    .as_str()
                    .is_some_and(|source| source.starts_with("registry+"))
        })
        .collect();
    if candidates.len() != 1 {
        return Err("expected one pinned registry i-slint-compiler 1.18.1 for standard-widget input validation".into());
    }
    Path::new(candidates[0]["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .join("widgets")
        .canonicalize()
        .map_err(|error| error.to_string())
}
