//! Persistent CLI through the production executable and real filesystem/processes.
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};
#[expect(
    clippy::unwrap_used,
    reason = "CLI fixture creation must fail the calling integration test"
)]
fn dir() -> tempfile::TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
#[expect(
    clippy::unwrap_used,
    reason = "Production command failure is inspected by each calling test"
)]
fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .arg("--library-dir")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
#[expect(
    clippy::unwrap_used,
    reason = "Machine output must be one valid JSON object, otherwise the calling test fails"
)]
fn json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
#[expect(
    clippy::unwrap_used,
    reason = "A bounded external schema-4 fixture with unknown history must construct"
)]
fn input(path: &Path, title: &str) -> auditorylab::core::Recipe {
    let mut draft = auditorylab::editing::Draft::new().unwrap();
    draft.set_title(title.into());
    draft.add(auditorylab::core::CarrierKind::Clicks).unwrap();
    let id = draft.components()[0].id;
    let mut editor = auditorylab::editing::EditorSession::new(draft).unwrap();
    let field = editor
        .handle(auditorylab::editing::EditingField::Parameter(
            id,
            auditorylab::core::ParameterId::ClickRate,
        ))
        .unwrap();
    editor.commit_field(field, "2").unwrap().unwrap();
    let recipe = editor.draft().recipe();
    fs::write(path, recipe.to_toml().unwrap()).unwrap();
    recipe
}
#[test]
fn root_precedence_path_and_builtin_only_reads_never_initialize() {
    let temp = dir();
    let flag = temp.path().join("flag");
    let environment = temp.path().join("environment");
    let out = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .env("AUDITORYLAB_LIBRARY_DIR", &environment)
        .arg("--library-dir")
        .arg(&flag)
        .args(["library", "path", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(json(&out)["root"]["path"], flag.to_string_lossy().as_ref());
    assert!(!flag.exists() && !environment.exists());
    let out = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .env("AUDITORYLAB_LIBRARY_DIR", &environment)
        .args(["library", "path", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        json(&out)["root"]["path"],
        environment.to_string_lossy().as_ref()
    );
    assert!(!environment.exists());
    let builtin = cli(
        &flag,
        &["list", "--collection", "builtin", "--format", "ids"],
    );
    assert!(builtin.status.success());
    assert!(
        String::from_utf8_lossy(&builtin.stdout)
            .lines()
            .all(|line| line.starts_with("builtin:"))
    );
    assert!(!flag.exists());
    let missing = cli(&flag, &["list", "--collection", "user", "--format", "ids"]);
    assert_eq!(missing.status.code(), Some(5));
    assert!(missing.stdout.is_empty());
    assert!(!flag.exists());
    let relative = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["--library-dir", "relative", "library", "path"])
        .output()
        .unwrap();
    assert_eq!(relative.status.code(), Some(2));
}
#[test]
fn native_cli_lifecycle_conflicts_dates_export_and_trash_are_shared_store_operations() {
    let temp = dir();
    let root = temp.path().join("library");
    let file = temp.path().join("external.toml");
    let original = input(&file, "My Delta sound");
    let bytes = fs::read(&file).unwrap();
    let key = format!("user:{}", original.document.id);
    assert!(cli(&root, &["library", "init"]).status.success());
    let add = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(add.status.success(), "{add:?}");
    let added = json(&add);
    assert_eq!(added["output_schema_version"], 2);
    assert_eq!(added["status"], "published");
    assert_eq!(added["entries"][0]["created_at"], serde_json::Value::Null);
    assert_eq!(added["entries"][0]["modified_at"], serde_json::Value::Null);
    assert_eq!(
        added["entries"][0]["nominal"]["bands"],
        serde_json::json!(["delta"])
    );
    let token = added["version_token"].as_str().unwrap();
    let added_at = added["entries"][0]["added_at"].clone();
    let duplicate = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(duplicate.status.success());
    assert_eq!(json(&duplicate)["status"], "unchanged");
    assert_eq!(json(&duplicate)["version_token"], token);
    let list = cli(
        &root,
        &[
            "list",
            "--collection",
            "user",
            "--band",
            "delta",
            "--method",
            "clicks",
            "--format",
            "ids",
        ],
    );
    assert!(list.status.success());
    assert_eq!(String::from_utf8(list.stdout).unwrap(), format!("{key}\n"));
    let rename = cli(
        &root,
        &[
            "library",
            "rename",
            &key,
            "--title",
            "A renamed sound",
            "--expect",
            token,
            "--format",
            "json",
        ],
    );
    assert!(rename.status.success(), "{rename:?}");
    let renamed = json(&rename);
    let next = renamed["version_token"].as_str().unwrap();
    assert_ne!(next, token);
    assert_eq!(renamed["entries"][0]["created_at"], serde_json::Value::Null);
    assert!(renamed["entries"][0]["modified_at"].is_string());
    assert_eq!(renamed["entries"][0]["added_at"], added_at);
    let stale = cli(
        &root,
        &[
            "library",
            "update",
            &key,
            "--file",
            file.to_str().unwrap(),
            "--expect",
            token,
            "--format",
            "json",
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(json(&stale)["status"], "conflict");
    let export = cli(&root, &["recipe", &key]);
    assert!(export.status.success());
    let exported = auditorylab::core::Recipe::from_bytes(&export.stdout).unwrap();
    assert_eq!(exported.document.id, original.document.id);
    assert!(exported.to_toml().unwrap().contains("A renamed sound"));
    assert!(!String::from_utf8_lossy(&export.stdout).contains("added_at"));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    let inspect = cli(&root, &["inspect", &key, "--format", "json"]);
    assert!(inspect.status.success(), "{inspect:?}");
    assert_eq!(
        json(&inspect)["entries"][0]["validation"]["sample_rate_hz"],
        48000
    );
    assert_eq!(
        json(&inspect)["entries"][0]["validation"]["components"][0]["facts"]["carrier"]["rate_hz"],
        2.0
    );
    let trash = cli(
        &root,
        &[
            "library", "trash", &key, "--expect", next, "--format", "json",
        ],
    );
    assert!(trash.status.success());
    let trashed = json(&trash);
    let trash_token = trashed["version_token"].as_str().unwrap();
    assert_eq!(
        trashed["entries"][0]["modified_at"],
        renamed["entries"][0]["modified_at"]
    );
    assert_eq!(trashed["entries"][0]["added_at"], added_at);
    let active = cli(&root, &["list", "--collection", "user", "--format", "ids"]);
    assert!(active.status.success());
    assert!(active.stdout.is_empty());
    let trash_list = cli(&root, &["library", "list-trash", "--format", "ids"]);
    assert_eq!(
        String::from_utf8(trash_list.stdout).unwrap(),
        format!("{key}\n")
    );
    let restored = cli(
        &root,
        &[
            "library",
            "restore",
            &key,
            "--expect",
            trash_token,
            "--format",
            "json",
        ],
    );
    assert!(restored.status.success());
    let restored = json(&restored);
    assert_eq!(restored["entries"][0]["added_at"], added_at);
    assert_eq!(
        restored["entries"][0]["modified_at"],
        renamed["entries"][0]["modified_at"]
    );
    assert_eq!(
        restored["entries"][0]["trashed_at"],
        serde_json::Value::Null
    );
    let again = cli(
        &root,
        &[
            "library",
            "trash",
            &key,
            "--expect",
            restored["version_token"].as_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(again.status.success());
    let again = json(&again);
    let trash_token = again["version_token"].as_str().unwrap();
    let no_confirm = cli(
        &root,
        &[
            "library",
            "purge",
            &key,
            "--expect",
            trash_token,
            "--format",
            "json",
        ],
    );
    assert_eq!(no_confirm.status.code(), Some(2));
    let purged = cli(
        &root,
        &[
            "library",
            "purge",
            &key,
            "--expect",
            trash_token,
            "--yes",
            "--format",
            "json",
        ],
    );
    assert!(purged.status.success());
    assert_eq!(json(&purged)["status"], "published");
    assert_eq!(json(&purged)["version_token"], serde_json::Value::Null);
    assert_eq!(fs::read(file).unwrap(), bytes);
}
#[test]
fn incomplete_json_keeps_healthy_rows_but_ids_emit_no_stdout_and_busy_is_distinct() {
    let temp = dir();
    let root = temp.path().join("library");
    assert!(cli(&root, &["library", "init"]).status.success());
    let file = temp.path().join("input.toml");
    input(&file, "healthy");
    assert!(
        cli(&root, &["library", "add", "--file", file.to_str().unwrap()])
            .status
            .success()
    );
    let bad = root.join("recipes/00000000-0000-4000-8000-000000000099.toml");
    fs::write(&bad, b"corrupt bytes retained").unwrap();
    let partial = cli(&root, &["list", "--collection", "user", "--format", "json"]);
    assert_eq!(partial.status.code(), Some(5));
    let partial = json(&partial);
    assert_eq!(partial["complete"], false);
    assert_eq!(partial["unique_count"], 1);
    assert!(!partial["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(fs::read(&bad).unwrap(), b"corrupt bytes retained");
    let ids = cli(&root, &["list", "--collection", "user", "--format", "ids"]);
    assert_eq!(ids.status.code(), Some(5));
    assert!(ids.stdout.is_empty());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join(".library.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let busy = cli(&root, &["list", "--collection", "user", "--format", "ids"]);
    assert_eq!(busy.status.code(), Some(4));
    assert!(busy.stdout.is_empty());
    drop(lock);
    assert_eq!(
        cli(&root, &["list", "--format", "ids", "--group-by", "band"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        cli(
            &root,
            &[
                "list",
                "--collection",
                "user",
                "--file",
                file.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(2)
    );
}
#[test]
fn copy_of_reserved_builtin_is_explicit_and_machine_output_escapes_bidi_controls() {
    let temp = dir();
    let root = temp.path().join("library");
    assert!(cli(&root, &["library", "init"]).status.success());
    let file = temp.path().join("builtin.toml");
    let source = auditorylab::recipe::builtin("ioannou-2015-binaural-2-hz").unwrap();
    fs::write(
        &file,
        auditorylab::recipe::builtin_source("ioannou-2015-binaural-2-hz").unwrap(),
    )
    .unwrap();
    let refused = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(refused.status.code(), Some(3));
    let copy = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            file.to_str().unwrap(),
            "--as-copy",
            "--format",
            "json",
        ],
    );
    assert!(copy.status.success());
    let copied = json(&copy);
    assert_ne!(
        copied["entries"][0]["document_id"],
        source.recipe().document.id.to_string()
    );
    assert!(
        copied["entries"][0]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["scope"] == "ancestor")
    );
    let key = copied["selector"].as_str().unwrap();
    let token = copied["version_token"].as_str().unwrap();
    let renamed = cli(
        &root,
        &[
            "library",
            "rename",
            key,
            "--title",
            "Rīga \u{202e}controls",
            "--expect",
            token,
            "--format",
            "json",
        ],
    );
    assert!(renamed.status.success());
    assert!(!String::from_utf8_lossy(&renamed.stdout).contains('\u{202e}'));
    assert_eq!(
        json(&renamed)["entries"][0]["title"],
        "Rīga \u{202e}controls"
    );
}
#[test]
fn lost_completion_pipe_does_not_undo_or_misreport_committed_mutation() {
    let temp = dir();
    let root = temp.path().join("library");
    assert!(cli(&root, &["library", "init"]).status.success());
    let file = temp.path().join("input.toml");
    let recipe = input(&file, "pipe outcome");
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .arg("--library-dir")
        .arg(&root)
        .args(["library", "add", "--file"])
        .arg(&file)
        .args(["--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Operation confirmed"));
    let described = cli(
        &root,
        &[
            "describe",
            &format!("user:{}", recipe.document.id),
            "--format",
            "json",
        ],
    );
    assert!(described.status.success());
}
#[test]
fn cli_usage_rejects_inverse_dates_unsupported_schema_and_legacy_plain_selectors() {
    let temp = dir();
    let root = temp.path().join("never-created");
    assert_eq!(
        cli(
            &root,
            &[
                "list",
                "--collection",
                "builtin",
                "--created-from",
                "2026-10-24T00:00:00Z",
                "--created-before",
                "2026-10-24T00:00:00Z"
            ]
        )
        .status
        .code(),
        Some(2)
    );
    assert!(!root.exists());
    assert_eq!(
        cli(&root, &["describe", "ioannou-2015-binaural-2-hz"])
            .status
            .code(),
        Some(2)
    );
    let bad = temp.path().join("unsupported.toml");
    fs::write(
        &bad,
        b"schema_version=2\nduration_seconds=1\nfade_seconds=0\n",
    )
    .unwrap();
    assert_eq!(
        cli(
            &root,
            &[
                "library",
                "add",
                "--file",
                bad.to_str().unwrap(),
                "--format",
                "json"
            ]
        )
        .status
        .code(),
        Some(2)
    );
    assert!(!root.exists());
}

#[test]
fn two_ordinary_cli_processes_cannot_overwrite_each_others_expected_version() {
    let temp = dir();
    let root = temp.path().join("library");
    let file = temp.path().join("external.toml");
    let original = input(&file, "Two ordinary writers");
    let key = format!("user:{}", original.document.id);
    assert!(cli(&root, &["library", "init"]).status.success());
    let added = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(added.status.success());
    let added = json(&added);
    let token = added["version_token"].as_str().unwrap();
    let spawn = |title: &str| {
        Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .arg("--library-dir")
            .arg(&root)
            .args([
                "library", "rename", &key, "--title", title, "--expect", token, "--format", "json",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = spawn("First writer");
    let second = spawn("Second writer");
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    let codes = [first.status.code().unwrap(), second.status.code().unwrap()];
    assert_eq!(
        codes.iter().filter(|&&code| code == 0).count(),
        1,
        "{first:?}\n{second:?}"
    );
    assert!(codes.iter().all(|code| [0, 3, 4].contains(code)));
    let saved = cli(&root, &["describe", &key, "--format", "json"]);
    assert!(saved.status.success());
    let title = json(&saved)["entries"][0]["title"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(title == "First writer" || title == "Second writer");
    let stale_noop = cli(
        &root,
        &[
            "library", "rename", &key, "--title", &title, "--expect", token, "--format", "json",
        ],
    );
    assert_eq!(stale_noop.status.code(), Some(3));
    assert_eq!(
        fs::read(&file).unwrap(),
        original.to_toml().unwrap().as_bytes()
    );
}

#[test]
fn optional_expect_plain_recipe_bytes_and_unknown_trash_dates_use_the_new_layout() {
    let temp = dir();
    let root = temp.path().join("library");
    let input_path = temp.path().join("input.toml");
    let recipe = input(&input_path, "Direct recipe bytes");
    let key = format!("user:{}", recipe.document.id);
    assert!(cli(&root, &["library", "init"]).status.success());
    assert!(root.join("library.toml").is_file());
    assert!(root.join("membership.toml").is_file());
    assert!(root.join(".stage").is_dir());
    let added = cli(
        &root,
        &[
            "library",
            "add",
            "--file",
            input_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(added.status.success());
    let token = json(&added)["version_token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    assert!(
        token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    let file = root
        .join("recipes")
        .join(format!("{}.toml", recipe.document.id));
    let mut manual = fs::read(&file).unwrap();
    manual.extend_from_slice(b"\n# direct file comment\n");
    fs::write(&file, &manual).unwrap();
    let exported = cli(&root, &["recipe", &key]);
    assert!(exported.status.success());
    assert_eq!(exported.stdout, manual);
    let renamed = cli(
        &root,
        &[
            "library",
            "rename",
            &key,
            "--title",
            "Optional token rename",
            "--format",
            "json",
        ],
    );
    assert!(renamed.status.success(), "{renamed:?}");
    let stale = cli(
        &root,
        &[
            "library",
            "rename",
            &key,
            "--title",
            "Stale must conflict",
            "--expect",
            &token,
        ],
    );
    assert_eq!(stale.status.code(), Some(3));
    assert!(cli(&root, &["library", "trash", &key]).status.success());
    fs::remove_file(root.join("membership.toml")).unwrap();
    let trash = cli(&root, &["library", "list-trash", "--format", "json"]);
    assert!(trash.status.success());
    let trash = json(&trash);
    assert_eq!(trash["entries"][0]["trashed"], true);
    assert_eq!(trash["entries"][0]["trashed_at"], serde_json::Value::Null);
    assert!(cli(&root, &["library", "restore", &key]).status.success());
    assert!(file.is_file());
    assert!(cli(&root, &["library", "trash", &key]).status.success());
    assert!(
        cli(&root, &["library", "purge", &key, "--yes"])
            .status
            .success()
    );
    assert!(!file.exists());
    assert!(
        !root
            .join("trash")
            .join(format!("{}.toml", recipe.document.id))
            .exists()
    );
}

#[test]
fn inspection_serializes_expanded_identity_without_conflating_authored_owners() {
    let root = dir();
    let result = cli(
        root.path(),
        &[
            "inspect",
            "builtin:ioannou-2015-binaural-10-hz",
            "--format",
            "json",
        ],
    );
    assert!(result.status.success(), "{result:?}");
    let report = json(&result);
    assert_eq!(report["output_schema_version"], 2);
    let voices = &report["entries"][0]["validation"]["components"];
    assert_eq!(
        voices[0]["id"],
        serde_json::json!({"component": 1, "lane": "primary"})
    );
    assert_eq!(
        voices[1]["id"],
        serde_json::json!({"component": 1, "lane": "secondary"})
    );
}
