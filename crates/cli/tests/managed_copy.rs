//! Actual managed-copy byte limits include final timestamps and publication metadata.
#[path = "../../app/tests/support/copy_boundary.rs"]
mod copy_boundary;
use std::{fs, process::Command};
#[test]
fn cli_managed_copy_accepts_exact_final_bytes_and_refuses_one_more_without_destination() {
    for bytes in [65_536, 65_537] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("library");
        let initialized = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .arg("--library-dir")
            .arg(&root)
            .args(["library", "init"])
            .output()
            .unwrap();
        assert!(initialized.status.success());
        let source = copy_boundary::source_for_final_size(copy_boundary::SOURCE_TITLE, bytes);
        let input = directory.path().join("input.toml");
        let original = source.to_toml().unwrap();
        fs::write(&input, &original).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .arg("--library-dir")
            .arg(&root)
            .args(["library", "add", "--file"])
            .arg(&input)
            .args(["--as-copy", "--format", "json"])
            .output()
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        if bytes == 65_536 {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
            let id = report["entries"][0]["document_id"].as_str().unwrap();
            let destination = root.join("recipes").join(format!("{id}.toml"));
            let copy =
                auditorylab_core::Recipe::from_bytes(&fs::read(destination).unwrap()).unwrap();
            copy_boundary::assert_final_copy(&source, &copy, copy_boundary::SOURCE_TITLE);
        } else {
            assert_eq!(result.status.code(), Some(2));
            assert_eq!(report["diagnostics"][0]["code"], "recipe.size");
            assert_eq!(fs::read_dir(root.join("recipes")).unwrap().count(), 0);
        }
        assert_eq!(fs::read_to_string(input).unwrap(), original);
    }
}
