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
    fn expected(self) -> Expected {
        let idle = Expected {
            play: true,
            import: true,
            new: true,
            copy: true,
            save: true,
            save_copy: true,
            export: true,
            portable: true,
            preview: true,
            edit: true,
            add: true,
            pair: true,
            restore: false,
            file_busy: false,
            audio: false,
            cancel: false,
            manage: true,
            add_file: false,
        };
        match self {
            Self::Idle => idle,
            Self::Prompt | Self::OpenFilePrompt => Expected {
                play: false,
                manage: false,
                import: false,
                new: false,
                copy: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                edit: false,
                add: false,
                pair: false,
                ..idle
            },
            Self::PendingFile => Expected {
                play: false,
                import: false,
                new: false,
                copy: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                edit: false,
                add: false,
                pair: false,
                file_busy: true,
                manage: false,
                ..idle
            },
            Self::Import => Expected {
                import: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                edit: false,
                add: false,
                pair: false,
                file_busy: true,
                manage: false,
                ..idle
            },
            Self::Export | Self::ExportAtEnd => Expected {
                play: false,
                import: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                edit: false,
                add: false,
                pair: false,
                file_busy: true,
                manage: false,
                cancel: true,
                ..idle
            },
            Self::Background | Self::OpenFileBusy => Expected {
                import: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                file_busy: true,
                manage: false,
                ..idle
            },
            Self::Audio => Expected {
                export: false,
                preview: false,
                audio: true,
                ..idle
            },
            Self::InvalidText => Expected {
                play: false,
                save: false,
                save_copy: false,
                export: false,
                portable: false,
                ..idle
            },
            Self::AcousticInvalid => Expected {
                export: false,
                ..idle
            },
            Self::Full | Self::Exhausted => Expected {
                add: false,
                pair: false,
                ..idle
            },
            Self::RestorableExhausted => Expected {
                add: false,
                pair: false,
                restore: true,
                ..idle
            },
            Self::Unavailable => Expected {
                save: false,
                manage: false,
                save_copy: false,
                ..idle
            },
            Self::NoSelection => Expected {
                play: false,
                copy: false,
                export: false,
                portable: false,
                ..idle
            },
            Self::FullLineage => Expected {
                save_copy: false,
                ..idle
            },
            Self::OpenFile => Expected {
                add_file: true,
                ..idle
            },
            Self::NoDevice => Expected {
                play: false,
                ..idle
            },
        }
    }
}
#[derive(Clone, Copy)]
struct Expected {
    play: bool,
    import: bool,
    new: bool,
    copy: bool,
    save: bool,
    save_copy: bool,
    export: bool,
    portable: bool,
    preview: bool,
    edit: bool,
    add: bool,
    pair: bool,
    restore: bool,
    file_busy: bool,
    audio: bool,
    cancel: bool,
    manage: bool,
    add_file: bool,
}

