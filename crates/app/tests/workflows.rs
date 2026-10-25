//! Public loading and publication contracts.
#![cfg(test)]
use auditorylab::core::SampleRate;
use auditorylab::error::Error;
use auditorylab::error::ErrorKind;
use auditorylab::identity::version_line;
use auditorylab::recipe::RecipeSnapshot;
use auditorylab::recipe::Rendition;
use auditorylab::recipe::catalog;
use auditorylab::recipe::load_file;
use auditorylab::recipe::load_reader;
use auditorylab::workflow::Cancel;
use auditorylab::workflow::Progress;
use auditorylab::workflow::Replace;
use auditorylab::workflow::export;
use std::{fs, path::Path};
fn dir() -> tempfile::TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
const SILENCE: &str = "schema_version=3\nduration_seconds=0.01\nfade_seconds=0\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n";
#[test]
fn bounded_input_and_regular_file_admission() {
    let mut bytes = SILENCE.as_bytes().to_vec();
    bytes.resize(65_536, b' ');
    assert!(load_reader(&bytes[..], "limit").is_ok());
    bytes.push(b' ');
    assert_eq!(
        load_reader(&bytes[..], "limit").unwrap_err().kind(),
        ErrorKind::InvalidRecipe
    );
    for bytes in [
        &[0xff][..],
        &[0xff, 0xfe][..],
        b"schema_version=3",
        b"schema_version=3\nunknown=1",
    ] {
        assert_eq!(
            load_reader(bytes, "bad").unwrap_err().kind(),
            ErrorKind::InvalidRecipe
        );
    }
    let dir = dir();
    assert_eq!(
        load_file(dir.path()).unwrap_err().kind(),
        ErrorKind::Unreadable
    );
    let path = dir.path().join("sound.toml");
    fs::write(&path, SILENCE).unwrap();
    assert!(load_file(&path).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(load_file(&link).is_ok());
        let fifo = dir.path().join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(load_file(&fifo).unwrap_err().kind(), ErrorKind::Unreadable);
    }
}
#[test]
fn public_export_progress_no_clobber_replace_and_cancellation() {
    let dir = dir();
    let path = dir.path().join("silence.wav");
    let sound = load_reader(SILENCE.as_bytes(), "silence")
        .unwrap()
        .at(SampleRate::DEFAULT)
        .unwrap();
    let progress = Progress::default();
    let result = export(&sound, &path, Replace::Never, &Cancel::default(), &progress).unwrap();
    assert_eq!(result.frames, 480);
    assert_eq!(progress.frames(), 480);
    let original = fs::read(&path).unwrap();
    assert_eq!(
        export(&sound, &path, Replace::Never, &Cancel::default(), &progress)
            .unwrap_err()
            .kind(),
        ErrorKind::DestinationExists
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::write(&path, b"old").unwrap();
    export(
        &sound,
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
        export(&sound, &other, Replace::Never, &cancelled, &progress)
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert!(!other.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn shared_values_cross_threads() {
    fn shared<T: Send + Sync + 'static>() {}
    shared::<RecipeSnapshot>();
    shared::<Rendition>();
    shared::<Error>();
    shared::<Cancel>();
    shared::<Progress>();
    assert!(!catalog().unwrap().is_empty());
    assert_eq!(
        version_line("auditorylab").split_whitespace().next(),
        Some("auditorylab")
    );
}

#[test]
fn untitled_file_names_are_concise_and_origins_remain_complete() {
    let plain =
        auditorylab::core::Recipe::parse("schema_version=3\nduration_seconds=1\nfade_seconds=0\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n")
            .unwrap();
    for (path, name, suggested) in [
        (
            "/deep/Rīga 🎧/huge-cancel.toml",
            "huge-cancel",
            "huge-cancel.wav",
        ),
        ("/deep/line\nbreak.toml", "line\\nbreak", "line break.wav"),
        ("/deep/symlink.toml", "symlink", "symlink.wav"),
    ] {
        let snapshot = auditorylab::recipe::RecipeSnapshot::new(
            auditorylab::recipe::Origin::File(path.into()),
            plain.clone(),
        );
        assert_eq!(snapshot.title(), name);
        assert_eq!(snapshot.suggested_output_name("wav"), suggested);
        assert!(
            auditorylab::presentation::recipe_details(
                &snapshot,
                auditorylab::core::SampleRate::DEFAULT
            )
            .contains("/deep/")
        );
        let draft = auditorylab::editing::Draft::from_snapshot(&snapshot).unwrap();
        assert_eq!(draft.title(), format!("{name} (edited)"));
    }
}
