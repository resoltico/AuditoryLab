//! Window layout, accessibility and input tests. Each test runs on its own
//! thread with its own offscreen platform.
use crate::generated::{AudioControlsWindow, MainWindow, MonitorWindow};
use crate::test_platform::install;
use crate::typography;
use auditorylab_presentation::workspace::monitor::*;
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// The warning text and announcement policy the shared transport view
/// produces, shown on the window under test.
fn sync_monitor_warnings(
    ui: &MainWindow,
    warning: Option<auditorylab_app::workspace::monitor::MonitorWarning>,
    capture_error: Option<&str>,
    underruns: u64,
    clipped: bool,
) {
    let mut text = capture_error.map_or_else(
        || {
            warning.map_or_else(String::new, |warning| {
                auditorylab_presentation::workspace::monitor::monitor_warning_message(warning)
                    .into()
            })
        },
        str::to_owned,
    );
    let live = capture_error.is_some()
        || warning
            .is_some_and(auditorylab_app::workspace::monitor::MonitorWarning::announces_changes);
    if clipped {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(
            auditorylab_presentation::workspace::monitor::monitor_warning_message(
                auditorylab_app::workspace::monitor::MonitorWarning::FixedRangeExceeded,
            ),
        );
    }
    ui.set_monitor_warning(text.into());
    ui.set_monitor_warning_live(live);
    ui.set_backend_warning(
        auditorylab_presentation::workspace::monitor::output_underrun_warning(underruns).into(),
    );
}

#[test]
fn audio_controls_keep_stop_visible() {
    let constraints = install();
    assert_audio_controls_keep_stop_visible(&constraints);
}

#[test]
fn text_edit_cursor_scrolls_into_view() {
    install();
    crate::text_edit_tests::assert_cursor_rect_scrolls_into_view();
}

#[test]
fn control_focus_scrolls_into_view() {
    install();
    crate::focus_scroll_tests::assert_control_focus_scrolls_into_view();
}

#[test]
fn platform_modifiers_drive_text_shortcuts() {
    install();
    assert_platform_modifiers_drive_text_shortcuts();
}

/// Monitor layout: drawer bounds, focused-window fallback, warnings and
/// lanes. Pure Slint layout, so properties are set directly.
#[test]
fn monitor_drawer_and_focused_window_layout() {
    let auxiliary_constraints = install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.set_can_change_waveform(true);
    ui.set_monitor_open(true);
    ui.set_ui_family(typography::resolve().family.into());
    ui.show().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(1100, 900));
    slint::platform::update_timers_and_animations();
    assert!(ui.get_drawer_fits());
    ui.set_scale_label("≈0.0316228".into());
    ui.set_waveform_caption(
        "Preview · not playing · Untitled sound · 40.00 ms · Fit ±≈0.0316228".into(),
    );
    ui.set_meters(
        "Captured sample peak L -30.0 / R -30.0 dBFS · RMS L -33.0 / R -33.0 dBFS".into(),
    );
    ui.set_motion_reason(
        "System motion preference permits live drawing. User chose Return to live.".into(),
    );
    ui.set_monitor_warning("Capture coverage is incomplete; waiting for fresh samples.".into());
    ui.invoke_resize_monitor(240.0);
    slint::platform::update_timers_and_animations();
    assert!(ui.get_drawer_fits());
    assert!(
        ui.get_effective_drawer_height() > 240.0,
        "readable axis labels and current explanations must raise the drawer's content minimum"
    );
    ui.set_motion_reason("".into());
    ui.set_monitor_warning("".into());
    ui.window().set_size(slint::PhysicalSize::new(680, 700));
    slint::platform::update_timers_and_animations();
    // The readable content minimum may exceed the viewport budget. In that
    // case the drawer is hidden and its calculated content height is not a
    // displayed pane; the focused monitor preserves the open choice.
    for requested_height in [240.0, 360.0] {
        ui.invoke_resize_monitor(requested_height);
        slint::platform::update_timers_and_animations();
        if ui.get_drawer_fits() {
            assert!(
                (240.0..=285.0).contains(&ui.get_effective_drawer_height()),
                "a visible minimum-window drawer must retain document space"
            );
        } else {
            assert!(
                ui.get_monitor_open(),
                "focused fallback preserves the open choice"
            );
        }
    }
    ui.set_ui_size(26.0);
    slint::platform::update_timers_and_animations();
    assert!(
        !ui.get_drawer_fits(),
        "enlarged text must use the focused monitor window"
    );
    assert!(
        ui.get_monitor_open(),
        "responsive fallback must preserve the user's open choice"
    );
    assert_monitor_warning_accessibility(&ui);
    assert_monitor_lane_geometry(&ui, &auxiliary_constraints);
    assert_monitor_layout_survives_warning_changes(&ui);
    ui.hide().unwrap();
}

