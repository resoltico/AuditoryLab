//! Production editor callbacks.
use super::{FieldHandle, MainWindow, Rc, RowKey, SlintAdapter};
use auditorylab_app::workspace::{Command, EditorCommand};

pub(super) fn install_editing(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    let weak = Rc::downgrade(adapter);
    let with = move |f: &dyn Fn(&Rc<SlintAdapter>)| {
        if let Some(adapter) = weak.upgrade() {
            f(&adapter);
        }
    };
    let w = with.clone();
    ui.on_stage_parameter(move |binding, text| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
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
    ui.on_commit_parameter(move |binding, text| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
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
    ui.on_restore_parameter(move |binding| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
                a.command(|w| w.apply(Command::Editor(EditorCommand::Restore(handle))));
            }
        });
    });
    let w = with.clone();
    ui.on_slide(move |binding, position| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
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
    ui.on_stage_metadata(move |binding, text| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
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
    ui.on_commit_metadata(move |binding, text| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
                a.command(|w| {
                    w.apply(Command::Editor(EditorCommand::CommitField {
                        field: handle,
                        text: text.to_string(),
                    }))
                });
            }
        });
    });
    let w = with;
    ui.on_restore_metadata(move |binding| {
        w(&|a| {
            if let Ok(handle) = binding.parse::<FieldHandle>() {
                a.command(|w| w.apply(Command::Editor(EditorCommand::Restore(handle))));
            }
        });
    });
    install_component_editing(adapter, ui);
}

fn install_component_editing(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    let weak = Rc::downgrade(adapter);
    let with = move |f: &dyn Fn(&Rc<SlintAdapter>)| {
        if let Some(adapter) = weak.upgrade() {
            f(&adapter);
        }
    };
    let w = with.clone();
    ui.on_set_routing(move |binding, index| {
        w(&|a| {
            a.choose_control(
                auditorylab_app::workspace::ControlMenu::ChannelRouting,
                Some(&binding),
                index,
            );
        });
    });
    let w = with.clone();
    ui.on_set_carrier(move |binding, index| {
        w(&|a| {
            a.choose_control(
                auditorylab_app::workspace::ControlMenu::Carrier,
                Some(&binding),
                index,
            );
        });
    });
    let w = with.clone();
    ui.on_set_modulation(move |binding, index| {
        w(&|a| {
            a.choose_control(
                auditorylab_app::workspace::ControlMenu::Modulation,
                Some(&binding),
                index,
            );
        });
    });
    let w = with.clone();
    ui.on_set_pulse_option(move |binding, menu, index| {
        w(&|a| {
            use auditorylab_app::workspace::ControlMenu;
            let menu = match menu {
                0 => ControlMenu::SourceWidth,
                1 => ControlMenu::SourceShape,
                2 => ControlMenu::ModulationWidth,
                3 => ControlMenu::ModulationShape,
                _ => return,
            };
            a.choose_control(menu, Some(&binding), index);
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
            a.choose_control(
                auditorylab_app::workspace::ControlMenu::AddComponent,
                Some(&binding),
                index,
            );
        });
    });
}
