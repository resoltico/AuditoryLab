//! Production editor callbacks.
use super::*;

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
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.stage(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_commit_parameter(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.commit_field(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_parameter(move |binding, key| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.restore(handle));
            }
        });
    });
    let w = with.clone();
    ui.on_slide(move |binding, key, position| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.slide(handle, f64::from(position)));
            }
        });
    });
    let w = with.clone();
    ui.on_stage_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.stage(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_commit_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.commit_field(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_metadata(move |binding, key| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.restore(handle));
            }
        });
    });
    let w = with.clone();
    ui.on_set_ears(move |binding, index| {
        w(&|a| {
            if let (Some((document, id)), Some(ears)) =
                (a.component(binding), choice(&Ears::ALL, index))
            {
                a.command(|w| w.set_ears(document, id, ears));
            }
        });
    });
    let w = with.clone();
    ui.on_set_carrier(move |binding, index| {
        w(&|a| {
            if let (Some((document, id)), Some(kind)) =
                (a.component(binding), choice(&CarrierKind::ALL, index))
            {
                a.command(|w| w.set_carrier(document, id, kind));
            }
        });
    });
    let w = with.clone();
    ui.on_set_modulation(move |binding, index| {
        w(&|a| {
            let kind = match index {
                0 => Some(None),
                i => choice(&ModulationKind::ALL, i - 1).map(Some),
            };
            if let (Some((document, id)), Some(kind)) = (a.component(binding), kind) {
                a.command(|w| w.set_modulation(document, id, kind));
            }
        });
    });
    let w = with.clone();
    ui.on_remove_component(move |binding| {
        w(&|a| {
            if let Some((document, id)) = a.component(binding) {
                a.command(|w| w.remove_component(document, id));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_component(move |binding| {
        w(&|a| {
            if let Some(Binding::Document(document)) = a.resolve(binding) {
                a.command(|w| w.restore_component(document));
            }
        });
    });
    let w = with;
    ui.on_add_component(move |binding, index| {
        w(&|a| {
            let Some(Binding::Document(document)) = a.resolve(binding) else {
                return;
            };
            match index {
                0 | 1 => {
                    if let Some(kind) = choice(&CarrierKind::ALL, index) {
                        a.command(|w| w.add_component(document, kind));
                    }
                }
                2 => {
                    a.command(|w| w.add_pair(document, auditorylab::PairKind::Binaural));
                }
                3 => {
                    a.command(|w| w.add_pair(document, auditorylab::PairKind::Monaural));
                }
                _ => {}
            }
        });
    });
}