fn assert_monitor_layout_survives_warning_changes(ui: &MainWindow) {
    fn settle(ui: &MainWindow) {
        slint::platform::update_timers_and_animations();
        // Materialize actual conditional delegates, as the native frame does.
        slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
            .draw_contents(|_, _| ());
        let _ = ui.get_drawer_fits();
        slint::platform::update_timers_and_animations();
    }
    ui.set_can_change_waveform(true);
    ui.set_monitor_open(true);
    ui.set_motion_reason("System motion preference permits live drawing.".into());
    ui.invoke_resize_monitor(270.0);
    let mut observed_fit = false;
    let mut observed_fallback = false;
    let mut changed_warning_bounds = false;
    for size in [13.0, 26.0, 40.0] {
        ui.set_ui_size(size);
        for width in [680, 1100] {
            for height in (700..=1200).step_by(5) {
                ui.window()
                    .set_size(slint::PhysicalSize::new(width, height));
                sync_monitor_warnings(ui, None, None, 0, false);
                settle(ui);
                let fits = ui.get_drawer_fits();
                let baseline = ui.get_effective_drawer_height();
                observed_fit |= fits;
                observed_fallback |= !fits;
                for (warning, underruns, clipped) in [
                    (
                        Some(
                            auditorylab_app::workspace::monitor::MonitorWarning::IncompleteCoverage,
                        ),
                        0,
                        false,
                    ),
                    (None, 1, false),
                    (
                        Some(
                            auditorylab_app::workspace::monitor::MonitorWarning::IncompleteCoverage,
                        ),
                        1,
                        true,
                    ),
                ] {
                    sync_monitor_warnings(ui, warning, None, underruns, clipped);
                    settle(ui);
                    assert_eq!(
                        ui.get_drawer_fits(),
                        fits,
                        "warning refresh must not move the monitor between windows at font{size}, {width}x{height}"
                    );
                    changed_warning_bounds |= ui.get_effective_drawer_height() != baseline;
                }
            }
        }
    }
    assert!(
        !changed_warning_bounds,
        "warning rows must keep stable readable bounds"
    );
    assert!(
        observed_fit && observed_fallback,
        "layout control must exercise both drawer and focused-window configurations"
    );
    ui.set_ui_size(13.0);
    ui.window().set_size(slint::PhysicalSize::new(1100, 900));
    sync_monitor_warnings(ui, None, None, 0, false);
    settle(ui);
    assert!(ui.get_drawer_fits());
    eprintln!(
        "stable-warning drawer13px1100x900={} logicalpx",
        ui.get_effective_drawer_height()
    );
    assert!(
        (240.0..=285.0).contains(&ui.get_effective_drawer_height()),
        "stable warnings must preserve the normal bounded drawer target"
    );
}

