// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Finite workspace roles, declared edges, and unified runtime capabilities.
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};

pub(super) fn dependencies(metadata: &Value) -> Result<Vec<&Value>, String> {
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("missing workspace members")?;
    let packages: Vec<_> = metadata["packages"]
        .as_array()
        .ok_or("missing packages")?
        .iter()
        .filter(|p| members.contains(&p["id"]))
        .collect();
    let expected = HashSet::from([
        "auditorylab-core",
        "auditory-device",
        "auditory-wav",
        "auditorylab",
        "auditorylab-cli",
        "auditorylab-gui",
    ]);
    let names: HashSet<_> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
    if names != expected || packages.len() != expected.len() || packages.len() != members.len() {
        return Err("workspace roles must classify exactly core, device, WAV, app, CLI and GUI; review every added member explicitly".into());
    }
    let mut frontends = Vec::new();
    for package in &packages {
        let name = package["name"].as_str().ok_or("missing package name")?;
        let remedy = |detail: &str| format!("workspace role {name}: {detail}");
        let targets = package["targets"]
            .as_array()
            .ok_or_else(|| remedy("missing targets"))?;
        if !matches!(name, "auditorylab" | "auditorylab-gui")
            && targets.iter().any(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "custom-build"))
            })
        {
            return Err(remedy("custom build targets are prohibited"));
        }
        for target in targets {
            let kinds = target["kind"]
                .as_array()
                .ok_or_else(|| remedy("missing target kind"))?;
            if kinds.is_empty()
                || !kinds.iter().all(|kind| match kind.as_str() {
                    Some("lib") => matches!(
                        name,
                        "auditorylab-core" | "auditory-device" | "auditory-wav" | "auditorylab"
                    ),
                    Some("bin") => matches!(name, "auditorylab-cli" | "auditorylab-gui"),
                    Some("custom-build") => matches!(name, "auditorylab" | "auditorylab-gui"),
                    Some("test" | "bench") => true,
                    _ => false,
                })
            {
                return Err(remedy(
                    "target kind is outside its reviewed library/frontend/test/build role",
                ));
            }
        }
        let mut app = false;
        for dep in package["dependencies"]
            .as_array()
            .ok_or_else(|| remedy("missing dependencies"))?
        {
            let dep_name = dep["name"]
                .as_str()
                .ok_or_else(|| remedy("missing dependency name"))?;
            let kind = dep["kind"].as_str().unwrap_or("normal");
            let profile = |features: &[&str]| {
                dep["uses_default_features"] == false
                    && dep["features"].as_array().is_some_and(|actual| {
                        actual
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<HashSet<_>>()
                            == features.iter().copied().collect()
                    })
            };
            let allowed = if names.contains(dep_name) {
                let direction = kind == "normal"
                    && match name {
                        "auditory-device" | "auditory-wav" => dep_name == "auditorylab-core",
                        "auditorylab" => matches!(
                            dep_name,
                            "auditorylab-core" | "auditory-device" | "auditory-wav"
                        ),
                        "auditorylab-cli" | "auditorylab-gui" => dep_name == "auditorylab",
                        _ => false,
                    };
                let target = packages.iter().find(|p| p["name"] == dep_name).unwrap();
                let target_path = Path::new(
                    target["manifest_path"]
                        .as_str()
                        .ok_or("missing workspace manifest")?,
                )
                .parent()
                .unwrap();
                let identity = dep["source"].is_null()
                    && dep["path"]
                        .as_str()
                        .is_some_and(|p| Path::new(p) == target_path);
                app |= direction && identity && dep_name == "auditorylab";
                direction && identity
            } else if !dep["source"]
                .as_str()
                .is_some_and(|s| s.starts_with("registry+"))
            {
                false
            } else {
                match (name, kind, dep_name) {
                    ("auditorylab-core", "normal", "libm") => profile(&["force-soft-floats"]),
                    ("auditorylab-core", "normal", "serde") => profile(&["alloc", "derive"]),
                    ("auditorylab-core", "normal", "toml") => profile(&["parse", "serde"]),
                    (
                        "auditorylab-core" | "auditory-device",
                        "dev",
                        "allocation-counter" | "proptest",
                    ) => true,
                    ("auditory-device", "normal", "cpal" | "rtrb") => true,
                    ("auditory-wav", "dev", "tempfile" | "waveadapter") => true,
                    ("auditorylab", "normal", "cpal" | "tempfile") => true,
                    ("auditorylab", "normal", "libc") => dep["target"] == "cfg(unix)",
                    ("auditorylab", "normal", "winapi-util") => dep["target"] == "cfg(windows)",
                    (
                        "auditorylab",
                        "dev",
                        "toml" | "quote" | "proc-macro2" | "serde_json" | "syn",
                    ) => true,
                    ("auditorylab", "dev", "i-slint-compiler") => {
                        dep["req"] == "=1.18.1" && profile(&[])
                    }
                    (
                        "auditorylab-cli",
                        "normal",
                        "ctrlc" | "lexopt" | "unicode-segmentation" | "unicode-width",
                    ) => true,
                    ("auditorylab-cli" | "auditorylab-gui", "dev", "tempfile") => true,
                    ("auditorylab-gui", "dev", "allocation-counter") => true,
                    ("auditorylab-gui", "normal", "slint") => {
                        dep["req"] == "=1.18.1"
                            && profile(&[
                                "std",
                                "compat-1-18",
                                "backend-winit",
                                "renderer-software",
                                "renderer-femtovg-wgpu",
                                "unstable-winit-030",
                                "accessibility",
                                "raw-window-handle-06",
                            ])
                    }
                    ("auditorylab-gui", "normal", "rfd") => profile(&["xdg-portal", "wayland"]),
                    ("auditorylab-gui", "build", "slint-build") => {
                        dep["req"] == "=1.18.1" && profile(&["compat-1-18"])
                    }
                    _ => false,
                }
            };
            if !allowed || !dep["rename"].is_null() {
                return Err(remedy(&format!(
                    "prohibited {kind} dependency {dep_name} (alias {}, target {})",
                    dep["rename"], dep["target"]
                )));
            }
        }
        if matches!(name, "auditorylab-cli" | "auditorylab-gui") {
            if !app {
                return Err(remedy("missing normal auditorylab dependency"));
            }
            frontends.push(*package);
        }
    }
    Ok(frontends)
}

