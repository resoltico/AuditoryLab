//! Production editor callbacks.
use super::*;
use auditorylab_app::workspace::{Command, EditorCommand};

pub(super) fn install_editing(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    let weak = Rc::downgrade(adapter);
    let with = move |f: &dyn Fn(&Rc<SlintAdapter>)| {
        if let Some(adapter) = weak.upgrade() {
            f(&adapter);
        }
    };
    let w = with.clone();
    ui.on_stage_parameter(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = parameter_handle(&binding, key) {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::Stage {
                        field: handle,
                        text: text.to_string(),
                    }))
                });
            }
        });
    });
    let w = with.clone();
    ui.on_commit_parameter(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = parameter_handle(&binding, key) {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::CommitField {
                        field: handle,
                        text: text.to_string(),
                    }))
                });
            }
        });
    });
    let w = with.clone();
    ui.on_restore_parameter(move |binding, key| {
        w(&|a| {
            if let Some(handle) = parameter_handle(&binding, key) {
                a.command(|w| w.apply(Command::Editor(EditorCommand::Restore(handle))));
            }
        });
    });
    let w = with.clone();
    ui.on_slide(move |binding, key, position| {
        w(&|a| {
            if let Some(handle) = parameter_handle(&binding, key) {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::Slide {
                        field: handle,
                        position: f64::from(position),
                    }))
                });
            }
        });
    });
    let w = with.clone();
    ui.on_stage_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = metadata_handle(&binding, key) {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::Stage {
                        field: handle,
                        text: text.to_string(),
                    }))
                });
            }
        });
    });
    let w = with.clone();
    ui.on_commit_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = metadata_handle(&binding, key) {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::CommitField {
                        field: handle,
                        text: text.to_string(),
                    }))
                });
            }
        });
    });
    let w = with.clone();
    ui.on_restore_metadata(move |binding, key| {
        w(&|a| {
            if let Some(handle) = metadata_handle(&binding, key) {
                a.command(|w| w.apply(Command::Editor(EditorCommand::Restore(handle))));
            }
        });
    });
    let w = with.clone();
    ui.on_set_ears(move |binding, index| {
        w(&|a| {
            if let (Ok(row), Some(ears)) = (binding.parse::<RowKey>(), choice(&Ears::ALL, index)) {
                a.command(|w| w.apply(Command::Editor(EditorCommand::SetEars { row, ears })));
            }
        });
    });
    let w = with.clone();
    ui.on_set_carrier(move |binding, index| {
        w(&|a| {
            if let (Ok(row), Some(kind)) =
                (binding.parse::<RowKey>(), choice(&CarrierKind::ALL, index))
            {
                a.command(|w| w.apply(Command::Editor(EditorCommand::SetCarrier { row, kind })));
            }
        });
    });
    let w = with.clone();
    ui.on_set_modulation(move |binding, index| {
        w(&|a| {
            let kind = choice(
                &auditorylab_app::workspace::controls::modulation_choices(),
                index,
            );
            if let (Ok(row), Some(kind)) = (binding.parse::<RowKey>(), kind) {
                a.command(|w| w.apply(Command::Editor(EditorCommand::SetModulation { row, kind })));
            }
        });
    });
    let w = with.clone();
    ui.on_remove_component(move |binding| {
        w(&|a| {
            if let Ok(row) = binding.parse::<RowKey>() {
                a.command(|w| w.apply(Command::Editor(EditorCommand::RemoveComponent(row))));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_component(move |binding| {
        w(&|a| {
            if let Ok(row) = binding.parse::<RowKey>() {
                a.command(|w| w.apply(Command::Editor(EditorCommand::RestoreComponent(row))));
            }
        });
    });
    let w = with;
    ui.on_add_component(move |binding, index| {
        w(&|a| {
            let Ok(row) = binding.parse::<RowKey>() else {
                return;
            };
            match index {
                0 | 1 => {
                    if let Some(kind) = choice(&CarrierKind::ALL, index) {
                        a.command(|w| {
                            w.apply(Command::Editor(EditorCommand::AddComponent { row, kind }))
                        });
                    }
                }
                2 => {
                    a.command(|w| {
                        w.apply(Command::Editor(EditorCommand::AddPair {
                            row,
                            kind: auditorylab_app::core::PairKind::Binaural,
                        }))
                    });
                }
                3 => {
                    a.command(|w| {
                        w.apply(Command::Editor(EditorCommand::AddPair {
                            row,
                            kind: auditorylab_app::core::PairKind::Monaural,
                        }))
                    });
                }
                _ => {}
            }
        });
    });
}