fn assert_monitor_lane_geometry(
    ui: &MainWindow,
    constraints: &Rc<RefCell<Option<slint::platform::LayoutConstraints>>>,
) {
    use slint::private_unstable_api::re_exports::{
        AccessibleStringProperty, ItemRc, Path, WindowInner,
    };
    use std::ops::ControlFlow;
    fn bounds(item: &ItemRc) -> (f32, f32, f32, f32) {
        let geometry = item.geometry();
        let origin = item.map_to_window(geometry.origin);
        (
            origin.x,
            origin.y,
            geometry.size.width,
            geometry.size.height,
        )
    }
    fn settle(window: &slint::Window) {
        slint::platform::update_timers_and_animations();
        WindowInner::from_pub(window).draw_contents(|_, _| ());
        slint::platform::update_timers_and_animations();
    }
    fn check(window: &slint::Window, size: f32, context: &str, expected_controls: usize) {
        let viewport = window.size().to_logical(window.scale_factor());
        let root = WindowInner::from_pub(window).window_item_rc().unwrap();
        let mut lanes = Vec::new();
        let mut controls = 0;
        let _ = root.visit_descendants(|item| {
            if context == "drawer" && !item.is_visible() { return ControlFlow::<()>::Continue(()) }
            if let Some(label) = item.accessible_string_property(AccessibleStringProperty::Label) {
                if label.starts_with("Left samples,") || label.starts_with("Right samples,") {
                    lanes.push(item.clone());
                }
                if matches!(label.as_str(), "Output monitor" | "Waveform time window" | "Waveform amplitude scale" | "Hold display" | "Return to live" | "Full audio diagnostics and capture source" | "Hide output monitor" | "Follow system motion preference") {
                    let (x,y,width,height) = bounds(item);
                    assert!(width > 0.0 && height > 0.0, "{context}: {label} needs a usable extent");
                    if context == "drawer" {
                        assert!(x >= 0.0 && y >= 0.0 && x+width <= viewport.width+0.01 && y+height <= viewport.height+0.01, "{context} at{size}px: {label} must remain inside its window, got{:?} in{viewport:?}", (x,y,width,height));
                    }
                    controls += 1;
                }
            }
            ControlFlow::<()>::Continue(())
        });
        assert_eq!(
            controls, expected_controls,
            "all monitor header and chart controls remain reachable"
        );
        assert_eq!(lanes.len(), 2, "both ear lanes remain visible at {size}px");
        let mut plots = Vec::new();
        for lane in lanes {
            let lane_bounds = bounds(&lane);
            let mut labels = Vec::new();
            let mut plot = None;
            let _ = lane.visit_descendants(|item| {
                if item.downcast::<Path>().is_some() {
                    plot = Some(bounds(item));
                }
                if let Some(label) =
                    item.accessible_string_property(AccessibleStringProperty::Label)
                    && !label.is_empty()
                {
                    labels.push((label.to_string(), bounds(item)));
                }
                ControlFlow::<()>::Continue(())
            });
            let title = labels
                .iter()
                .find(|(label, _)| matches!(label.as_str(), "Left" | "Right"))
                .unwrap()
                .1;
            let positive = labels
                .iter()
                .find(|(label, _)| label.starts_with('+'))
                .unwrap()
                .1;
            let zero = labels.iter().find(|(label, _)| label == "0").unwrap().1;
            let negative = labels
                .iter()
                .find(|(label, _)| label.starts_with('−'))
                .unwrap()
                .1;
            let plot = plot.expect("the actual generated lane contains its sample path");
            plots.push(plot);
            eprintln!(
                "Lane {context} at{size}px viewport{viewport:?}: lane={lane_bounds:?}, title={title:?}, +={positive:?}, 0={zero:?}, -={negative:?}, plot={plot:?}"
            );
            assert!(
                title.0 + title.2 <= positive.0,
                "ear label and numeric ticks need separate columns"
            );
            assert!(
                (title.1 + title.3 / 2.0 - zero.1 - zero.3 / 2.0).abs() < 0.01,
                "ear label stays centered on the numeric scale"
            );
            for tick in [positive, zero, negative] {
                assert!(
                    tick.0 + tick.2 <= plot.0,
                    "full numeric labels remain outside the plot"
                );
                assert!(
                    tick.1 >= lane_bounds.1
                        && tick.1 + tick.3 <= lane_bounds.1 + lane_bounds.3 + 0.01,
                    "numeric labels remain inside their lane"
                );
            }
            assert!(
                plot.2 >= plot.3 * 2.0 - 0.01
                    && plot.0 + plot.2 <= lane_bounds.0 + lane_bounds.2 + 0.01,
                "the plot must fit beside its readable gutter with at least twice its height in time-axis space"
            );
            assert!((positive.1 + positive.3 / 2.0 - plot.1).abs() < 0.01);
            assert!(
                (negative.1 + negative.3 / 2.0 - plot.1 - plot.3).abs() < 0.01,
                "the plot retains the full distance between the positive and negative ticks"
            );
        }
        assert!(
            (plots[0].0 - plots[1].0).abs() < 0.01
                && (plots[0].2 - plots[1].2).abs() < 0.01
                && (plots[0].3 - plots[1].3).abs() < 0.01,
            "both ears must retain the same plot origin, time scale, and amplitude height"
        );
    }
    ui.set_can_change_waveform(true);
    ui.set_monitor_open(true);
    ui.invoke_resize_monitor(270.0);
    let window = MonitorWindow::new().unwrap();
    crate::adapter::project_captions(
        window.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    window.set_ui_family(ui.get_ui_family());
    let mut drawer_cases = 0;
    for size in [40.0, 32.666_668, 26.0, 13.0] {
        window.set_ui_size(size);
        window.show().unwrap();
        settle(window.window());
        let minimum = constraints.borrow().unwrap().min.unwrap();
        assert_eq!(
            minimum,
            slint::LogicalSize::new(680.0, 400.0),
            "focused content must not force a window beyond the desktop"
        );
        for (width, height) in [(680.0_f32, 700.0), (1280.0, 800.0)] {
            window
                .window()
                .set_size(slint::LogicalSize::new(width, height));
            for state in ["Live", "Held", "Unavailable"] {
                let mut data = ui.get_monitor_data();
                data.state = state.into();
                data.age = "Current session receipt 123456 milliseconds ago; full receipt provenance remains accessible".into();
                data.held = state == "Held";
                data.settingsenabled = state != "Held";
                data.overridden = true;
                data.motion = "System Reduce Motion is enabled: display held by default.".into();
                data.backendwarning = "Backend-reported output underruns: 8 · see details.".into();
                window.set_data(data.clone());
                settle(window.window());
                check(window.window(), size, "focused", 7);
                assert_focused_monitor_reachable(&window, size, &data);
            }
            ui.set_ui_size(size);
            ui.window().set_size(slint::LogicalSize::new(width, 1200.0));
            settle(ui.window());
            if ui.get_drawer_fits() {
                drawer_cases += 1;
                check(ui.window(), size, "drawer", 7);
            } else {
                assert!(
                    ui.get_monitor_open(),
                    "responsive fallback preserves the open monitor choice"
                );
            }
        }
        window.hide().unwrap();
    }
    assert!(
        drawer_cases > 0,
        "geometry checks must include the visible drawer"
    );
    ui.set_ui_size(13.0);
    ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
}

fn assert_focused_monitor_reachable(
    window: &MonitorWindow,
    size: f32,
    data: &crate::generated::MonitorData,
) {
    use slint::platform::{Key, WindowEvent};
    use slint::private_unstable_api::re_exports::{
        AccessibleStringProperty, ItemRc, Orientation, WindowInner,
    };
    use std::collections::BTreeSet;
    use std::ops::ControlFlow;
    fn settle(window: &slint::Window) {
        slint::platform::update_timers_and_animations();
        WindowInner::from_pub(window).draw_contents(|_, _| ());
        slint::platform::update_timers_and_animations();
    }
    fn bounds(item: &ItemRc) -> (f32, f32, f32, f32) {
        let geometry = item.geometry();
        let origin = item.map_to_window(geometry.origin);
        (
            origin.x,
            origin.y,
            geometry.size.width,
            geometry.size.height,
        )
    }
    fn key(window: &slint::Window, key: Key) {
        let text: slint::SharedString = key.into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        settle(window);
    }
    let inner = WindowInner::from_pub(window.window());
    let root = inner.window_item_rc().unwrap();
    let find = |label: &str| {
        let mut matches = Vec::new();
        let _ = root.visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|value| value == label)
            {
                matches.push(item.clone());
            }
            ControlFlow::<()>::Continue(())
        });
        assert_eq!(matches.len(), 1, "one actual monitor item for {label}");
        matches.pop().unwrap()
    };
    let state = find(data.state.as_str());
    let state_bounds = bounds(&state);
    let state_intrinsic = state.borrow().as_ref().layout_info(
        Orientation::Horizontal,
        state_bounds.3,
        &inner.window_adapter(),
        &state,
    );
    assert!(
        state_bounds.2 + 0.1 >= state_intrinsic.preferred,
        "{} must retain its complete shaped width {} at {size}px: {state_bounds:?}",
        data.state,
        state_intrinsic.preferred
    );
    assert!(
        state_bounds.3 > size * 0.8,
        "monitor state cannot collapse vertically"
    );
    let age = find(data.age.as_str());
    let age_bounds = bounds(&age);
    let age_intrinsic = age.borrow().as_ref().layout_info(
        Orientation::Horizontal,
        age_bounds.3,
        &inner.window_adapter(),
        &age,
    );
    assert!(
        age_bounds.2 > 0.0 && age_bounds.2 + 0.1 >= age_intrinsic.min,
        "receipt age retains at least its shaped elision marker"
    );
    let age_line = age.borrow().as_ref().layout_info(
        Orientation::Vertical,
        age_bounds.2,
        &inner.window_adapter(),
        &age,
    );
    assert!(
        age_line.preferred <= size * 1.7 && age_bounds.3 + 0.1 >= age_line.preferred,
        "receipt age must fit its complete single shaped line at {size}px: {age_bounds:?}, {age_line:?}"
    );
    let labels = [
        "Full audio diagnostics and capture source",
        "Hide output monitor",
        "Waveform time window",
        "Waveform amplitude scale",
        if data.held {
            "Return to live"
        } else {
            "Hold display"
        },
        "Follow system motion preference",
    ];
    let controls: Vec<_> = labels.iter().map(|label| (*label, find(label))).collect();
    slint::slint! {
        import { Button } from "std-widgets.slint";
        export component MonitorButtonMeasure inherits Window {
            in property <string> family;
            in property <length> size;
            default-font-family:root.family;default-font-size:root.size;
            out property <length> natural-height:max(measure.min-height,measure.preferred-height);
            out property <length> widget-minimum-height:measure.min-height;
            out property <length> widget-preferred-height:measure.preferred-height;
            measure := Button {text:"Details…";}
        }
    }
    let measure = MonitorButtonMeasure::new().unwrap();
    measure.set_family(window.get_ui_family());
    measure.set_size(size);
    let natural_height = measure.get_natural_height();
    eprintln!(
        "Actual widget height oracle {size}px: minimum={}, preferred={}, admitted={natural_height}",
        measure.get_widget_minimum_height(),
        measure.get_widget_preferred_height()
    );
    assert!(
        natural_height > 0.0,
        "standard button height oracle is meaningful"
    );
    for (label, item) in &controls[..2] {
        assert!(
            bounds(item).3 <= natural_height + 0.1,
            "{label} must retain its natural height at {size}px: actual={:?}, preferred={natural_height}",
            bounds(item)
        );
    }
    let viewport = window
        .window()
        .size()
        .to_logical(window.window().scale_factor());
    let mut seen = BTreeSet::new();
    for reverse in [false, true] {
        let mut direction_seen = BTreeSet::new();
        if reverse {
            window.window().dispatch_event(WindowEvent::KeyPressed {
                text: Key::Shift.into(),
            });
        }
        for _ in 0..18 {
            key(window.window(), Key::Tab);
            let focused = inner
                .focus_item
                .borrow()
                .upgrade()
                .expect("Tab focuses a monitor control");
            for (label, item) in &controls {
                let mut contains_focus = *item == focused;
                let _ = item.visit_descendants(|child| {
                    contains_focus |= *child == focused;
                    ControlFlow::<()>::Continue(())
                });
                if contains_focus {
                    let (x, y, width, height) = bounds(item);
                    if x < -0.1
                        || y < -0.1
                        || x + width > viewport.width + 0.1
                        || y + height > viewport.height + 0.1
                    {
                        eprintln!(
                            "Focus reveal diagnostic: focused={:?}, control={:?}",
                            bounds(&focused),
                            bounds(item)
                        );
                        let _ = root.visit_descendants(|candidate| {
                            if let Some(flick) = candidate
                                .downcast::<slint::private_unstable_api::re_exports::Flickable>(
                            ) {
                                let flick = flick.as_pin_ref();
                                eprintln!(
                                    "Flickable {:?} content={:?}x{:?}, offset={:?},{:?}",
                                    bounds(candidate),
                                    flick.content_width(),
                                    flick.content_height(),
                                    flick.content_x(),
                                    flick.content_y()
                                );
                            }
                            ControlFlow::<()>::Continue(())
                        });
                    }
                    assert!(
                        x >= -0.1
                            && y >= -0.1
                            && x + width <= viewport.width + 0.1
                            && y + height <= viewport.height + 0.1,
                        "Tab reverse={reverse} must reveal {label} at {size}px: {:?} in {viewport:?}",
                        (x, y, width, height)
                    );
                    seen.insert(*label);
                    direction_seen.insert(*label);
                }
            }
        }
        for (label, _) in &controls {
            if data.held && matches!(*label, "Waveform time window" | "Waveform amplitude scale") {
                continue;
            }
            assert!(
                direction_seen.contains(label),
                "Tab reverse={reverse} must reach {label}"
            );
        }
        if reverse {
            window.window().dispatch_event(WindowEvent::KeyReleased {
                text: Key::Shift.into(),
            });
        }
    }
    for (label, _) in &controls {
        if data.held && matches!(*label, "Waveform time window" | "Waveform amplitude scale") {
            continue;
        }
        assert!(
            seen.contains(label),
            "actual Tab traversal must reach {label}"
        );
    }
    let flickable = root
        .visit_descendants(|item| {
            item.downcast::<slint::private_unstable_api::re_exports::Flickable>()
                .map_or(ControlFlow::Continue(()), |flickable| {
                    ControlFlow::Break(flickable)
                })
        })
        .unwrap();
    let flickable = flickable.as_pin_ref();
    let bottom = (viewport.height - flickable.content_height().get()).min(0.0);
    for _ in 0..8 {
        let before = flickable.content_y().get();
        key(window.window(), Key::PageDown);
        let after = flickable.content_y().get();
        assert!(
            after >= bottom - 0.1 && after <= before + 0.1 && after <= 0.1,
            "PageDown must advance within its actual scroll bounds"
        );
        if (after - before).abs() < 0.1 {
            break;
        }
    }
    assert!(
        (flickable.content_y().get() - bottom).abs() < 0.1,
        "keyboard paging must reach the warning end of the focused monitor"
    );
    key(window.window(), Key::PageUp);
    assert!(flickable.content_y().get() >= bottom - 0.1 && flickable.content_y().get() <= 0.1);
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(window.window(), Key::End);
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    settle(window.window());
    for text in [&data.motion, &data.backendwarning] {
        let (x, y, width, height) = bounds(&find(text.as_str()));
        assert!(
            width > 0.0
                && height > 0.0
                && x < viewport.width
                && x + width > 0.0
                && y >= -0.1
                && y + height <= viewport.height + 0.1,
            "Ctrl/Cmd End must reach warning text with only the keyboard"
        );
    }
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(window.window(), Key::Home);
    window.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    settle(window.window());
    let (x, y, width, height) = bounds(&find(data.state.as_str()));
    assert!(
        x >= -0.1
            && y >= -0.1
            && x + width <= viewport.width + 0.1
            && y + height <= viewport.height + 0.1,
        "Ctrl/Cmd Home must return left/top to the complete monitor state"
    );
    window
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(viewport.width / 2.0, viewport.height / 2.0),
            delta_x: -100_000.0,
            delta_y: -100_000.0,
        });
    settle(window.window());
    for text in [&data.motion, &data.backendwarning] {
        let (x, y, width, height) = bounds(&find(text.as_str()));
        assert!(
            width > 0.0
                && height > 0.0
                && x < viewport.width
                && x + width > 0.0
                && y >= -0.1
                && y + height <= viewport.height + 0.1,
            "scrolling must reach the complete warning row at {size}px: {:?}",
            (x, y, width, height)
        );
    }
    eprintln!(
        "Focused monitor {size}px {viewport:?} state={} reachable controls={seen:?}",
        data.state
    );
}