pub(super) fn resolved_runtime_profile(metadata: &Value) -> Result<(), String> {
    let packages = metadata["packages"].as_array().unwrap();
    let package = |name: &str| {
        let found: Vec<_> = packages.iter().filter(|p| p["name"] == name).collect();
        if found.len() == 1 {
            Ok(found[0])
        } else {
            Err(format!("expected one resolved {name} package"))
        }
    };
    let features = |id: &Value| -> Result<HashSet<&str>, String> {
        metadata["resolve"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.iter().find(|node| &node["id"] == id))
            .and_then(|node| node["features"].as_array())
            .map(|features| features.iter().filter_map(Value::as_str).collect())
            .ok_or_else(|| format!("missing resolved features for {id}"))
    };
    let slint = package("slint")?;
    let expected = HashSet::from([
        "std",
        "compat-1-18",
        "backend-winit",
        "renderer-software",
        "renderer-femtovg-wgpu",
        "unstable-winit-030",
        "i-slint-renderer-femtovg",
        "accessibility",
        "raw-window-handle-06",
    ]);
    if slint["version"] != "1.18.1" || features(&slint["id"])? != expected {
        return Err(
            "resolved Slint capabilities differ from the reviewed GPU/software profile".into(),
        );
    }
    let core = package("auditorylab-core")?;
    let math = core["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dep| dep["name"] == "libm" && dep["kind"].is_null() && dep["target"].is_null())
        .ok_or("auditorylab-core must directly select portable libm")?;
    if math["uses_default_features"] != false || math["features"] != json!(["force-soft-floats"]) {
        return Err("auditorylab-core must select libm force-soft-floats without defaults".into());
    }
    let libm = package("libm")?;
    let selected = features(&libm["id"])?;
    // GUI dependencies can unify `default`/`arch`. The maintained override wins
    // over both architecture implementations and unstable intrinsics in libm.
    if libm["version"] != "0.2.16"
        || !selected.contains("force-soft-floats")
        || !selected.is_subset(&HashSet::from(["arch", "default", "force-soft-floats"]))
    {
        return Err(
            "resolved libm must retain the software-math override and reviewed features".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolved_gpu_features_and_math_override_survive_feature_unification() {
        let fixture = json!({"packages":[
            {"name":"slint","id":"slint","version":"1.18.1"},
            {"name":"libm","id":"libm","version":"0.2.16"},
            {"name":"auditorylab-core","id":"core","dependencies":[
                {"name":"libm","kind":null,"target":null,"uses_default_features":false,"features":["force-soft-floats"]}
            ]}
        ],"resolve":{"nodes":[
            {"id":"slint","features":["std","compat-1-18","backend-winit","renderer-software","renderer-femtovg-wgpu","unstable-winit-030","i-slint-renderer-femtovg","accessibility","raw-window-handle-06"]},
            {"id":"libm","features":["arch","default","force-soft-floats"]}
        ]}});
        resolved_runtime_profile(&fixture).unwrap();
        let mut software_only = fixture.clone();
        software_only["resolve"]["nodes"][1]["features"] = json!(["force-soft-floats"]);
        resolved_runtime_profile(&software_only).unwrap();
        for index in [0, 1] {
            let mut missing = fixture.clone();
            missing["resolve"]["nodes"][index]["features"]
                .as_array_mut()
                .unwrap()
                .pop();
            assert!(resolved_runtime_profile(&missing).is_err());
        }
        for (index, extra) in [
            (0, "unstable-wgpu-30"),
            (0, "renderer-skia"),
            (1, "unstable-intrinsics"),
        ] {
            let mut expanded = fixture.clone();
            expanded["resolve"]["nodes"][index]["features"]
                .as_array_mut()
                .unwrap()
                .push(json!(extra));
            assert!(resolved_runtime_profile(&expanded).is_err());
        }
        let mut absent = fixture;
        absent["packages"][2]["dependencies"][0]["features"] = json!([]);
        assert!(resolved_runtime_profile(&absent).is_err());
    }
    fn fixture() -> Value {
        json!({"workspace_members":["app", "cli", "core", "device", "wav", "gui"],"packages":[
        {"id":"app","name":"auditorylab","manifest_path":"/fixture/app/Cargo.toml","targets":[],"dependencies":[]},
        {"id":"core","name":"auditorylab-core","manifest_path":"/fixture/core/Cargo.toml","targets":[],"dependencies":[]},
        {"id":"device","name":"auditory-device","manifest_path":"/fixture/device/Cargo.toml","targets":[],"dependencies":[]},
        {"id":"wav","name":"auditory-wav","manifest_path":"/fixture/wav/Cargo.toml","targets":[],"dependencies":[]},
        {"id":"cli","name":"auditorylab-cli","manifest_path":"/fixture/cli/Cargo.toml","targets":[{"kind":["bin"]}],"dependencies":[{"name":"auditorylab","kind":null,"source":null,"path":"/fixture/app"}]},
{"id":"gui","name":"auditorylab-gui","manifest_path":"/fixture/gui/Cargo.toml","targets":[{"kind":["bin"]}],"dependencies":[{"name":"auditorylab","kind":null,"source":null,"path":"/fixture/app"}]}]})
    }
    #[test]
    fn declared_dependencies_reject_aliases_targets_and_builds() {
        for name in ["auditorylab-core", "auditory-device", "auditory-wav"] {
            for kind in [Value::Null, json!("dev"), json!("build")] {
                let mut metadata = fixture();
                metadata["packages"][4]["dependencies"].as_array_mut().unwrap().push(json!({"name":name,"rename":"innocent","kind":kind,"target":"cfg(windows)","source":null}));
                let failure = dependencies(&metadata).unwrap_err();
                assert!(failure.contains("auditorylab-cli") && failure.contains(name));
            }
        }
        let mut metadata = fixture();
        metadata["packages"][4]["targets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":["custom-build"]}));
        assert!(
            dependencies(&metadata)
                .unwrap_err()
                .contains("custom build")
        );
    }
    #[test]
    fn whole_workspace_roles_reject_wrong_directions_and_unclassified_members() {
        dependencies(&fixture()).unwrap();
        // Only the facade admits native Windows document handles. Neither
        // frontends nor pure/adapter crates gain this producer in any scope.
        for owner in 0..6 {
            for kind in [Value::Null, json!("dev"), json!("build")] {
                for target in [Value::Null, json!("cfg(windows)"), json!("cfg(unix)")] {
                    let mut metadata = fixture();
                    let owner_name = metadata["packages"][owner]["name"]
                        .as_str()
                        .unwrap()
                        .to_owned();
                    let allowed =
                        owner_name == "auditorylab" && kind.is_null() && target == "cfg(windows)";
                    metadata["packages"][owner]["dependencies"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({
                            "name":"winapi-util", "kind":kind, "target":target,
                            "source":"registry+https://github.com/rust-lang/crates.io-index",
                            "req":"^0.1.11", "rename":null
                        }));
                    assert_eq!(
                        dependencies(&metadata).is_ok(),
                        allowed,
                        "{owner_name}: {kind}, {target}"
                    );
                    if allowed {
                        metadata["packages"][owner]["dependencies"]
                            .as_array_mut()
                            .unwrap()
                            .last_mut()
                            .unwrap()["rename"] = json!("innocent");
                        assert!(dependencies(&metadata).is_err());
                    }
                }
            }
        }
        for from in 0..6 {
            for to in 0..6 {
                for kind in [Value::Null, json!("dev"), json!("build")] {
                    let mut metadata = fixture();
                    let target = metadata["packages"][to].clone();
                    let name = target["name"].as_str().unwrap();
                    let owner = metadata["packages"][from]["name"]
                        .as_str()
                        .unwrap()
                        .to_owned();
                    let permitted = kind.is_null()
                        && match owner.as_str() {
                            "auditory-device" | "auditory-wav" => name == "auditorylab-core",
                            "auditorylab" => {
                                matches!(name, "auditorylab-core" | "auditory-device" | "auditory-wav")
                            }
                            "auditorylab-cli" | "auditorylab-gui" => name == "auditorylab",
                            _ => false,
                        };
                    let path = Path::new(target["manifest_path"].as_str().unwrap())
                        .parent()
                        .unwrap();
                    let edge = json!({"name":name,"kind":kind,"source":null,"path":path,"target":"cfg(windows)"});
                    metadata["packages"][from]["dependencies"]
                        .as_array_mut()
                        .unwrap()
                        .push(edge);
                    assert_eq!(
                        dependencies(&metadata).is_ok(),
                        permitted,
                        "{owner} -> {name}, {kind}"
                    );
                    metadata["packages"][from]["dependencies"]
                        .as_array_mut()
                        .unwrap()
                        .last_mut()
                        .unwrap()["rename"] = json!("innocent");
                    assert!(
                        dependencies(&metadata).is_err(),
                        "alias admitted {owner} -> {name}"
                    );
                }
            }
            for kind in [Value::Null, json!("dev"), json!("build")] {
                let mut metadata = fixture();
                metadata["packages"][from]["dependencies"].as_array_mut().unwrap().push(json!({"name":"std-filesystem-helper","kind":kind,"source":"registry+https://example.com","target":"cfg(unix)"}));
                assert!(dependencies(&metadata).is_err());
            }
        }
        for index in 0..6 {
            for kind in [
                "lib",
                "bin",
                "custom-build",
                "test",
                "bench",
                "example",
                "proc-macro",
                "cdylib",
            ] {
                let mut metadata = fixture();
                let owner = metadata["packages"][index]["name"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let allowed = match kind {
                    "lib" => matches!(
                        owner.as_str(),
                        "auditorylab-core" | "auditory-device" | "auditory-wav" | "auditorylab"
                    ),
                    "bin" => matches!(owner.as_str(), "auditorylab-cli" | "auditorylab-gui"),
                    "custom-build" => matches!(owner.as_str(), "auditorylab" | "auditorylab-gui"),
                    "test" | "bench" => true,
                    _ => false,
                };
                metadata["packages"][index]["targets"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind":[kind]}));
                assert_eq!(
                    dependencies(&metadata).is_ok(),
                    allowed,
                    "{owner} {kind} target"
                );
            }
        }
        for (dependency, features) in [
            ("libm", vec!["force-soft-floats"]),
            ("serde", vec!["alloc", "derive"]),
            ("toml", vec!["parse", "serde"]),
        ] {
            let mut metadata = fixture();
            metadata["packages"][1]["dependencies"].as_array_mut().unwrap().push(json!({"name":dependency,"kind":null,"source":"registry+https://example.com","features":features,"uses_default_features":false,"target":"cfg(windows)"}));
            dependencies(&metadata).unwrap();
            metadata["packages"][1]["dependencies"][0]["features"]
                .as_array_mut()
                .unwrap()
                .push(json!("std"));
            assert!(
                dependencies(&metadata).is_err(),
                "core std feature admitted on conditional {dependency}"
            );
        }
        let mut metadata = fixture();
        let mut extra = metadata["packages"][4].clone();
        extra["id"] = json!("extra");
        extra["name"] = json!("unclassified");
        metadata["packages"].as_array_mut().unwrap().push(extra);
        metadata["workspace_members"]
            .as_array_mut()
            .unwrap()
            .push(json!("extra"));
        assert!(dependencies(&metadata).unwrap_err().contains("classify"));
        let mut missing = fixture();
        missing["workspace_members"].as_array_mut().unwrap().pop();
        assert!(dependencies(&missing).is_err());
    }
    #[test]
    fn facade_dependency_requires_workspace_path_identity() {
        for (source, path) in [
            (json!("registry+https://example.com"), Value::Null),
            (Value::Null, json!("/another/app")),
        ] {
            let mut metadata = fixture();
            metadata["packages"][4]["dependencies"][0]["source"] = source;
            metadata["packages"][4]["dependencies"][0]["path"] = path;
            assert!(
                dependencies(&metadata)
                    .unwrap_err()
                    .contains("auditorylab")
            );
        }
    }
    fn gui_metadata_fixture() -> Value {
        let mut metadata = fixture();
        metadata["packages"].as_array_mut().unwrap().swap(4, 5);
        let frontend = &mut metadata["packages"][4];
        frontend["targets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":["custom-build"]}));
        for (name, kind, features) in [
            (
                "slint",
                Value::Null,
                vec![
                    "std",
                    "compat-1-18",
                    "backend-winit",
                    "renderer-software",
                    "renderer-femtovg-wgpu",
                    "unstable-winit-030",
                    "accessibility",
                    "raw-window-handle-06",
                ],
            ),
            ("rfd", Value::Null, vec!["xdg-portal", "wayland"]),
            ("slint-build", json!("build"), vec!["compat-1-18"]),
        ] {
            frontend["dependencies"].as_array_mut().unwrap().push(json!({"name":name,"kind":kind,"source":"registry+https://github.com/rust-lang/crates.io-index","req":if name=="rfd"{"^0.17.2"}else{"=1.18.1"},"uses_default_features":false,"features":features}));
        }
        metadata
    }
    #[test]
    fn gui_dependency_exception_is_product_specific_and_requires_the_reviewed_profile() {
        dependencies(&gui_metadata_fixture()).unwrap();
        for name in [
            "ctrlc",
            "lexopt",
            "auditorylab-core",
            "wgpu",
            "wgpu-30",
            "winit",
            "another-build-helper",
        ] {
            for kind in [Value::Null, json!("dev"), json!("build")] {
                let mut metadata = gui_metadata_fixture();
                metadata["packages"][4]["dependencies"].as_array_mut().unwrap().push(json!({"name":name,"kind":kind,"source":"registry+https://github.com/rust-lang/crates.io-index","rename":"innocent","target":"cfg(windows)"}));
                assert!(
                    dependencies(&metadata).is_err(),
                    "accepted GUI {kind} {name}"
                );
            }
        }
        for name in ["slint", "rfd"] {
            let mut metadata = fixture();
            let dep = gui_metadata_fixture()["packages"][4]["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .find(|dep| dep["name"] == name)
                .unwrap()
                .clone();
            metadata["packages"][4]["dependencies"]
                .as_array_mut()
                .unwrap()
                .push(dep);
            assert!(
                dependencies(&metadata).is_err(),
                "GUI dependency leaked into CLI"
            );
        }
        for index in [1, 3] {
            let mut metadata = gui_metadata_fixture();
            metadata["packages"][4]["dependencies"][index]["req"] = json!("^1.18.1");
            assert!(
                dependencies(&metadata).is_err(),
                "runtime/compiler version not exact"
            );
        }
        let mut metadata = gui_metadata_fixture();
        metadata["packages"][4]["dependencies"][1]["uses_default_features"] = json!(true);
        assert!(dependencies(&metadata).is_err());
        let mut metadata = gui_metadata_fixture();
        metadata["packages"][4]["dependencies"][1]["features"]
            .as_array_mut()
            .unwrap()
            .push(json!("renderer-skia"));
        assert!(dependencies(&metadata).is_err());
    }
    #[test]
    fn gpu_profile_does_not_open_raw_unstable_or_unrelated_renderer_interfaces() {
        for forbidden in [
            "unstable-wgpu-29",
            "unstable-wgpu-30",
            "unstable-winit-028",
            "unstable-winit-029",
            "unstable-winit-031",
            "renderer-femtovg",
            "renderer-skia",
            "backend-qt",
            "static-dxc",
        ] {
            let mut metadata = gui_metadata_fixture();
            metadata["packages"][4]["dependencies"][1]["features"]
                .as_array_mut()
                .unwrap()
                .push(json!(forbidden));
            assert!(
                dependencies(&metadata).is_err(),
                "accepted extra Slint capability {forbidden}"
            );
        }
        let mut metadata = gui_metadata_fixture();
        metadata["packages"][4]["dependencies"][1]["features"]
            .as_array_mut()
            .unwrap()
            .retain(|feature| feature != "renderer-software");
        assert!(
            dependencies(&metadata).is_err(),
            "software recovery renderer was dropped"
        );
    }
}
