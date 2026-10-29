//! Independent policy states; these tests enqueue work but never start native audio.
use super::*;

#[derive(Debug, Clone, Copy)]
enum Case {
    Idle,
    Prompt,
    PendingFile,
    Import,
    Export,
    ExportAtEnd,
    Background,
    Audio,
    InvalidText,
    AcousticInvalid,
    Full,
    Exhausted,
    RestorableExhausted,
    Unavailable,
    NoSelection,
    FullLineage,
    NoDevice,
    OpenFile,
    OpenFilePrompt,
    OpenFileBusy,
}
impl Case {
    const ALL: [Self; 20] = [
        Self::Idle,
        Self::Prompt,
        Self::PendingFile,
        Self::Import,
        Self::Export,
        Self::ExportAtEnd,
        Self::Background,
        Self::Audio,
        Self::InvalidText,
        Self::AcousticInvalid,
        Self::Full,
        Self::Exhausted,
        Self::RestorableExhausted,
        Self::Unavailable,
        Self::NoSelection,
        Self::FullLineage,
        Self::NoDevice,
        Self::OpenFile,
        Self::OpenFilePrompt,
        Self::OpenFileBusy,
    ];
    #[expect(
        clippy::too_many_lines,
        reason = "Independent data table lists permitted observations for every named fixture case"
    )]
    fn expected(self) -> Expected {
        let idle = Expected::empty()
            .with(&[
                Permission::Play,
                Permission::Import,
                Permission::New,
                Permission::Copy,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
                Permission::Preview,
                Permission::Edit,
                Permission::Add,
                Permission::Pair,
                Permission::Manage,
            ])
            .without(&[
                Permission::Restore,
                Permission::FileBusy,
                Permission::Audio,
                Permission::Cancel,
                Permission::AddFile,
            ]);
        match self {
            Self::Idle => idle,
            Self::Prompt | Self::OpenFilePrompt => idle.without(&[
                Permission::Play,
                Permission::Manage,
                Permission::Import,
                Permission::New,
                Permission::Copy,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
                Permission::Edit,
                Permission::Add,
                Permission::Pair,
            ]),
            Self::PendingFile => idle.with(&[Permission::FileBusy]).without(&[
                Permission::Play,
                Permission::Import,
                Permission::New,
                Permission::Copy,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
                Permission::Edit,
                Permission::Add,
                Permission::Pair,
                Permission::Manage,
            ]),
            Self::Import => idle.with(&[Permission::FileBusy]).without(&[
                Permission::Import,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
                Permission::Edit,
                Permission::Add,
                Permission::Pair,
                Permission::Manage,
            ]),
            Self::Export | Self::ExportAtEnd => idle
                .with(&[Permission::FileBusy, Permission::Cancel])
                .without(&[
                    Permission::Play,
                    Permission::Import,
                    Permission::Save,
                    Permission::SaveCopy,
                    Permission::Export,
                    Permission::Portable,
                    Permission::Edit,
                    Permission::Add,
                    Permission::Pair,
                    Permission::Manage,
                ]),
            Self::Background | Self::OpenFileBusy => idle.with(&[Permission::FileBusy]).without(&[
                Permission::Import,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
                Permission::Manage,
            ]),
            Self::Audio => idle
                .with(&[Permission::Audio])
                .without(&[Permission::Export, Permission::Preview]),
            Self::InvalidText => idle.without(&[
                Permission::Play,
                Permission::Save,
                Permission::SaveCopy,
                Permission::Export,
                Permission::Portable,
            ]),
            Self::AcousticInvalid => idle.without(&[Permission::Export]),
            Self::Full | Self::Exhausted => idle.without(&[Permission::Add, Permission::Pair]),
            Self::RestorableExhausted => idle
                .with(&[Permission::Restore])
                .without(&[Permission::Add, Permission::Pair]),
            Self::Unavailable => {
                idle.without(&[Permission::Save, Permission::Manage, Permission::SaveCopy])
            }
            Self::NoSelection => idle.without(&[
                Permission::Play,
                Permission::Copy,
                Permission::Export,
                Permission::Portable,
            ]),
            Self::FullLineage => idle.without(&[Permission::SaveCopy]),
            Self::OpenFile => idle.with(&[Permission::AddFile]),
            Self::NoDevice => idle.without(&[Permission::Play]),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Permission {
    Play,
    Import,
    New,
    Copy,
    Save,
    SaveCopy,
    Export,
    Portable,
    Preview,
    Edit,
    Add,
    Pair,
    Restore,
    FileBusy,
    Audio,
    Cancel,
    Manage,
    AddFile,
}
struct Expected {
    permitted: std::collections::BTreeSet<Permission>,
}
impl Expected {
    fn empty() -> Self {
        Self {
            permitted: std::collections::BTreeSet::new(),
        }
    }
    fn with(mut self, values: &[Permission]) -> Self {
        self.permitted.extend(values);
        self
    }
    fn without(mut self, values: &[Permission]) -> Self {
        self.permitted.retain(|value| !values.contains(value));
        self
    }
    fn allows(&self, value: Permission) -> bool {
        self.permitted.contains(&value)
    }
}

fn fixture(case: Case) -> TestWorkspace {
    let mut ws = workspace();
    let _ = ws.apply(Command::SetView(View::Workbench));
    match case {
        Case::Idle => {}
        Case::Prompt => ws.apply(Command::File(FileCommand::Import)).unwrap(),
        Case::PendingFile | Case::Import => {
            let path = ws.store.path().join("pending-input.toml");
            ws.start_file(FileKind::Import(path));
            if matches!(case, Case::PendingFile) {
                let _ = ws.apply(Command::Editor(EditorCommand::New));
                assert!(ws.action_pending());
            }
        }
        Case::Export | Case::ExportAtEnd => {
            let sound = ws
                .editor()
                .draft()
                .snapshot()
                .unwrap()
                .at(SampleRate::DEFAULT)
                .unwrap();
            let frames = sound.signal().frames();
            let progress = Progress::default();
            if matches!(case, Case::ExportAtEnd) {
                progress.set(frames);
            }
            let path = ws.store.path().join("pending-output.wav");
            ws.start_file(FileKind::Export {
                title: "silent capture".into(),
                path,
                frames,
                cancel: Cancel::default(),
                progress,
                sound,
            });
        }
        Case::Background => {
            let _ = ws.apply(Command::Library(LibraryCommand::Refresh));
            assert!(ws.file_busy());
        }
        Case::Audio => ws.apply(Command::Audio(AudioCommand::Play)).unwrap(),
        Case::InvalidText => {
            let handle = ws
                .editor()
                .handle(EditingField::Metadata(
                    crate::document::editing::MetadataField::Duration,
                ))
                .unwrap();
            ws.apply(Command::Editor(EditorCommand::Stage {
                field: handle,
                text: ("unparsed duration").to_string(),
            }))
            .unwrap();
        }
        Case::AcousticInvalid
        | Case::Full
        | Case::Exhausted
        | Case::RestorableExhausted
        | Case::FullLineage => {
            prepare_document_case(&mut ws, case);
        }
        Case::Unavailable => {
            ws.library.replace_binding(
                LibraryBinding::from_context(crate::services::LibraryContext::resolve(Some(
                    std::path::Path::new("relative-unavailable-root"),
                )))
                .unwrap(),
            );
        }
        Case::NoSelection => {
            let _ = ws.apply(Command::SetView(View::Library));
            let _ = ws.apply(Command::Library(LibraryCommand::Search(
                ("no catalog source has this title").to_string(),
            )));
            assert!(ws.selected().is_none());
        }
        Case::NoDevice => ws
            .audio
            .set_capabilities(Capabilities::no_device_playback()),
        Case::OpenFile | Case::OpenFilePrompt | Case::OpenFileBusy => {
            let snapshot = RecipeSnapshot::new(
                crate::document::recipe::Origin::Stream("opened input".into()),
                crate::test_recipes::silent(1.0, 0.0),
            );
            ws.add_import(snapshot).unwrap();
            let _ = ws.apply(Command::SetView(View::Library));
            if matches!(case, Case::OpenFilePrompt) {
                ws.apply(Command::File(FileCommand::Import)).unwrap();
            }
            if matches!(case, Case::OpenFileBusy) {
                let _ = ws.apply(Command::Library(LibraryCommand::Refresh));
                assert!(ws.file_busy());
            }
        }
    }
    ws
}

macro_rules! action_matrix {
    ($name:ident, $field:ident, $expected:ident, $admit:expr, $action:expr) => {
        #[test]
        fn $name() {
            for case in Case::ALL {
                let mut ws = fixture(case);
                let expected = case.expected().allows(Permission::$expected);
                let next = ws.next_id;
                let view = ws.action_view();
                assert_eq!(view.$field(), expected, "view: {case:?}");
                assert_eq!(
                    ($admit)(&mut ws.inner).is_ok(),
                    expected,
                    "admission: {case:?}"
                );
                assert_eq!(
                    ($action)(&mut ws.inner).is_ok(),
                    expected,
                    "action: {case:?}"
                );
                if !expected {
                    assert_eq!(ws.next_id, next, "refusal consumed an identity: {case:?}");
                }
            }
        }
    };
}
action_matrix!(
    play_admission_matches_view_and_action,
    can_play,
    Play,
    |ws: &mut Workspace| ws.play_readiness(),
    |ws: &mut Workspace| ws.apply(Command::Audio(AudioCommand::Play))
);
action_matrix!(
    import_admission_matches_view_and_action,
    can_import,
    Import,
    |ws: &mut Workspace| ws.import_admission(),
    |ws: &mut Workspace| ws.apply(Command::File(FileCommand::Import))
);
action_matrix!(
    save_admission_matches_view_and_action,
    can_save,
    Save,
    |ws: &mut Workspace| ws.prepare_save().map(|_| ()),
    |ws: &mut Workspace| ws.save()
);
action_matrix!(
    save_copy_admission_matches_view_and_action,
    can_save_copy,
    SaveCopy,
    |ws: &mut Workspace| ws.prepare_save_copy().map(|_| ()),
    |ws: &mut Workspace| ws.save_independent_copy()
);
action_matrix!(
    export_admission_matches_view_and_action,
    can_export,
    Export,
    |ws: &mut Workspace| ws.prepare_export().map(|_| ()),
    |ws: &mut Workspace| ws.apply(Command::File(FileCommand::ExportWav))
);
action_matrix!(
    portable_export_admission_matches_view_and_action,
    can_export_recipe,
    Portable,
    |ws: &mut Workspace| ws.prepare_export_recipe().map(|_| ()),
    |ws: &mut Workspace| ws.apply(Command::File(FileCommand::ExportRecipe))
);
action_matrix!(
    component_admission_matches_view_and_action,
    can_add_component,
    Add,
    |ws: &mut Workspace| ws.component_add_admission(ws.editor().document_key(), CarrierKind::Tone),
    |ws: &mut Workspace| ws.apply(Command::Editor(EditorCommand::AddComponent {
        row: ws.editor().document_key(),
        kind: CarrierKind::Tone
    }))
);
action_matrix!(
    pair_admission_matches_view_and_action,
    can_add_pair,
    Pair,
    |ws: &mut Workspace| ws
        .pair_add_admission(ws.editor().document_key(), crate::core::PairKind::Binaural),
    |ws: &mut Workspace| ws.apply(Command::Editor(EditorCommand::AddPair {
        row: ws.editor().document_key(),
        kind: crate::core::PairKind::Binaural
    }))
);
action_matrix!(
    restore_admission_matches_view_and_action,
    can_restore_component,
    Restore,
    |ws: &mut Workspace| ws.restore_component_admission(ws.editor().document_key()),
    |ws: &mut Workspace| ws.apply(Command::Editor(EditorCommand::RestoreComponent(
        ws.editor().document_key()
    )))
);

action_matrix!(
    new_admission_matches_view_and_action,
    can_new,
    New,
    |ws: &mut Workspace| ws.start_allowed(),
    |ws: &mut Workspace| ws.apply(Command::Editor(EditorCommand::New))
);
action_matrix!(
    copy_admission_matches_view_and_action,
    can_copy,
    Copy,
    |ws: &mut Workspace| ws.copy_admission(),
    |ws: &mut Workspace| ws.apply(Command::Editor(EditorCommand::CopySelected))
);

#[test]
fn preview_admission_and_file_busy_use_owned_resource_state() {
    for case in Case::ALL {
        let mut ws = fixture(case);
        let expected = case.expected();
        let view = ws.action_view();
        assert_eq!(
            view.can_preview(),
            expected.allows(Permission::Preview),
            "{case:?}"
        );
        assert_eq!(
            ws.preview_admission().is_ok(),
            expected.allows(Permission::Preview),
            "{case:?}"
        );
        assert_eq!(
            view.file_busy,
            expected.allows(Permission::FileBusy),
            "{case:?}"
        );
        assert_eq!(
            ws.file_busy(),
            expected.allows(Permission::FileBusy),
            "{case:?}"
        );
        let revision = ws.revision();
        let _ = ws.apply(Command::Output(OutputCommand::Preview));
        assert_eq!(
            ws.revision() != revision,
            expected.allows(Permission::Preview),
            "{case:?}"
        );
    }
}
#[test]
fn stop_and_cancel_stay_independent_of_other_admissions_until_join() {
    for case in Case::ALL {
        let mut ws = fixture(case);
        let expected = case.expected();
        let view = ws.action_view();
        assert_eq!(
            view.playback_busy(),
            expected.allows(Permission::Audio),
            "{case:?}"
        );
        assert_eq!(
            view.exporting,
            expected.allows(Permission::Cancel),
            "{case:?}"
        );
        assert_eq!(
            view.transport_enabled(),
            expected.allows(Permission::Audio) || expected.allows(Permission::Play),
            "{case:?}"
        );
        assert_eq!(
            ws.audio.stop_admission().is_ok(),
            expected.allows(Permission::Audio),
            "{case:?}"
        );
        assert_eq!(
            ws.files.cancel_admission().is_ok(),
            expected.allows(Permission::Cancel),
            "{case:?}"
        );
        assert_eq!(
            ws.apply(Command::Audio(AudioCommand::Stop)).is_ok(),
            expected.allows(Permission::Audio),
            "{case:?}"
        );
        assert_eq!(
            ws.apply(Command::File(FileCommand::CancelExport)).is_ok(),
            expected.allows(Permission::Cancel),
            "{case:?}"
        );
        assert_eq!(
            ws.audio_busy(),
            expected.allows(Permission::Audio),
            "Stop cannot release an unjoined slot: {case:?}"
        );
        assert_eq!(
            ws.file_busy(),
            expected.allows(Permission::FileBusy),
            "Cancel cannot release an unjoined slot: {case:?}"
        );
    }
}
#[test]
fn editor_actions_share_the_owned_gate() {
    for case in Case::ALL {
        let mut ws = fixture(case);
        let expected = case.expected();
        let view = ws.action_view();
        assert_eq!(
            view.can_edit(),
            expected.allows(Permission::Edit),
            "{case:?}"
        );
        assert_eq!(
            ws.editing_allowed().is_ok(),
            expected.allows(Permission::Edit),
            "{case:?}"
        );
        let handle = ws
            .editor()
            .handle(EditingField::Metadata(
                crate::document::editing::MetadataField::Summary,
            ))
            .unwrap();
        assert_eq!(
            ws.apply(Command::Editor(EditorCommand::Restore(handle)))
                .is_ok(),
            expected.allows(Permission::Edit),
            "{case:?}"
        );
    }
}

action_matrix!(
    library_worker_admission_matches_view_and_action,
    can_manage_library,
    Manage,
    |ws: &mut Workspace| ws.library_job_admission(),
    |ws: &mut Workspace| ws.apply(Command::Library(LibraryCommand::Initialize))
);
action_matrix!(
    add_input_admission_matches_view_and_action,
    can_add_to_library,
    AddFile,
    |ws: &mut Workspace| ws.prepare_add_to_library().map(|_| ()),
    |ws: &mut Workspace| ws.apply(Command::Library(LibraryCommand::AddSelected))
);

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

fn prepare_document_case(ws: &mut TestWorkspace, case: Case) {
    let mut recipe = ws.editor().draft().recipe().unwrap().into_builder();
    let count = if matches!(case, Case::Full) {
        16
    } else {
        u32::from(!matches!(case, Case::FullLineage))
    };
    recipe.content.sound = {
        let fixture = auditorylab_core::test_support::FiniteFixture::new(
            recipe.content.timing.duration_seconds,
            recipe.content.timing.fade_seconds,
            (0..count)
                .map(|index| crate::core::Component {
                    id: ComponentId::new(
                        if matches!(case, Case::Exhausted | Case::RestorableExhausted) {
                            u32::MAX
                        } else {
                            index + 1
                        },
                    )
                    .unwrap(),
                    form: crate::core::ComponentForm {
                        source: crate::core::SoundSource::Tone {
                            frequency_hz: if matches!(case, Case::AcousticInvalid) {
                                finite(30000.0)
                            } else {
                                finite(440.0)
                            },
                            route: crate::core::ChannelRouting::Both,
                            peak_dbfs: finite(-40.0),
                        },
                        modulation: None,
                    },
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        recipe.content.timing = fixture.timing;
        fixture.sound
    };
    if matches!(case, Case::FullLineage) {
        recipe.content.lineage = (0..16)
            .map(|index| crate::core::provenance::Lineage {
                title: format!("Ancestor {index}"),
                note: "Retained relationship".into(),
                document_id: None,
                catalog_id: None,
                source_keys: Vec::new(),
            })
            .collect();
    }
    ws.install_document(EditorSession::new(Draft::from_recipe(recipe.build().unwrap())).unwrap());
    if matches!(case, Case::RestorableExhausted) {
        let row = ws
            .editor()
            .component_key(ComponentId::new(u32::MAX).unwrap())
            .unwrap();
        ws.apply(Command::Editor(EditorCommand::RemoveComponent(row)))
            .unwrap();
    }
}