fn assert_monitor_warning_accessibility(ui: &MainWindow) {
    use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
    use std::ops::ControlFlow;
    fn reveal_warnings(window: &slint::Window) {
        slint::platform::update_timers_and_animations();
        WindowInner::from_pub(window).draw_contents(|_, _| ());
        let viewport = window.size().to_logical(window.scale_factor());
        window.dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(viewport.width / 2.0, viewport.height / 2.0),
            delta_x: 0.0,
            delta_y: -100_000.0,
        });
        slint::platform::update_timers_and_animations();
        WindowInner::from_pub(window).draw_contents(|_, _| ());
    }
    let window = MonitorWindow::new().unwrap();
    crate::adapter::project_captions(
        window.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    window.show().unwrap();
    window
        .window()
        .set_size(slint::LogicalSize::new(950.0, 420.0));
    let root = WindowInner::from_pub(window.window())
        .window_item_rc()
        .unwrap();
    let find = |label: &str| {
        root.visit_descendants(|item| {
            if item.is_visible()
                && item.is_accessible()
                && item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .is_some_and(|value| value == label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
    };
    let backend_text = "Backend-reported output underruns: 1 · see details.";
    sync_monitor_warnings(ui, None, None, 1, false);
    window.set_data(ui.get_monitor_data());
    reveal_warnings(window.window());
    let backend = find(backend_text).expect("independent backend warning must be accessible");
    assert_eq!(
        backend
            .accessible_string_property(AccessibleStringProperty::LiveRegion)
            .as_deref(),
        Some("polite")
    );
    for _ in 0..10 {
        let partial = auditorylab_app::workspace::monitor::MonitorWarning::IncompleteCoverage;
        sync_monitor_warnings(ui, Some(partial), None, 1, false);
        window.set_data(ui.get_monitor_data());
        reveal_warnings(window.window());
        let coverage =
            find(auditorylab_presentation::workspace::monitor::monitor_warning_message(partial))
                .expect("current partial coverage remains readable");
        assert_eq!(
            coverage
                .accessible_string_property(AccessibleStringProperty::LiveRegion)
                .as_deref(),
            Some("off"),
            "transient coverage must not generate automatic speech"
        );
        assert!(
            find(backend_text).is_some_and(|item| item == backend),
            "coverage changes must not recreate the backend warning form"
        );
        sync_monitor_warnings(ui, None, None, 1, false);
        window.set_data(ui.get_monitor_data());
        reveal_warnings(window.window());
        assert!(
            find(auditorylab_presentation::workspace::monitor::monitor_warning_message(partial))
                .is_none()
        );
        assert!(find(backend_text).is_some_and(|item| item == backend));
    }
    {
        let critical = auditorylab_app::workspace::monitor::MonitorWarning::Stale;
        sync_monitor_warnings(ui, Some(critical), None, 1, false);
        window.set_data(ui.get_monitor_data());
        reveal_warnings(window.window());
        let warning =
            find(auditorylab_presentation::workspace::monitor::monitor_warning_message(critical))
                .unwrap();
        assert_eq!(
            warning
                .accessible_string_property(AccessibleStringProperty::LiveRegion)
                .as_deref(),
            Some("polite")
        );
        assert!(find(backend_text).is_some_and(|item| item == backend));
    }
    let capture_error =
        "Invalid waveform capture (non-finite samples); retaining the last valid window";
    sync_monitor_warnings(ui, None, Some(capture_error), 1, false);
    window.set_data(ui.get_monitor_data());
    slint::platform::update_timers_and_animations();
    assert_eq!(
        find(capture_error)
            .unwrap()
            .accessible_string_property(AccessibleStringProperty::LiveRegion)
            .as_deref(),
        Some("polite")
    );
    sync_monitor_warnings(ui, None, None, 1, true);
    window.set_data(ui.get_monitor_data());
    slint::platform::update_timers_and_animations();
    let advisory = find("Waveform exceeds the fixed display range; exact levels are retained. Use Fit to view the full extent.").unwrap();
    assert_eq!(
        advisory
            .accessible_string_property(AccessibleStringProperty::LiveRegion)
            .as_deref(),
        Some("off")
    );
    assert!(find(backend_text).is_some_and(|item| item == backend));
    sync_monitor_warnings(ui, None, None, 0, false);
    window.set_data(ui.get_monitor_data());
    slint::platform::update_timers_and_animations();
    assert!(find(backend_text).is_none());
    window.hide().unwrap();
}

fn assert_platform_modifiers_drive_text_shortcuts() {
    use slint::platform::{Key, WindowEvent};
    use slint::private_unstable_api::re_exports::{KeyboardModifiers, WindowInner};
    slint::slint! {
        export component ModifierProbe inherits Window {
            width:400px;height:80px;
            in-out property <string> value:"baseline";
            out property <bool> pressed-control;
            out property <bool> pressed-alt;
            out property <bool> pressed-shift;
            out property <bool> pressed-meta;
            out property <bool> released-control;
            public function focus-field(){field.focus();}
            field := TextInput {
                text <=> root.value;
                key-pressed(e)=>{
                    root.pressed-control=e.modifiers.control;
                    root.pressed-alt=e.modifiers.alt;
                    root.pressed-shift=e.modifiers.shift;
                    root.pressed-meta=e.modifiers.meta;
                    return reject;
                }
                key-released(e)=>{root.released-control=e.modifiers.control;return reject;}
            }
        }
    }
    fn press(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
        probe
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    }
    fn release(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
        probe
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
    }
    fn key(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
        let text = text.into();
        press(probe, text.clone());
        release(probe, text);
    }
    fn reset(probe: &ModifierProbe) {
        probe
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(false));
        probe
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
        probe.invoke_focus_field();
    }
    let probe = ModifierProbe::new().unwrap();
    probe.show().unwrap();
    reset(&probe);
    let inner = WindowInner::from_pub(probe.window());
    let mut command = KeyboardModifiers::default();
    command.control = true;
    inner.set_keyboard_modifiers(KeyboardModifiers::default());
    press(&probe, Key::Shift);
    assert!(
        probe.get_pressed_shift(),
        "physical Shift callback precedes its aggregate report"
    );
    release(&probe, Key::Shift);
    press(&probe, Key::Control);
    assert!(probe.get_pressed_control());
    release(&probe, Key::Control);
    press(&probe, Key::Meta);
    assert!(probe.get_pressed_meta());
    release(&probe, Key::Meta);
    press(&probe, Key::Alt);
    assert!(probe.get_pressed_alt());
    release(&probe, Key::Alt);
    inner.set_keyboard_modifiers(command);
    key(&probe, "a");
    inner.set_keyboard_modifiers(KeyboardModifiers::default());
    key(&probe, "x");
    assert_eq!(
        probe.get_value(),
        "x",
        "flag-only command selects actual TextInput content"
    );
    key(&probe, "a");
    assert_eq!(
        probe.get_value(),
        "xa",
        "released platform flags cannot stick on plain input"
    );

    // Without platform reports, preserve the physical left/right tracker.
    reset(&probe);
    press(&probe, Key::Control);
    press(&probe, Key::ControlR);
    release(&probe, Key::Control);
    assert!(probe.get_released_control());
    release(&probe, Key::ControlR);
    assert!(!probe.get_released_control());

    // Winit can report physical releases before the next aggregate flags.
    // Keep the last authoritative aggregate until that following report.
    inner.set_keyboard_modifiers(command);
    press(&probe, Key::Control);
    press(&probe, Key::ControlR);
    release(&probe, Key::Control);
    assert!(probe.get_released_control());
    release(&probe, Key::ControlR);
    assert!(
        !probe.get_released_control(),
        "modifier callback retains immediate physical family"
    );
    key(&probe, "b");
    assert!(
        probe.get_pressed_control(),
        "ordinary input retains the last platform aggregate"
    );
    inner.set_keyboard_modifiers(KeyboardModifiers::default());
    key(&probe, "b");
    assert!(!probe.get_pressed_control());
    inner.set_keyboard_modifiers(command);
    press(&probe, Key::Control);
    press(&probe, Key::ControlR);
    inner.set_keyboard_modifiers(KeyboardModifiers::default());
    release(&probe, Key::Control);
    assert!(
        !probe.get_released_control(),
        "aggregate false clears both known sides before release"
    );
    release(&probe, Key::ControlR);
    assert!(!probe.get_released_control());

    let mut all = KeyboardModifiers::default();
    all.control = true;
    all.alt = true;
    all.shift = true;
    all.meta = true;
    inner.set_keyboard_modifiers(all);
    key(&probe, "b");
    assert!(
        probe.get_pressed_control()
            && probe.get_pressed_alt()
            && probe.get_pressed_shift()
            && probe.get_pressed_meta()
    );
    reset(&probe);
    key(&probe, "b");
    assert!(
        !probe.get_pressed_control()
            && !probe.get_pressed_alt()
            && !probe.get_pressed_shift()
            && !probe.get_pressed_meta(),
        "focus loss clears aggregate and physical state"
    );

    #[cfg(target_os = "windows")]
    {
        // Clearing aggregate Alt also clears a prior physical AltGr flag.
        // Otherwise the existing Ctrl+Alt text heuristic would be skipped.
        press(&probe, Key::AltGr);
        inner.set_keyboard_modifiers(KeyboardModifiers::default());
        let mut control_alt = KeyboardModifiers::default();
        control_alt.control = true;
        control_alt.alt = true;
        inner.set_keyboard_modifiers(control_alt);
        key(&probe, "@");
        assert!(!probe.get_pressed_control() && !probe.get_pressed_alt());
    }
    probe
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(false));
    probe.hide().unwrap();
}

fn assert_audio_controls_keep_stop_visible(
    constraints: &Rc<RefCell<Option<slint::platform::LayoutConstraints>>>,
) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let controls = AudioControlsWindow::new().unwrap();
    controls.set_ui_family(typography::resolve().family.into());
    controls.set_active(true);
    controls
        .global::<crate::generated::Actions>()
        .set_stop(crate::generated::Action {
            label: auditorylab_presentation::workspace::operation_label(
                auditorylab_app::vocabulary::Operation::Stop,
            )
            .into(),
            enabled: true,
        });
    controls.set_elapsed("59:59:59 / 99:59:59".into());
    let stops = Rc::new(Cell::new(0));
    let calls = stops.clone();
    controls.on_stop(move || calls.set(calls.get() + 1));
    let mut previous_height = 0.0;
    for (index, size) in [13.0, 26.0, 40.0].into_iter().enumerate() {
        controls.set_ui_size(size);
        controls.set_source("Short source".into());
        controls.show().unwrap();
        let short = constraints.borrow().unwrap().min.unwrap();
        controls.hide().unwrap();
        controls.set_source(
            "A long playing source with a detailed session description "
                .repeat(80)
                .into(),
        );
        controls.show().unwrap();
        let minimum = constraints.borrow().unwrap().min.unwrap();
        eprintln!("Audio controls at {size}px: native minimum {minimum:?}");
        assert!(
            minimum.width >= 320.0,
            "native minimum must keep the audio controls usable"
        );
        assert_eq!(
            minimum, short,
            "source length must not force an unbounded window"
        );
        assert!(
            minimum.height > previous_height,
            "content minimum must grow with native text metrics"
        );
        previous_height = minimum.height;
        controls.window().set_size(minimum);
        slint::platform::update_timers_and_animations();
        // Clicks inside the bottom row of the actual generated window at its native minimum.
        let position = slint::LogicalPosition::new(minimum.width / 2.0, minimum.height - 24.0);
        controls
            .window()
            .dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
        controls
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        assert_eq!(
            stops.get(),
            index + 1,
            "Stop must be clickable inside the visible client bounds"
        );
        controls.hide().unwrap();
    }
}

