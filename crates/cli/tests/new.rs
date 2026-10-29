//! CLI creation exercises the actual executable and shared constructors.
use auditorylab_app::core::{Recipe, SampleRate};
use auditorylab_app::document::construction::CreationKind;
use auditorylab_app::document::editing::{Draft, EditorSession};
use std::process::Command;
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
}
#[test]
fn each_offered_kind_emits_a_complete_current_recipe_with_fresh_identity() {
    for kind in CreationKind::ALL {
        let output = cli().args(["new", kind.key()]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stderr, [] as [u8; 0]);
        let recipe = Recipe::from_bytes(&output.stdout).unwrap();
        assert_eq!(Recipe::SCHEMA_VERSION, 5);
        let mut desktop = EditorSession::new(Draft::new().unwrap()).unwrap();
        desktop
            .add_component(desktop.document_key(), kind.component())
            .unwrap();
        let expected = desktop.draft().recipe().unwrap();
        assert_eq!(recipe.content(), expected.content());
        assert_ne!(recipe.document().id, expected.document().id);
        assert!(recipe.signal(SampleRate::DEFAULT).is_ok());
    }
}
#[test]
fn output_is_no_clobber_and_success_has_no_recipe_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recipe.toml");
    let result = cli()
        .args(["new", "pulsed-tone", "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, [] as [u8; 0]);
    let before = std::fs::read(&path).unwrap();
    assert!(Recipe::from_bytes(&before).is_ok());
    let result = cli()
        .args(["new", "tone", "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
#[test]
fn help_and_invalid_kind_have_independent_usage_outputs() {
    let result = cli().args(["new", "--help"]).output().unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("--output"));
    for args in [
        vec!["new"],
        vec!["new", "isochronic"],
        vec!["new", "tone", "extra"],
    ] {
        let result = cli().args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert_eq!(result.stdout, [] as [u8; 0]);
        assert_ne!(result.stderr, [] as [u8; 0]);
    }
}
