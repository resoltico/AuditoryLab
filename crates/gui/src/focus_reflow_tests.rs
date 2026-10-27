//! Workbench focus through typography reflow, clipping, kind changes,
//! row removal and auxiliary-window handoff, with the production bindings.
use crate::adapter::hide_monitor_window;
use crate::adapter_tests::Harness;
use crate::generated::MonitorWindow;
use auditorylab_app::core::ParameterId;
use auditorylab_app::document::editing::EditingField;
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{
    Flickable, FocusReason, ItemRc, TextInput, WindowInner,
};
use slint::{ComponentHandle, Model};
use std::ops::ControlFlow;

fn snapshot(h: &Harness, item: &ItemRc, context: &str) {
    let ui = h.ui();
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

fn text(item: &ItemRc) -> String {
    item.downcast::<TextInput>()
        .unwrap()
        .as_pin_ref()
        .text()
        .to_string()
}

/// Staged text and committed value of the first component's field.
fn field_state(h: &Harness, id: ParameterId) -> (Option<String>, Option<f64>) {
    h.adapter.inspect(|w| {
        let component = w.editor().draft().components()[0].id;
        let field = EditingField::Parameter(component, id);
        (
            w.editor().staged(field).map(str::to_owned),
            w.editor().draft().value(field),
        )
    })
}

#[test]
fn pending_entry_survives_font_reflow_clipping_and_row_changes() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.set_ui_family(crate::typography::resolve().family.into());
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 1);
    let inner = WindowInner::from_pub(ui.window());
    let rate_label = "Clicks · Component 1: Clicks rate (Hz)";
    let peak_label = "Clicks · Component 1: Peak level (dBFS)";
    for (label, id, committed, raw, height) in [
        (rate_label, ParameterId::ClickRate, 40.0, "405", 900.0),
        (peak_label, ParameterId::PeakLevel, -30.0, "-305", 700.0),
    ] {
        ui.set_ui_size(13.0);
        ui.window()
            .set_size(slint::LogicalSize::new(1100.0, height));
        h.settle();
        let input = h.field(label);
        inner.set_focus_item(&input, true, FocusReason::Programmatic);
        for _ in 0..4 {
            h.key(Key::RightArrow);
        }
        h.key("5");
        assert_eq!(field_state(&h, id), (Some(raw.to_owned()), Some(committed)));
        for size in [22.75, 13.0] {
            ui.set_ui_size(size);
            h.settle();
            snapshot(&h, &input, "no-input typography leg");
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
            assert_eq!(text(&input), raw);
            assert_eq!(
                field_state(&h, id),
                (Some(raw.to_owned()), Some(committed)),
                "font reflow does not commit pending text"
            );
        }
        h.key(Key::Escape);
        assert_eq!(field_state(&h, id), (None, Some(committed)));
    }
    let input = h.field(rate_label);
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    ui.set_tab_index(0);
    h.settle();
    snapshot(&h, &input, "genuinely hidden tab negative control");
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
    h.workbench();
    ui.set_ui_size(13.0);
    ui.set_editor_scroll_y(0.0);
    h.settle();
    // Use a supported overflowing document, not an assumed scroll offset.
    for _ in 2..=8 {
        ui.invoke_add_component(ui.get_document(), 1);
    }
    h.settle();
    // Distinguish pure reflow from a non-click pointer event on a clipped entry.
    let input = h.field(rate_label);
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("5");
    assert_eq!(
        field_state(&h, ParameterId::ClickRate).0.as_deref(),
        Some("405")
    );
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
    assert!(
        overflow > 0.0,
        "multi-component editor must actually overflow"
    );
    ui.set_editor_scroll_y(-overflow);
    h.settle();
    assert!(viewport.is_visible(), "the editor viewport remains visible");
    assert_eq!(ui.get_tab_index(), 1, "the entry's tab remains selected");
    assert!(
        flickable.as_pin_ref().content_y().get() < 0.0,
        "actual content must scroll"
    );
    snapshot(
        &h,
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
    h.key(Key::Escape);
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
        "no-pointer Escape must retain the clipped editor's focus"
    );
    assert_eq!(
        field_state(&h, ParameterId::ClickRate),
        (None, Some(40.0)),
        "no-pointer Escape restores without committing"
    );
    assert_eq!(text(&input), "40");
    assert!(
        input.is_visible(),
        "no-pointer Escape must reveal its restored editor"
    );
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("5");
    ui.set_editor_scroll_y(-overflow);
    h.settle();
    assert!(!input.is_visible(), "pointer case must begin clipped again");
    let scroll_before_pointer = ui.get_editor_scroll_y();
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(10.0, 10.0),
    });
    h.settle();
    snapshot(&h, &input, "after non-click pointer move");
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
        "pointer movement must not discard an offscreen pending editor"
    );
    assert_eq!(
        ui.get_editor_scroll_y(),
        scroll_before_pointer,
        "pointer movement must not move the viewport or retarget a later click"
    );
    assert_eq!(
        field_state(&h, ParameterId::ClickRate),
        (Some("405".into()), Some(40.0))
    );
    h.key("1");
    assert_eq!(
        field_state(&h, ParameterId::ClickRate).0.as_deref(),
        Some("4051")
    );
    assert!(
        input.is_visible(),
        "keyboard editing must reveal the retained offscreen entry"
    );
    assert!(inner.focus_item.borrow().upgrade().as_ref() == Some(&input));
    h.key(Key::Escape);
    // A real click still activates its original target and commits the old field once.
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("5");
    let other = h.field(peak_label);
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
            WindowEvent::PointerPressed {
                position,
                button: slint::platform::PointerEventButton::Left,
            }
        } else {
            WindowEvent::PointerReleased {
                position,
                button: slint::platform::PointerEventButton::Left,
            }
        });
    }
    h.settle();
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&other),
        "click must activate the measured target"
    );
    assert_eq!(
        field_state(&h, ParameterId::ClickRate),
        (None, Some(405.0)),
        "ordinary blur commits the pending entry"
    );
    assert_eq!(
        ui.get_editor_scroll_y(),
        scroll_before_click,
        "the pointer guard must not scroll before hit testing"
    );
    drop(other);
    // Changing kind replaces the field, not its component row.
    let old_field = input.clone();
    let binding = ui.get_components().row_data(0).unwrap().key;
    ui.invoke_set_carrier(binding.clone(), 0);
    h.settle();
    assert!(
        root.visit_descendants(|item| {
            if *item == old_field {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_none(),
        "kind change must remove the actual click-rate field"
    );
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(10.0, 10.0),
    });
    h.settle();
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() != Some(&old_field),
        "a removed field must not retain focus"
    );
    drop(old_field);
    drop(input);
    let frequency = h.field("Steady tone · Component 1: Tone frequency (Hz)");
    inner.set_focus_item(&frequency, true, FocusReason::Programmatic);
    h.settle();
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&frequency),
        "the replacement field is focusable"
    );
    let destroyed = frequency.downgrade();
    drop(frequency);
    while let Some(row) = ui.get_components().row_data(0) {
        ui.invoke_remove_component(row.key);
    }
    h.settle();
    assert_eq!(
        ui.get_components().row_count(),
        0,
        "the row model must really be empty"
    );
    if let Some(detached) = destroyed.upgrade() {
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
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(10.0, 10.0),
    });
    h.settle();
    assert!(
        inner.focus_item.borrow().upgrade().is_none(),
        "destroyed field cannot retain focus"
    );
}