/// The transport's play/stop button and its neighbours keep their exact
/// geometry while the label and the clock change, at ordinary and large text.
#[test]
fn transport_controls_keep_their_geometry_while_labels_and_clock_change() {
    use slint::platform::{Key, WindowEvent};
    use slint::private_unstable_api::re_exports::WindowInner;
    use slint::{Model, VecModel};
    install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.set_ui_family(typography::resolve().family.into());
    ui.set_transport_labels(slint::ModelRc::new(VecModel::from(
        crate::labels::transport_labels()
            .iter()
            .map(|label| slint::SharedString::from(*label))
            .collect::<Vec<_>>(),
    )));
    ui.set_playback_busy(true);
    ui.set_transport_enabled(true);
    ui.show().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    let window = ui.window();
    let bounds_of = |label: &str| exposed_bounds(window, label);
    for size in [13.0, 26.0] {
        ui.set_ui_size(size);
        let mut reference = None;
        for label in crate::labels::transport_labels() {
            for (elapsed, status) in [
                ("0:00", "Preparing output… · A"),
                (
                    "0:09 / 5:00",
                    "Playing · 5 Hz binaural beat · Finite recipe",
                ),
                ("10:00 / 5:00:00", ""),
                ("26:03:04", &"A very long title ".repeat(30)),
            ] {
                ui.set_transport_action(label.into());
                ui.set_elapsed(elapsed.into());
                ui.set_player_title(status.into());
                ui.set_generated(status.into());
                crate::test_platform::settle(window);
                let geometry = (bounds_of(label), bounds_of("Audio details"));
                match reference {
                    None => reference = Some(geometry),
                    Some(expected) => assert_eq!(
                        geometry, expected,
                        "{label} with {elapsed} at {size}px moved the transport"
                    ),
                }
            }
        }
    }
    // The hidden measuring buttons never take keyboard focus.
    let inner = WindowInner::from_pub(window);
    window.dispatch_event(WindowEvent::WindowActiveChanged(true));
    for _ in 0..60 {
        let text: slint::SharedString = Key::Tab.into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
        crate::test_platform::settle(window);
        if let Some(focused) = inner.focus_item.borrow().upgrade() {
            assert!(
                focused.is_visible(),
                "Tab reached a hidden measuring control"
            );
        }
    }
    let _ = ui.get_transport_labels().row_count();
    ui.hide().unwrap();
}

