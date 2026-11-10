//! Controlled configured sources exercise default/environment policy without global environment changes.
use super::*;
use crate::document::editing::{EditingField, MetadataField};
use crate::workspace::{
    Capabilities, Command, EditorCommand, Effect, FileCommand, JobKind, View, Workspace,
};
fn drain(workspace: &mut Workspace) {
    for _ in 0..100 {
        let mut ran = false;
        for effect in workspace.take_effects() {
            if let Effect::Run(job) = effect {
                assert_eq!(job.kind(), JobKind::File);
                workspace.respond(job.execute(|_| panic!("no audio")));
                ran = true;
            }
        }
        workspace.poll();
        if !ran && !workspace.file_busy() {
            return;
        }
    }
    panic!("file work did not settle");
}
#[test]
fn missing_default_first_save_creates_only_on_execution_but_environment_override_requires_setup() {
    for source in [RootSource::Default, RootSource::Environment] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library");
        let library =
            Library::from_root(LibraryRoot::configured(path.clone(), source).unwrap()).unwrap();
        let mut workspace = Workspace::with_library(
            crate::services::catalog().unwrap(),
            Capabilities::no_device_playback(),
            library.clone(),
        )
        .unwrap();
        drain(&mut workspace);
        workspace.apply(Command::SetView(View::Workbench)).unwrap();
        let field = workspace
            .editor()
            .handle(EditingField::Metadata(MetadataField::Title))
            .unwrap();
        workspace
            .apply(Command::Editor(EditorCommand::Stage {
                field,
                text: "First managed save".into(),
            }))
            .unwrap();
        let document = workspace.editor().document();
        for _ in 0..3 {
            assert_eq!(
                workspace.action_view().can_save(),
                source == RootSource::Default
            );
        }
        assert!(!path.exists());
        let result = workspace.apply(Command::File(FileCommand::Save));
        assert_eq!(result.is_ok(), source == RootSource::Default);
        assert!(
            !path.exists(),
            "admission itself does not initialize a root"
        );
        if source == RootSource::Default {
            drain(&mut workspace);
            assert!(path.join("library.toml").is_file());
            assert_eq!(library.scan(&Cancel::default()).entries.len(), 1);
        }
        assert_eq!(workspace.editor().document(), document);
    }
}
