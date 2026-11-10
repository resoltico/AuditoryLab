//! Destination policy through real captured save jobs, without devices or global preferences.
use super::*;
use crate::{document::editing::MetadataField, services::library::LibraryContext};
use std::path::Path;

fn configured(path: &Path) -> Workspace {
    let binding = LibraryBinding::from_context(LibraryContext::resolve(Some(path))).unwrap();
    let mut workspace = Workspace::with_binding(
        crate::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
        binding,
    )
    .unwrap();
    drain_files(&mut workspace);
    workspace.take_effects();
    workspace
}
fn import_external(workspace: &mut Workspace, path: &Path) {
    workspace.apply(Command::File(FileCommand::Import)).unwrap();
    let prompt = workspace
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ChoosePath { prompt, .. } => Some(prompt),
            _ => None,
        })
        .unwrap();
    workspace
        .apply(Command::Answer {
            prompt,
            answer: PromptAnswer::Path(Some(path.into())),
        })
        .unwrap();
    drain_files(workspace);
    workspace
        .apply(Command::Library(LibraryCommand::EditSelected))
        .unwrap();
    workspace.poll();
    workspace.take_effects();
    assert!(workspace.library_data().external.is_some());
    assert_eq!(workspace.view(), View::Workbench);
}
fn stage_title(workspace: &mut Workspace, value: &str) {
    let field = workspace
        .editor()
        .handle(EditingField::Metadata(MetadataField::Title))
        .unwrap();
    workspace
        .apply(Command::Editor(EditorCommand::Stage {
            field,
            text: value.into(),
        }))
        .unwrap();
}
#[test]
fn explicit_missing_or_uninitialized_root_refuses_managed_save_and_copy_without_work_and_allows_setup()
 {
    for existed in [false, true] {
        let temp = temporary_directory();
        let root = temp.path().join("missing library");
        if existed {
            std::fs::create_dir(&root).unwrap();
        }
        let mut workspace = configured(&root);
        workspace.apply(Command::SetView(View::Workbench)).unwrap();
        stage_title(&mut workspace, "Preserved staged title");
        workspace.take_effects();
        let document = workspace.editor().document();
        let next = workspace.next_id;
        for _ in 0..3 {
            let actions = workspace.action_view();
            assert!(!actions.can_save());
            assert!(!actions.can_save_copy());
            assert!(actions.operations.iter().any(|item| item.operation
                == crate::vocabulary::Operation::LibraryInit
                && item.enabled));
        }
        assert_eq!(workspace.next_id, next);
        assert!(matches!(
            workspace.apply(Command::File(FileCommand::Save)),
            Err(Refusal::LibraryUnavailable(_))
        ));
        assert_eq!(workspace.editor().document(), document);
        assert!(workspace.editor().has_staged());
        assert!(!workspace.file_busy());
        assert_eq!(root.exists(), existed);
        if existed {
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        }
        assert!(
            workspace
                .take_effects()
                .iter()
                .all(|effect| !matches!(effect, Effect::Run(_)))
        );
        workspace
            .apply(Command::Library(LibraryCommand::Initialize))
            .unwrap();
        drain_files(&mut workspace);
        assert!(root.join("library.toml").is_file());
        assert_eq!(workspace.editor().document(), document);
        assert!(workspace.editor().has_staged());
        assert!(workspace.action_view().can_save());
        workspace.apply(Command::File(FileCommand::Save)).unwrap();
        drain_files(&mut workspace);
        assert!(!workspace.editor().dirty());
    }
}

