use super::*;
use crate::{document::editing::Draft, storage::load::load_file};
use std::fs;
use std::path::PathBuf;
fn fixture() -> (tempfile::TempDir, PathBuf, RecipeSnapshot) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("recipe.toml");
    let recipe = Draft::new().unwrap().recipe().unwrap();
    fs::write(
        &path,
        format!("# retained formatting\n{}", recipe.to_toml().unwrap()),
    )
    .unwrap();
    let snapshot = load_file(&path).unwrap();
    (temp, path, snapshot)
}
#[test]
fn unchanged_save_preserves_exact_bytes_and_changed_save_keeps_identity_and_creation_history() {
    let (temp, path, snapshot) = fixture();
    let original = fs::read(&path).unwrap();
    let expected = snapshot.external_file().unwrap();
    let same = super::save(expected, &(snapshot.recipe().clone()), &Cancel::default()).unwrap();
    assert_eq!(same.status, MutationStatus::Unchanged);
    assert_eq!(fs::read(&path).unwrap(), original);
    let mut recipe = snapshot.recipe().clone().into_builder();
    recipe.content.description = Some(crate::core::provenance::Description {
        title: "Edited external".into(),
        notes: Some("own notes".into()),
    });
    let outcome = super::save(expected, &(recipe.build().unwrap()), &Cancel::default()).unwrap();
    assert_eq!(
        outcome.recipe.document().id,
        snapshot.recipe().document().id
    );
    assert_eq!(
        outcome.recipe.document().created_at,
        snapshot.recipe().document().created_at
    );
    assert!(outcome.recipe.document().modified_at.is_some());
    assert_eq!(outcome.snapshot().title(), "Edited external");
    assert_eq!(load_file(&path).unwrap().recipe(), outcome.recipe.as_ref());
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".lock")
    }));
}
#[test]
fn stale_bytes_or_same_bytes_in_a_new_file_refuse_even_noop_save() {
    for replacement in [false, true] {
        let (_temp, path, snapshot) = fixture();
        let original = fs::read(&path).unwrap();
        if replacement {
            let other = path.with_extension("other");
            fs::write(&other, &original).unwrap();
            fs::rename(other, &path).unwrap();
        } else {
            let mut changed = original;
            changed.extend_from_slice(b"\n# foreign edit");
            fs::write(&path, changed).unwrap();
        }
        let before = fs::read(&path).unwrap();
        assert_eq!(
            super::save(
                snapshot.external_file().unwrap(),
                &(snapshot.recipe().clone()),
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
            crate::outcome::Outcome::Conflict
        );
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
#[test]
fn history_change_and_cancel_are_refused_without_a_stage_or_file_replacement() {
    let (temp, path, snapshot) = fixture();
    let expected = snapshot.external_file().unwrap();
    let before = fs::read(&path).unwrap();
    let mut changed = snapshot.recipe().clone().into_builder();
    changed.document.created_at = Some("2020-01-01T00:00:00Z".parse().unwrap());
    assert_eq!(
        super::save(expected, &(changed.build().unwrap()), &Cancel::default())
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::HistoryConflict
    );
    let cancel = Cancel::default();
    cancel.request();
    assert_eq!(
        super::save(expected, &(snapshot.recipe().clone()), &cancel)
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::Cancelled
    );
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn changed_symlink_and_fifo_are_never_followed_or_waited_on() {
    use std::{
        os::unix::fs::symlink,
        process::Command,
        time::{Duration, Instant},
    };
    let (temp, path, snapshot) = fixture();
    let expected = snapshot.external_file().unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"secret outside bytes").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(super::save(expected, &(snapshot.recipe().clone()), &Cancel::default()).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"secret outside bytes");
    fs::remove_file(&path).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let start = Instant::now();
    assert!(super::save(expected, &(snapshot.recipe().clone()), &Cancel::default()).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}
