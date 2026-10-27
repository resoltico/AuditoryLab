//! Public loading and publication contracts.
#![cfg(test)]
#[path = "../../app/tests/common/mod.rs"]
mod common;
use auditorylab_app::core::SampleRate;
use auditorylab_app::document::recipe::RecipeSnapshot;
use auditorylab_app::document::recipe::Rendition;
use auditorylab_app::identity::BUILD_IDENTITY;
use auditorylab_app::job::Cancel;
use auditorylab_app::job::Progress;
use auditorylab_app::job::Replace;
use auditorylab_app::outcome::Outcome;
use auditorylab_app::services::OperationError;
use auditorylab_app::services::catalog;
use auditorylab_app::services::export;
use auditorylab_app::services::open_file;
use auditorylab_app::services::read_source;
use auditorylab_presentation::{BuildIdentityText, RecipeSnapshotText};
use std::{fs, path::Path};
fn dir() -> tempfile::TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
fn silence() -> String {
    common::silent(0.01, 0.0).to_toml().unwrap()
}
#[test]
fn bounded_input_and_regular_file_admission() {
    let mut bytes = silence().as_bytes().to_vec();
    bytes.resize(65_536, b' ');
    assert!(read_source(&bytes[..], "limit").is_ok());
    bytes.push(b' ');
    assert_eq!(
        read_source(&bytes[..], "limit").unwrap_err().outcome(),
        Outcome::InvalidRecipe
    );
    for bytes in [
        &[0xff][..],
        &[0xff, 0xfe][..],
        b"schema_version=4",
        b"schema_version=4\nunknown=1",
    ] {
        assert_eq!(
            read_source(bytes, "bad").unwrap_err().outcome(),
            Outcome::InvalidRecipe
        );
    }
    let dir = dir();
    assert_eq!(
        open_file(dir.path()).unwrap_err().outcome(),
        Outcome::Unreadable
    );
    let path = dir.path().join("sound.toml");
    fs::write(&path, silence()).unwrap();
    assert!(open_file(&path).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(open_file(&link).is_ok());
        let fifo = dir.path().join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(open_file(&fifo).unwrap_err().outcome(), Outcome::Unreadable);
    }
}
#[test]
fn public_export_progress_no_clobber_replace_and_cancellation() {
    let dir = dir();
    let path = dir.path().join("silence.wav");
    let sound = read_source(silence().as_bytes(), "silence")
        .unwrap()
        .at(SampleRate::DEFAULT)
        .unwrap();
    let progress = Progress::default();
    let result = export(
        sound.snapshot(),
        sound.signal().sample_rate(),
        &path,
        Replace::Never,
        &Cancel::default(),
        &progress,
    )
    .unwrap();
    assert_eq!(result.frames, 480);
    assert_eq!(progress.frames(), 480);
    let original = fs::read(&path).unwrap();
    assert_eq!(
        export(
            sound.snapshot(),
            sound.signal().sample_rate(),
            &path,
            Replace::Never,
            &Cancel::default(),
            &progress
        )
        .unwrap_err()
        .outcome(),
        Outcome::DestinationExists
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::write(&path, b"old").unwrap();
    export(
        sound.snapshot(),
        sound.signal().sample_rate(),
        &path,
        Replace::Allowed,
        &Cancel::default(),
        &progress,
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    let cancelled = Cancel::default();
    cancelled.request();
    let other = dir.path().join("cancelled.wav");
    assert_eq!(
        export(
            sound.snapshot(),
            sound.signal().sample_rate(),
            &other,
            Replace::Never,
            &cancelled,
            &progress
        )
        .unwrap_err()
        .outcome(),
        Outcome::Cancelled
    );
    assert!(!other.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn shared_values_cross_threads() {
    fn shared<T: Send + Sync + 'static>() {}
    shared::<RecipeSnapshot>();
    shared::<Rendition>();
    shared::<OperationError>();
    shared::<Cancel>();
    shared::<Progress>();
    assert!(!catalog().unwrap().is_empty());
    assert_eq!(
        BUILD_IDENTITY
            .version_line("auditorylab")
            .split_whitespace()
            .next(),
        Some("auditorylab")
    );
}

#[test]
fn untitled_file_names_are_concise_and_origins_remain_complete() {
    let plain = common::silent(1.0, 0.0);
    for (path, name, suggested) in [
        (
            "/deep/Rīga 🎧/huge-cancel.toml",
            "huge-cancel",
            "huge-cancel.wav",
        ),
        ("/deep/line\nbreak.toml", "line\\nbreak", "line break.wav"),
        ("/deep/symlink.toml", "symlink", "symlink.wav"),
    ] {
        let snapshot = auditorylab_app::document::recipe::RecipeSnapshot::new(
            auditorylab_app::document::recipe::Origin::File(path.into()),
            plain.clone(),
        );
        assert_eq!(snapshot.title(), name);
        assert_eq!(snapshot.suggested_output_name("wav"), suggested);
        assert!(
            snapshot
                .details_at(auditorylab_app::core::SampleRate::DEFAULT)
                .contains("/deep/")
        );
        let draft = auditorylab_app::document::editing::Draft::from_snapshot(&snapshot).unwrap();
        assert_eq!(draft.title(), format!("{name} (edited)"));
    }
}