#[test]
fn monitor_handoff_and_deactivation_preserve_numeric_entry() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    let staged = || field_state(&h, ParameterId::ToneFrequency).0;
    let committed = || field_state(&h, ParameterId::ToneFrequency).1;
    ui.invoke_focus_monitor_opener();
    // Navigate the real component tree, proving the target by its staged field.
    for _ in 0..60 {
        h.key(Key::Tab);
        for _ in 0..4 {
            h.key(Key::RightArrow);
        }
        h.key("1");
        if staged().is_some() {
            break;
        }
        h.key(Key::Escape);
    }
    // Arrow keys on the way also moved the frequency slider, which commits.
    let base = committed();
    let base_text = h.adapter.inspect(|w| {
        let component = w.editor().draft().components()[0].id;
        w.editor().committed_text(EditingField::Parameter(
            component,
            ParameterId::ToneFrequency,
        ))
    });
    assert_eq!(staged(), Some(format!("{base_text}1")));
    let monitor = MonitorWindow::new().unwrap();
    monitor.show().unwrap();
    hide_monitor_window(ui, &monitor, false).unwrap();
    h.settle();
    assert_eq!(
        committed(),
        base,
        "hiding an unfocused monitor must not commit a main-window field"
    );
    h.key(Key::Escape);
    assert!(staged().is_none());
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("e");
    assert_eq!(staged(), Some(format!("{base_text}e")));
    monitor.show().unwrap();
    hide_monitor_window(ui, &monitor, false).unwrap();
    h.settle();
    assert_eq!(
        staged(),
        Some(format!("{base_text}e")),
        "incomplete numeric entry must survive responsive hiding"
    );
    h.key(Key::Escape);
    assert!(staged().is_none());
    // Native deactivation suspends edit commits before Slint delivers focus loss.
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("1");
    ui.set_edits_suspended(true);
    ui.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    monitor.show().unwrap();
    hide_monitor_window(ui, &monitor, true).unwrap();
    ui.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    ui.set_edits_suspended(false);
    h.settle();
    assert_eq!(committed(), base);
    let staged_before = staged();
    h.key("2");
    assert_eq!(
        staged(),
        staged_before,
        "opener handoff must leave the numeric field unfocused"
    );
}