fn fixture(case: Case) -> TestWorkspace {
    let mut ws = workspace();
    ws.set_view(View::Workbench);
    match case {
        Case::Idle => {}
        Case::Prompt => ws.import().unwrap(),
        Case::PendingFile | Case::Import => {
            let path = ws.store.path().join("pending-input.toml");
            ws.start_file(FileKind::Import(path));
            if matches!(case, Case::PendingFile) {
                ws.new_document();
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
            ws.refresh_library();
            assert!(ws.file_busy());
        }
        Case::Audio => ws.play().unwrap(),
        Case::InvalidText => {
            let handle = ws
                .editor()
                .handle(EditingField::Metadata(
                    crate::document::editing::MetadataField::Duration,
                ))
                .unwrap();
            ws.stage(handle, "unparsed duration").unwrap();
        }
        Case::AcousticInvalid
        | Case::Full
        | Case::Exhausted
        | Case::RestorableExhausted
        | Case::FullLineage => {
            let mut recipe = ws.editor().draft().recipe().unwrap().into_builder();
            let count = if matches!(case, Case::Full) {
                16
            } else if matches!(case, Case::FullLineage) {
                0
            } else {
                1
            };
            recipe.content.sound = auditorylab_core::Sound::new(
                recipe.content.sound.duration_seconds(),
                recipe.content.sound.fade_seconds(),
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
                        form: crate::core::ComponentForm::Tone {
                            frequency_hz: if matches!(case, Case::AcousticInvalid) {
                                finite(30000.0)
                            } else {
                                finite(440.0)
                            },
                            ears: crate::core::Ears::Both,
                            peak_dbfs: finite(-40.0),
                            modulation: None,
                        },
                    })
                    .collect(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
            if matches!(case, Case::FullLineage) {
                recipe.content.lineage = (0..16)
                    .map(|index| crate::core::Lineage {
                        title: format!("Ancestor {index}"),
                        note: "Retained relationship".into(),
                        document_id: None,
                        catalog_id: None,
                        source_keys: Vec::new(),
                    })
                    .collect();
            }
            ws.install_document(
                EditorSession::new(Draft::from_recipe(recipe.build().unwrap())).unwrap(),
            );
            if matches!(case, Case::RestorableExhausted) {
                let row = ws
                    .editor()
                    .component_key(ComponentId::new(u32::MAX).unwrap())
                    .unwrap();
                ws.remove_component(row).unwrap();
            }
        }
        Case::Unavailable => {
            ws.library.binding =
                LibraryBinding::Unavailable(crate::services::library::LibraryDiagnostic {
                    storage_id: "configured root".into(),
                    code: "configuration".into(),
                    detail: crate::services::library::DiagnosticDetail::Failure(Box::new(
                        crate::storage::library::LibraryError::NativeRootUnavailable,
                    )),
                });
        }
        Case::NoSelection => {
            ws.set_view(View::Library);
            ws.set_query("no catalog source has this title");
            assert!(ws.selected().is_none());
        }
        Case::NoDevice => ws.audio.capabilities = Capabilities::no_device_playback(),
        Case::OpenFile | Case::OpenFilePrompt | Case::OpenFileBusy => {
            let snapshot = RecipeSnapshot::new(
                crate::document::recipe::Origin::Stream("opened input".into()),
                crate::test_recipes::silent(1.0, 0.0),
            );
            ws.add_import(snapshot).unwrap();
            ws.set_view(View::Library);
            if matches!(case, Case::OpenFilePrompt) {
                ws.import().unwrap();
            }
            if matches!(case, Case::OpenFileBusy) {
                ws.refresh_library();
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
                let expected = case.expected().$expected;
                let next = ws.next_id;
                let view = ws.action_view();
                assert_eq!(view.$field, expected, "view: {case:?}");
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
    play,
    |ws: &mut Workspace| ws.play_readiness(),
    |ws: &mut Workspace| ws.play()
);
action_matrix!(
    import_admission_matches_view_and_action,
    can_import,
    import,
    |ws: &mut Workspace| ws.import_admission(),
    |ws: &mut Workspace| ws.import()
);
action_matrix!(
    save_admission_matches_view_and_action,
    can_save,
    save,
    |ws: &mut Workspace| ws.prepare_save().map(|_| ()),
    |ws: &mut Workspace| ws.save()
);
action_matrix!(
    save_copy_admission_matches_view_and_action,
    can_save_copy,
    save_copy,
    |ws: &mut Workspace| ws.prepare_save_copy().map(|_| ()),
    |ws: &mut Workspace| ws.save_independent_copy()
);
action_matrix!(
    export_admission_matches_view_and_action,
    can_export,
    export,
    |ws: &mut Workspace| ws.prepare_export().map(|_| ()),
    |ws: &mut Workspace| ws.export()
);
action_matrix!(
    portable_export_admission_matches_view_and_action,
    can_export_recipe,
    portable,
    |ws: &mut Workspace| ws.prepare_export_recipe().map(|_| ()),
    |ws: &mut Workspace| ws.export_recipe()
);
action_matrix!(
    component_admission_matches_view_and_action,
    can_add_component,
    add,
    |ws: &mut Workspace| ws.component_add_admission(ws.editor().document_key(), CarrierKind::Tone),
    |ws: &mut Workspace| ws.add_component(ws.editor().document_key(), CarrierKind::Tone)
);
action_matrix!(
    pair_admission_matches_view_and_action,
    can_add_pair,
    pair,
    |ws: &mut Workspace| ws
        .pair_add_admission(ws.editor().document_key(), crate::core::PairKind::Binaural),
    |ws: &mut Workspace| ws.add_pair(ws.editor().document_key(), crate::core::PairKind::Binaural)
);
action_matrix!(
    restore_admission_matches_view_and_action,
    can_restore_component,
    restore,
    |ws: &mut Workspace| ws.restore_component_admission(ws.editor().document_key()),
    |ws: &mut Workspace| ws.restore_component(ws.editor().document_key())
);

fn document_action_result(ws: &Workspace) -> Result<(), Refusal> {
    if let FileStatus::Refused(refusal) = &ws.files.status {
        Err(refusal.clone())
    } else {
        Ok(())
    }
}
action_matrix!(
    new_admission_matches_view_and_action,
    can_new,
    new,
    |ws: &mut Workspace| ws.start_allowed(),
    |ws: &mut Workspace| {
        ws.new_document();
        document_action_result(ws)
    }
);
action_matrix!(
    copy_admission_matches_view_and_action,
    can_copy,
    copy,
    |ws: &mut Workspace| ws.copy_admission(),
    |ws: &mut Workspace| {
        ws.copy_selected();
        document_action_result(ws)
    }
);

#[test]
fn preview_admission_and_file_busy_use_owned_resource_state() {
    for case in Case::ALL {
        let mut ws = fixture(case);
        let expected = case.expected();
        let view = ws.action_view();
        assert_eq!(view.can_preview, expected.preview, "{case:?}");
        assert_eq!(ws.preview_admission().is_ok(), expected.preview, "{case:?}");
        assert_eq!(view.file_busy, expected.file_busy, "{case:?}");
        assert_eq!(ws.file_busy(), expected.file_busy, "{case:?}");
        let revision = ws.revision();
        ws.preview_current();
        assert_eq!(ws.revision() != revision, expected.preview, "{case:?}");
    }
}
#[test]
fn stop_and_cancel_stay_independent_of_other_admissions_until_join() {
    for case in Case::ALL {
        let mut ws = fixture(case);
        let expected = case.expected();
        let view = ws.action_view();
        assert_eq!(view.playback_busy, expected.audio, "{case:?}");
        assert_eq!(view.exporting, expected.cancel, "{case:?}");
        assert_eq!(
            view.transport_enabled,
            expected.audio || expected.play,
            "{case:?}"
        );
        assert_eq!(
            ws.audio.stop_admission().is_ok(),
            expected.audio,
            "{case:?}"
        );
        assert_eq!(
            ws.files.cancel_admission().is_ok(),
            expected.cancel,
            "{case:?}"
        );
        assert_eq!(ws.stop().is_ok(), expected.audio, "{case:?}");
        assert_eq!(ws.cancel_export().is_ok(), expected.cancel, "{case:?}");
        assert_eq!(
            ws.audio_busy(),
            expected.audio,
            "Stop cannot release an unjoined slot: {case:?}"
        );
        assert_eq!(
            ws.file_busy(),
            expected.file_busy,
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
        assert_eq!(view.can_edit, expected.edit, "{case:?}");
        assert_eq!(ws.editing_allowed().is_ok(), expected.edit, "{case:?}");
        let handle = ws
            .editor()
            .handle(EditingField::Metadata(
                crate::document::editing::MetadataField::Summary,
            ))
            .unwrap();
        assert_eq!(ws.restore(handle).is_ok(), expected.edit, "{case:?}");
    }
}

action_matrix!(
    library_worker_admission_matches_view_and_action,
    can_manage_library,
    manage,
    |ws: &mut Workspace| ws.library_job_admission(),
    |ws: &mut Workspace| ws.initialize_library()
);
action_matrix!(
    add_input_admission_matches_view_and_action,
    can_add_to_library,
    add_file,
    |ws: &mut Workspace| ws.prepare_add_to_library().map(|_| ()),
    |ws: &mut Workspace| ws.add_selected_to_library()
);

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
