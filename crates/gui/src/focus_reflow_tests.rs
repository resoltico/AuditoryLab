//! Generated Workbench focus checks across typography and clipping transitions.

use crate::{MainWindow, Row};
use slint::private_unstable_api::re_exports::{
    AccessibleStringProperty, Flickable, FocusReason, ItemRc, TextInput, WindowInner,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, ops::ControlFlow, rc::Rc};

fn settle(ui: &MainWindow) {
    for _ in 0..3 {
        slint::platform::update_timers_and_animations();
        WindowInner::from_pub(ui.window()).draw_contents(|_, _| ());
    }
}

fn key(ui: &MainWindow, text: impl Into<slint::SharedString>) {
    let text = text.into();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
    settle(ui);
}

fn entry(ui: &MainWindow, label: &str) -> ItemRc {
    let root = WindowInner::from_pub(ui.window()).window_item_rc().unwrap();
    let owner = root
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
    owner
        .visit_descendants(|item| {
            if item.downcast::<TextInput>().is_some() {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap()
}

fn snapshot(ui: &MainWindow, item: &ItemRc, context: &str) {
    let inner = WindowInner::from_pub(ui.window());
    let field = item.downcast::<TextInput>().unwrap();
    let focused = inner.focus_item.borrow().upgrade();
    let geometry = item.geometry();
    eprintln!(
        "Workbench reflow {context}: font={} compact={} monitor={} active={} same-focus={} field-focus={} raw={:?} visible={} geometry={geometry:?} window-position={:?} editor-scroll={}",
        ui.get_ui_size(),
        ui.get_compact(),
        ui.get_monitor_open(),
        inner.active(),
        focused.as_ref() == Some(item),
        field.as_pin_ref().has_focus(),
        field.as_pin_ref().text(),
        item.is_visible(),
        item.map_to_window(geometry.origin),
        ui.get_editor_scroll_y(),
    );
}

/// Uses the already initialized test platform and actual generated Workbench controls.
pub(super) fn assert_pending_entry_survives_font_reflow(ui: &MainWindow) {
    const ID: &str = "font-reflow-fixture";
    let pending = Rc::new(RefCell::new(None::<(i32, String)>));
    let commits = Rc::new(RefCell::new(Vec::<(i32, String)>::new()));
    let restores = Rc::new(RefCell::new(Vec::new()));
    let rows = Rc::new(VecModel::from(vec![Row {
        id: ID.into(),
        source: 1,
        name: "Click rhythm · layer 1".into(),
        heading: "Click rhythm · layer 1".into(),
        text1: "40".into(),
        text2: "1".into(),
        text3: "-30".into(),
        text4: "0".into(),
        ..Row::default()
    }]));
    let p = pending.clone();
    let model = rows.clone();
    ui.on_stage(move |id, setting, value| {
        assert_eq!(id, ID);
        *p.borrow_mut() = Some((setting, value.to_string()));
        let mut row = model.row_data(0).unwrap();
        match setting {
            1 => {
                row.text1 = value;
                row.staged1 = true;
            }
            3 => {
                row.text3 = value;
                row.staged3 = true;
            }
            _ => panic!("unexpected focused setting {setting}"),
        }
        model.set_row_data(0, row);
    });
    let c = commits.clone();
    ui.on_edit(move |id, setting, value| {
        assert_eq!(id, ID);
        c.borrow_mut().push((setting, value.to_string()));
    });
    let p = pending.clone();
    let r = restores.clone();
    let model = rows.clone();
    ui.on_restore(move |id, setting| {
        assert_eq!(id, ID);
        *p.borrow_mut() = None;
        r.borrow_mut().push(setting);
        let mut row = model.row_data(0).unwrap();
        let value = match setting {
            1 => {
                row.text1 = "40".into();
                row.staged1 = false;
                "40"
            }
            3 => {
                row.text3 = "-30".into();
                row.staged3 = false;
                "-30"
            }
            _ => panic!("unexpected restored setting {setting}"),
        };
        model.set_row_data(0, row);
        value.into()
    });
    ui.set_monitor_open(false);
    ui.set_tab_index(1);
    ui.set_can_edit(true);
    ui.set_edits_suspended(false);
    ui.set_ui_family(crate::typography::resolve().family.into());
    ui.set_rows(ModelRc::from(rows.clone()));
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    let inner = WindowInner::from_pub(ui.window());
    for (label, setting, raw, height) in [
        ("Click rhythm · layer 1: Click rate (Hz)", 1, "40e", 900.0),
        (
            "Click rhythm · layer 1: Peak level (dBFS)",
            3,
            "-30e",
            700.0,
        ),
    ] {
        ui.set_ui_size(13.0);
        ui.window()
            .set_size(slint::LogicalSize::new(1100.0, height));
        settle(ui);
        let input = entry(ui, label);
        inner.set_focus_item(&input, true, FocusReason::Programmatic);
        for _ in 0..4 {
            key(ui, slint::platform::Key::RightArrow);
        }
        key(ui, "e");
        assert_eq!(pending.borrow().as_ref(), Some(&(setting, raw.to_owned())));
        assert!(commits.borrow().is_empty());
        for size in [22.75, 13.0] {
            ui.set_ui_size(size);
            settle(ui);
            snapshot(ui, &input, "no-input typography leg");
            assert!(
                !ui.get_monitor_open(),
                "closed monitor remains closed throughout reflow"
            );
            assert!(
                inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
                "typography reflow must retain the same focused item"
            );
            assert!(
                input
                    .downcast::<TextInput>()
                    .unwrap()
                    .as_pin_ref()
                    .has_focus()
            );
            assert_eq!(
                input.downcast::<TextInput>().unwrap().as_pin_ref().text(),
                raw
            );
            assert!(
                commits.borrow().is_empty(),
                "font reflow does not commit pending text"
            );
        }
        key(ui, slint::platform::Key::Escape);
        assert_eq!(restores.borrow().last(), Some(&setting));
        assert!(pending.borrow().is_none());
    }
    let input = entry(ui, "Click rhythm · layer 1: Click rate (Hz)");
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    ui.set_tab_index(0);
    settle(ui);
    snapshot(ui, &input, "genuinely hidden tab negative control");
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() != Some(&input),
        "a genuinely hidden tab must not retain editing focus"
    );
    assert!(
        !input
            .downcast::<TextInput>()
            .unwrap()
            .as_pin_ref()
            .has_focus()
    );
    ui.set_tab_index(1);
    ui.set_ui_size(13.0);
    ui.set_editor_scroll_y(0.0);
    settle(ui);
    // Use a supported overflowing document, not an assumed scroll offset.
    for layer in 2..=8 {
        let mut row = rows.row_data(0).unwrap();
        row.id = format!("font-reflow-layer-{layer}").into();
        row.name = format!("Click rhythm · layer {layer}").into();
        row.heading = row.name.clone();
        rows.push(row);
    }
    assert!(rows.row_count() <= 16);
    settle(ui);
    // Distinguish pure reflow from a non-click pointer event on a clipped entry.
    let input = entry(ui, "Click rhythm · layer 1: Click rate (Hz)");
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    for _ in 0..4 {
        key(ui, slint::platform::Key::RightArrow);
    }
    key(ui, "e");
    assert_eq!(pending.borrow().as_ref(), Some(&(1, "40e".to_owned())));
    let root = inner.window_item_rc().unwrap();
    let viewport = root
        .visit_descendants(|item| {
            if item.downcast::<Flickable>().is_some()
                && item
                    .visit_descendants(|child| {
                        if *child == input {
                            ControlFlow::Break(())
                        } else {
                            ControlFlow::Continue(())
                        }
                    })
                    .is_some()
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
    let flickable = viewport.downcast::<Flickable>().unwrap();
    let overflow = flickable.as_pin_ref().content_height().get() - viewport.geometry().size.height;
    assert!(overflow > 0.0, "multi-layer editor must actually overflow");
    ui.set_editor_scroll_y(-overflow);
    settle(ui);
    assert!(viewport.is_visible(), "the editor viewport remains visible");
    assert_eq!(ui.get_tab_index(), 1, "the entry's tab remains selected");
    assert!(
        flickable.as_pin_ref().content_y().get() < 0.0,
        "actual content must scroll"
    );
    snapshot(
        ui,
        &input,
        "deliberately scrolled offscreen before pointer move",
    );
    assert!(
        !input.is_visible(),
        "control must actually be outside the viewport"
    );
    let input_geometry = input.geometry();
    let input_position = input.map_to_window(input_geometry.origin);
    let viewport_geometry = viewport.geometry();
    let viewport_position = viewport.map_to_window(viewport_geometry.origin);
    assert!(
        input_position.y + input_geometry.size.height < viewport_position.y
            || input_position.y > viewport_position.y + viewport_geometry.size.height,
        "the visible Flickable must be the physical vertical clipping boundary"
    );

    // Escape is the first input after clipping: no pointer event or refocus.
    assert!(inner.focus_item.borrow().upgrade().as_ref() == Some(&input));
    key(ui, slint::platform::Key::Escape);
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
        "no-pointer Escape must retain the clipped editor's focus"
    );
    assert!(
        pending.borrow().is_none(),
        "no-pointer Escape must restore pending text"
    );
    assert_eq!(
        input.downcast::<TextInput>().unwrap().as_pin_ref().text(),
        "40"
    );
    assert!(
        input.is_visible(),
        "no-pointer Escape must reveal its restored editor"
    );
    assert!(
        commits.borrow().is_empty(),
        "no-pointer Escape must not blur or commit"
    );
    for _ in 0..4 {
        key(ui, slint::platform::Key::RightArrow);
    }
    key(ui, "e");
    assert_eq!(pending.borrow().as_ref(), Some(&(1, "40e".to_owned())));
    ui.set_editor_scroll_y(-overflow);
    settle(ui);
    assert!(!input.is_visible(), "pointer case must begin clipped again");
    let scroll_before_pointer = ui.get_editor_scroll_y();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    settle(ui);
    snapshot(ui, &input, "after non-click pointer move");
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
        "pointer movement must not discard an offscreen pending editor"
    );
    assert!(commits.borrow().is_empty());
    assert_eq!(
        ui.get_editor_scroll_y(),
        scroll_before_pointer,
        "pointer movement must not move the viewport or retarget a later click"
    );
    assert_eq!(pending.borrow().as_ref(), Some(&(1, "40e".to_owned())));
    key(ui, "1");
    assert_eq!(pending.borrow().as_ref(), Some(&(1, "40e1".to_owned())));
    assert!(
        input.is_visible(),
        "keyboard editing must reveal the retained offscreen entry"
    );
    assert!(inner.focus_item.borrow().upgrade().as_ref() == Some(&input));
    key(ui, slint::platform::Key::Escape);
    assert!(pending.borrow().is_none());
    // A real click still activates its original target and commits the old field once.
    for _ in 0..4 {
        key(ui, slint::platform::Key::RightArrow);
    }
    key(ui, "e");
    let other = entry(ui, "Click rhythm · layer 1: Peak level (dBFS)");
    let geometry = other.geometry();
    let origin = other.map_to_window(geometry.origin);
    let position = slint::LogicalPosition::new(
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    assert!(
        other.is_visible(),
        "the intended click target must be visible before the click"
    );
    let scroll_before_click = ui.get_editor_scroll_y();
    for pressed in [true, false] {
        ui.window().dispatch_event(if pressed {
            slint::platform::WindowEvent::PointerPressed {
                position,
                button: slint::platform::PointerEventButton::Left,
            }
        } else {
            slint::platform::WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            }
        });
    }
    settle(ui);
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&other),
        "click must activate the measured target"
    );
    assert_eq!(
        &*commits.borrow(),
        &[(1, "40e".to_owned())],
        "ordinary blur commits exactly once"
    );
    assert_eq!(
        ui.get_editor_scroll_y(),
        scroll_before_click,
        "the pointer guard must not scroll before hit testing"
    );
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    key(ui, slint::platform::Key::Escape);
    assert!(pending.borrow().is_none());
    drop(other);
    // Changing kind replaces a single-instance conditional field, not its row.
    let old_conditional = input.clone();
    let mut tone = rows.row_data(0).unwrap();
    tone.source = 0;
    tone.text0 = "3750".into();
    rows.set_row_data(0, tone);
    settle(ui);
    assert!(
        root.visit_descendants(|item| {
            if *item == old_conditional {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_none(),
        "kind change must remove the actual click-rate conditional"
    );
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    settle(ui);
    assert!(
        inner.focus_item.borrow().upgrade().is_none(),
        "replaced conditional must not retain focus"
    );
    drop(old_conditional);
    drop(input);
    let frequency = entry(ui, "Click rhythm · layer 1: Tone frequency (Hz)");
    inner.set_focus_item(&frequency, true, FocusReason::Programmatic);
    settle(ui);
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&frequency),
        "attached replacement conditional remains focusable"
    );
    let destroyed = frequency.downgrade();
    drop(frequency);
    rows.set_vec(Vec::new());
    settle(ui);
    assert_eq!(
        ui.get_rows().row_count(),
        0,
        "the row model must really be empty"
    );
    if let Some(detached) = destroyed.upgrade() {
        eprintln!(
            "Removed delegate diagnostic: old is_visible={} geometry={:?} current-focus={}",
            detached.is_visible(),
            detached.geometry(),
            inner.focus_item.borrow().upgrade().as_ref() == Some(&detached)
        );
        assert!(
            root.visit_descendants(|item| {
                if *item == detached {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_none(),
            "removed input must leave the actual live item tree"
        );
    }
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    settle(ui);
    assert!(
        inner.focus_item.borrow().upgrade().is_none(),
        "destroyed delegate cannot retain focus"
    );
    ui.set_editor_scroll_y(0.0);
    settle(ui);
}