#[test]
fn external_save_uses_only_its_captured_file_with_missing_empty_or_unavailable_user_root() {
    for mode in ["missing", "empty", "unavailable"] {
        let temp = temporary_directory();
        let root = temp.path().join("unrelated library");
        if mode == "empty" {
            std::fs::create_dir(&root).unwrap();
        }
        let selected = if mode == "unavailable" {
            Path::new("relative-unavailable-unused-library")
        } else {
            root.as_path()
        };
        let mut workspace = configured(selected);
        let path = temp.path().join("external.toml");
        let original = short_silent_source().recipe().clone();
        std::fs::write(&path, original.to_toml().unwrap()).unwrap();
        import_external(&mut workspace, &path);
        stage_title(&mut workspace, "External captured title");
        assert!(workspace.action_view().can_save(), "{mode}");
        assert!(
            !workspace.action_view().can_save_copy(),
            "{mode}: managed copy needs setup"
        );
        assert!(
            !workspace.action_view().can_add_to_library(),
            "{mode}: managed Add needs setup"
        );
        let document = workspace.editor().document();
        assert!(matches!(
            workspace.apply(Command::File(FileCommand::SaveCopy)),
            Err(Refusal::LibraryUnavailable(_))
        ));
        assert!(matches!(
            workspace.apply(Command::Library(LibraryCommand::AddSelected)),
            Err(Refusal::LibraryUnavailable(_))
        ));
        assert_eq!(workspace.editor().document(), document);
        assert!(!workspace.file_busy());
        workspace.apply(Command::File(FileCommand::Save)).unwrap();
        let activity = workspace.action_view().file_activity.unwrap();
        assert_eq!(activity.path, std::fs::canonicalize(&path).unwrap());
        assert_eq!(
            activity.kind.operation(),
            crate::vocabulary::Operation::Save
        );
        let job = take_file_job(&mut workspace);
        stage_title(&mut workspace, "Later unfinished title");
        workspace.respond(execute_file(job));
        workspace.poll();
        let saved = Recipe::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            saved.description().unwrap().title,
            "External captured title"
        );
        assert_eq!(saved.document().id, original.document().id);
        assert_eq!(saved.document().created_at, original.document().created_at);
        assert!(workspace.editor().dirty());
        assert!(workspace.editor().has_staged());
        if mode == "empty" {
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        } else {
            assert!(!root.exists());
        }
    }
}

#[test]
fn external_save_before_close_does_not_require_an_unused_root() {
    let temp = temporary_directory();
    let root = temp.path().join("missing library");
    let mut workspace = configured(&root);
    let path = temp.path().join("external.toml");
    std::fs::write(&path, short_silent_source().recipe().to_toml().unwrap()).unwrap();
    import_external(&mut workspace, &path);
    stage_title(&mut workspace, "Saved before close");
    workspace.apply(Command::Close).unwrap();
    let prompt = workspace
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::AskUnsaved(prompt) => Some(prompt),
            _ => None,
        })
        .unwrap();
    workspace
        .apply(Command::Answer {
            prompt,
            answer: PromptAnswer::Save,
        })
        .unwrap();
    drain_files(&mut workspace);
    workspace.poll();
    assert!(
        workspace
            .take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::CloseResolved { approved: true }))
    );
    assert_eq!(
        Recipe::from_bytes(&std::fs::read(path).unwrap())
            .unwrap()
            .description()
            .unwrap()
            .title,
        "Saved before close"
    );
    assert!(!root.exists());
}

#[test]
fn external_destination_still_checks_stale_bytes_and_replaced_identity_after_capture() {
    for replace in [false, true] {
        let temp = temporary_directory();
        let root = temp.path().join("missing library");
        let mut workspace = configured(&root);
        let path = temp.path().join("external.toml");
        let original = short_silent_source().recipe().to_toml().unwrap();
        std::fs::write(&path, &original).unwrap();
        import_external(&mut workspace, &path);
        stage_title(&mut workspace, "Must not publish");
        workspace.apply(Command::File(FileCommand::Save)).unwrap();
        let job = take_file_job(&mut workspace);
        let preserved = if replace {
            let replacement = temp.path().join("replacement.toml");
            std::fs::write(&replacement, &original).unwrap();
            std::fs::remove_file(&path).unwrap();
            std::fs::rename(replacement, &path).unwrap();
            original
        } else {
            format!("# concurrent change\n{original}")
        };
        if !replace {
            std::fs::write(&path, &preserved).unwrap();
        }
        workspace.respond(execute_file(job));
        assert!(
            matches!(workspace.files.status(), FileStatus::Library(LibraryStatus::Failed { error, .. })
            if error.outcome() == crate::outcome::Outcome::Conflict)
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), preserved);
        assert!(workspace.editor().dirty());
        assert!(!root.exists());
    }
}