/// The window bounds of the one visible accessible item labelled `label`.
fn exposed_bounds(window: &slint::Window, label: &str) -> (f32, f32, f32, f32) {
    use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
    use std::ops::ControlFlow;
    let root = WindowInner::from_pub(window).window_item_rc().unwrap();
    let mut found = Vec::new();
    let _ = root.visit_descendants(|item| {
        if item.is_visible()
            && item.is_accessible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|value| value == label)
        {
            let geometry = item.geometry();
            let origin = item.map_to_window(geometry.origin);
            found.push((
                origin.x,
                origin.y,
                geometry.size.width,
                geometry.size.height,
            ));
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        found.len(),
        1,
        "exactly one exposed control is labelled {label}"
    );
    found[0]
}

/// When an export ends, Cancel export leaves an empty space, so a late press
/// never lands on another control.
#[test]
fn a_finished_export_leaves_no_control_under_cancel_export() {
    install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.set_status("Exporting WAV".into());
    ui.set_exporting(true);
    ui.global::<crate::generated::Actions>()
        .set_cancel_export(crate::generated::Action {
            label: auditorylab_presentation::workspace::operation_label(
                auditorylab_app::vocabulary::Operation::CancelExport,
            )
            .into(),
            enabled: true,
        });
    ui.show().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    crate::test_platform::settle(ui.window());
    let details = exposed_bounds(ui.window(), "Task details…");
    let (x, y, width, height) = exposed_bounds(ui.window(), "Cancel export");
    ui.set_exporting(false);
    ui.set_status("Exported".into());
    crate::test_platform::settle(ui.window());
    assert_eq!(exposed_bounds(ui.window(), "Task details…"), details);
    let (dx, _, dwidth, _) = details;
    assert!(
        dx + dwidth <= x || x + width <= dx,
        "Task details at {details:?} took Cancel export's place at {:?}",
        (x, y, width, height)
    );
    ui.hide().unwrap();
}

/// A recipe without a study or summary leaves no blank lines in its details.
#[test]
fn absent_recipe_texts_leave_no_gap() {
    install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.set_has_results(true);
    ui.set_sound_title("Title".into());
    ui.set_structure("Structure".into());
    ui.set_recipe_timing("Timing".into());
    ui.show().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    crate::test_platform::settle(ui.window());
    let below = |label| {
        let (_, y, _, height) = exposed_bounds(ui.window(), label);
        y + height
    };
    let top = |label| exposed_bounds(ui.window(), label).1;
    assert_eq!(
        top("Timing") - below("Structure"),
        top("Structure") - below("Title")
    );
    ui.hide().unwrap();
}

/// An assistive technology's press reaches a button only while it is enabled.
#[test]
fn accessibility_presses_respect_disabled_buttons() {
    use slint::private_unstable_api::re_exports::{
        AccessibilityAction, AccessibleStringProperty, WindowInner,
    };
    use std::ops::ControlFlow;
    slint::slint! {
        import { Button } from "../ui/button.slint";
        export component PressProbe inherits Window {
            in property <bool> enabled;
            out property <int> presses;
            Button { text: "Probe"; enabled: root.enabled; triggered => { root.presses += 1; } }
        }
    }
    install();
    let probe = PressProbe::new().unwrap();
    probe.show().unwrap();
    let press = |probe: &PressProbe| {
        let root = WindowInner::from_pub(probe.window())
            .window_item_rc()
            .unwrap();
        let button = root
            .visit_descendants(|item| {
                if item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .is_some_and(|label| label == "Probe")
                {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        button.accessible_action(&AccessibilityAction::Default);
    };
    probe.set_enabled(false);
    press(&probe);
    assert_eq!(
        probe.get_presses(),
        0,
        "a disabled button ignores the press"
    );
    probe.set_enabled(true);
    press(&probe);
    assert_eq!(probe.get_presses(), 1);
    probe.hide().unwrap();
}

/// Long unbreakable text wraps inside the document instead of widening it.
#[test]
fn unbreakable_text_never_widens_the_documents() {
    use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
    use std::ops::ControlFlow;
    install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.show().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    let long = "Supercalifragilisticexpialidocious_".repeat(30);
    ui.set_status(long.as_str().into());
    ui.set_draft_brief(long.as_str().into());
    ui.set_retained_warning(long.as_str().into());
    for tab in [1, 0] {
        ui.set_tab_index(tab);
        crate::test_platform::settle(ui.window());
        let root = WindowInner::from_pub(ui.window()).window_item_rc().unwrap();
        let mut checked = 0;
        let _ = root.visit_descendants(|item| {
            let label = item.accessible_string_property(AccessibleStringProperty::Label);
            if item.is_visible()
                && label
                    .as_deref()
                    .is_some_and(|l| matches!(l, "Sound title" | "Find a recipe"))
            {
                let geometry = item.geometry();
                let origin = item.map_to_window(geometry.origin);
                assert!(
                    origin.x >= 0.0 && origin.x + geometry.size.width <= 1100.0 + 0.1,
                    "{label:?} must stay inside the window: {origin:?} {geometry:?}"
                );
                checked += 1;
            }
            ControlFlow::<()>::Continue(())
        });
        assert!(checked > 0, "the tab's main field was checked");
    }
    ui.hide().unwrap();
}

#[test]
fn about_geometry_keeps_identity_icon_and_actions_visible_at_font_and_display_scales() {
    use crate::generated::AboutWindow;
    use crate::test_platform::settle;
    use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
    use std::ops::ControlFlow;
    install();
    let about = AboutWindow::new().unwrap();
    crate::native_text::initialize_identity(about.global::<crate::generated::Terms>());
    about.set_identity("Version 0.4.0 (Build 2)".into());
    about.show().unwrap();
    for scale in [1.0, 2.0] {
        about
            .window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
        for size in [13.0_f32, 26.0, 40.0] {
            about.set_ui_size(size);
            let width = 420.0_f32.max(size * 30.0);
            let height = 410.0_f32.max(size * 27.0);
            about
                .window()
                .set_size(slint::LogicalSize::new(width, height));
            settle(about.window());
            let root = WindowInner::from_pub(about.window())
                .window_item_rc()
                .unwrap();
            let mut observed = 0;
            let _ = root.visit_descendants(|item| {
                if let Some(label) =
                    item.accessible_string_property(AccessibleStringProperty::Label)
                    && (matches!(
                        label.as_str(),
                        "Application icon"
                            | "Auditory Lab"
                            | "Version 0.4.0 (Build 2)"
                            | "Copyright © 2026 Ervins Strauhmanis"
                            | "Project website"
                            | "Licenses"
                            | "#MadeWithSlint"
                    ) || label.starts_with("Play and export"))
                {
                    let geometry = item.geometry();
                    let origin = item.map_to_window(geometry.origin);
                    assert!(
                        origin.x >= 0.0
                            && origin.y >= 0.0
                            && geometry.size.width > 0.0
                            && geometry.size.height > 0.0
                            && origin.x + geometry.size.width <= width + 0.1
                            && origin.y + geometry.size.height <= height + 0.1,
                        "{label} clipped at font {size}, scale {scale}: {geometry:?} {origin:?}"
                    );
                    if label == "Application icon" {
                        assert_eq!((geometry.size.width, geometry.size.height), (64.0, 64.0));
                    }
                    observed += 1;
                }
                ControlFlow::<()>::Continue(())
            });
            assert_eq!(observed, 8, "all required About contents are reachable");
            if let Ok(directory) = std::env::var("AUDITORYLAB_TEST_SCREENSHOTS") {
                use std::io::Write;
                std::fs::create_dir_all(&directory).unwrap();
                let pixels = about.window().take_snapshot().unwrap();
                let mut file = std::fs::File::create(
                    std::path::Path::new(&directory)
                        .join(format!("about-font{size}-scale{scale}.ppm")),
                )
                .unwrap();
                write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
                for pixel in pixels.as_slice() {
                    file.write_all(&[pixel.r, pixel.g, pixel.b]).unwrap();
                }
            }
        }
    }
}

#[test]
fn incomplete_filtered_count_header_fits_large_text_in_compact_window() {
    install();
    let ui = MainWindow::new().unwrap();
    crate::adapter::project_captions(
        ui.global::<crate::generated::ViewCaptions>(),
        auditorylab_app::vocabulary::EditorStatus::UnmodifiedDraft,
    );
    crate::native_text::initialize_identity(ui.global::<crate::generated::Terms>());
    ui.set_ui_family(typography::resolve().family.into());
    ui.set_ui_size(26.0);
    ui.set_library_overview(
        "0 observed unique sounds · Inventory incomplete · Filters active".into(),
    );
    ui.show().unwrap();
    ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
    crate::test_platform::settle(ui.window());
    for label in [
        "0 observed unique sounds · Inventory incomplete · Filters active",
        "Applied library filters",
    ] {
        let (x, y, width, height) = exposed_bounds(ui.window(), label);
        assert!(
            x >= 0.0 && y >= 0.0 && x + width <= 800.0 && y + height <= 600.0,
            "count and disclosure must fit: {label}: {x} {y} {width} {height}"
        );
    }
}

#[test]
fn first_show_projects_nonempty_operation_labels_and_accessible_names() {
    let harness = crate::adapter_tests::Harness::with_catalog(
        Vec::new(),
        auditorylab_app::workspace::Capabilities::no_device_playback(),
    );
    let ui = &harness.windows.main;
    let actions = ui.global::<crate::generated::Actions>();
    assert_eq!(actions.get_open_recipe().label, "Open Recipe");
    assert_eq!(actions.get_new_sound().label, "New Sound");
    assert_eq!(actions.get_save().label, "Save sound");
    assert!(exposed_bounds(ui.window(), "Open Recipe…").2 > 0.0);
}