#[test]
fn external_exports_keep_their_own_destinations_without_managed_setup() {
    let temp = temporary_directory();
    let root = temp.path().join("missing library");
    let mut workspace = configured(&root);
    let input = temp.path().join("external.toml");
    std::fs::write(&input, short_silent_source().recipe().to_toml().unwrap()).unwrap();
    import_external(&mut workspace, &input);
    for (command, name) in [
        (FileCommand::ExportRecipe, "portable.toml"),
        (FileCommand::ExportWav, "render.wav"),
    ] {
        let destination = temp.path().join(name);
        workspace.apply(Command::File(command)).unwrap();
        let prompt = workspace
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::ChoosePath { prompt, .. } => Some(prompt),
                _ => None,
            })
            .unwrap();
        workspace
            .apply(Command::Answer {
                prompt,
                answer: PromptAnswer::Path(Some(destination.clone())),
            })
            .unwrap();
        drain_files(&mut workspace);
        assert!(destination.is_file());
        assert!(!root.exists());
    }
}

#[cfg(unix)]
#[test]
fn external_save_still_obeys_actual_destination_permissions_without_an_unused_root() {
    use std::os::unix::fs::PermissionsExt;
    struct RestoreDirectoryPermissions(std::path::PathBuf, std::fs::Permissions);
    impl Drop for RestoreDirectoryPermissions {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, self.1.clone());
        }
    }
    let temp = temporary_directory();
    let root = temp.path().join("missing library");
    let parent = temp.path().join("external");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("recipe.toml");
    let original = short_silent_source().recipe().to_toml().unwrap();
    std::fs::write(&path, &original).unwrap();
    let mut workspace = configured(&root);
    import_external(&mut workspace, &path);
    stage_title(&mut workspace, "Permission-controlled save");
    assert!(workspace.action_view().can_save());
    workspace.apply(Command::File(FileCommand::Save)).unwrap();
    let job = take_file_job(&mut workspace);
    let restore = RestoreDirectoryPermissions(
        parent.clone(),
        std::fs::metadata(&parent).unwrap().permissions(),
    );
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = parent.join("independent-write-control");
    let writable = match std::fs::write(&probe, b"control") {
        Ok(()) => {
            std::fs::remove_file(probe).unwrap();
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => false,
        Err(error) => {
            panic!("permission fixture could not establish native write capability: {error}")
        }
    };
    eprintln!("native destination write control after mode0555: {writable}");
    workspace.respond(execute_file(job));
    drop(restore);
    if writable {
        assert_eq!(
            Recipe::from_bytes(&std::fs::read(&path).unwrap())
                .unwrap()
                .description()
                .unwrap()
                .title,
            "Permission-controlled save"
        );
    } else {
        assert!(
            matches!(workspace.files.status(), FileStatus::Library(LibraryStatus::Failed { error, .. })
            if error.outcome() == crate::outcome::Outcome::Write)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(workspace.editor().dirty());
    }
    assert!(!root.exists());
}

#[test]
fn vanished_managed_root_after_projection_is_refused_by_actual_save_without_recreation() {
    let mut workspace = workspace();
    workspace.apply(Command::SetView(View::Workbench)).unwrap();
    stage_title(&mut workspace, "Retain after vanished root");
    workspace.take_effects();
    assert!(workspace.action_view().can_save());
    let root = workspace.store.path().to_owned();
    std::fs::remove_dir_all(&root).unwrap();
    let document = workspace.editor().document();
    workspace.apply(Command::File(FileCommand::Save)).unwrap();
    let captured = workspace.editor().draft().recipe().unwrap();
    let job = take_file_job(&mut workspace);
    workspace.respond(execute_file(job));
    assert!(matches!(
        workspace.files.status(),
        FileStatus::Library(LibraryStatus::Failed {
            error: crate::services::OperationError::Library(
                crate::services::library::LibraryError::MissingMarkerOnCreate
            ),
            ..
        })
    ));
    assert_eq!(workspace.editor().document(), document);
    assert_eq!(workspace.editor().draft().recipe().unwrap(), captured);
    assert!(workspace.editor().dirty());
    assert!(
        !root.exists(),
        "an explicit root is never recreated by Save"
    );
}
