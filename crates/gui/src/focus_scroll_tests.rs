//! Actual widget coverage for revealing keyboard focus without changing hit testing.

use slint::ComponentHandle;
use slint::private_unstable_api::re_exports::{AccessibleStringProperty, ItemRc, WindowInner};
use std::ops::ControlFlow;

slint::slint! {
    #[edition = "1.19"]
    import { Button } from "../ui/button.slint";
    import { CheckBox, Switch, ComboBox, LineEdit, Slider } from "../ui/focus-controls.slint";
    import { ScrollView } from "std-widgets.slint";
    export component FocusScrollProbe inherits Window {
        in property <bool> hide-button;
        out property <length> scroll-y:outer.content-y;
        out property <int> popup-selection:choices.current-index;
        public function reset-scroll() {outer.content-x=0px;outer.content-y=0px;}
        public function scroll-to-end() {outer.content-y=-700px;}
        public function focus-target(index:int) {
            if index==0 {bottom.focus();}
            if index==1 {top.focus();}
            if index==2 {check.focus();}
            if index==3 {toggle.focus();}
            if index==4 {choices.focus();}
            if index==5 {editable.focus();}
            if index==6 {level.focus();}
            if index==7 {clipped.focus();}
            if index==8 {read-only-input.focus();}
        }
        FocusScope {width:parent.width;height:parent.height;
        capture-key-pressed(event)=>{if event.text=="k" {return accept;}if event.text=="j" {
            bottom.focus();return reject;}return reject;}
        outer := ScrollView {content-width:1000px;content-height:900px;
            Rectangle {width:1000px;height:900px;accessible-role:groupbox;
                bottom := Button {x:800px;y:700px;width:150px;height:40px;text:"Bottom button";
                    visible:!root.hide-button;}
                top := Button {x:40px;y:30px;width:150px;height:40px;text:"Top button";}
                choices := ComboBox {x:40px;y:80px;width:180px;height:40px;model:["First","Second"];
                    accessible-label:"Choices";}
                check := CheckBox {x:800px;y:100px;width:180px;height:40px;text:"Right checkbox";}
                toggle := Switch {x:40px;y:700px;width:180px;height:40px;text:"Bottom switch";}
                editable := LineEdit {x:800px;y:250px;width:180px;height:40px;
                    accessible-label:"Editable value";text:"Value";}
                level := Slider {x:800px;y:350px;width:180px;height:40px;
                    accessible-label:"Level slider";}
                Rectangle {x:300px;y:200px;width:100px;height:100px;clip:true;
                    clipped := FocusScope {x:120px;y:20px;width:40px;height:40px;}
                }
                read-only-input := TextInput {x:40px;y:150px;width:150px;height:40px;read-only:true;
                    text:"Read-only selection";}

            }
        }
        }
    }
}

slint::slint! {
    #[edition = "1.19"]
    export component ConditionalLifetimeProbe inherits Window {
        in property <[int]> entries;
        for entry in root.entries: Rectangle {width:200px;height:100px;
            if entry>0: TextInput {width:150px;height:30px;text:"Nested editor";}
        }
    }
}

fn assert_orphaned_conditional_loses_focus() {
    use slint::private_unstable_api::re_exports::{FocusReason, ItemTreeRc, ItemWeak, TextInput};

    let probe = ConditionalLifetimeProbe::new().unwrap();
    let rows = std::rc::Rc::new(slint::VecModel::from(vec![1]));
    probe.set_entries(slint::ModelRc::from(rows.clone()));
    probe.show().unwrap();
    probe
        .window()
        .set_size(slint::LogicalSize::new(300.0, 200.0));
    let inner = WindowInner::from_pub(probe.window());
    slint::platform::update_timers_and_animations();
    inner.draw_contents(|_, _| ());
    let root = inner.window_item_rc().unwrap();
    let input = root
        .visit_descendants(|item| {
            if item.downcast::<TextInput>().is_some() {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
    let parent = {
        let mut parent = ItemWeak::default();
        ItemTreeRc::borrow_pin(input.item_tree())
            .as_ref()
            .parent_node(&mut parent);
        assert!(
            parent.upgrade().is_some(),
            "nested conditional must begin with a live parent row"
        );
        parent
    };
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    rows.set_vec(Vec::new());
    slint::platform::update_timers_and_animations();
    inner.draw_contents(|_, _| ());
    assert!(
        root.visit_descendants(|item| {
            if *item == input {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_none(),
        "removed conditional must leave the live tree"
    );
    assert!(
        parent.upgrade().is_none(),
        "fixture must release the entire parent row, retaining only its nested input"
    );
    assert_eq!(inner.focus_item.borrow().upgrade().as_ref(), Some(&input));
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    assert!(
        inner.focus_item.borrow().upgrade().is_none(),
        "orphaned conditional cannot retain focus after its parent drops"
    );
    probe.hide().unwrap();
}

pub(super) fn assert_control_focus_scrolls_into_view() {
    let probe = FocusScrollProbe::new().unwrap();
    probe.show().unwrap();
    probe
        .window()
        .set_size(slint::LogicalSize::new(300.0, 200.0));
    settle(probe.window());
    let inner = WindowInner::from_pub(probe.window());
    let root = inner.window_item_rc().unwrap();
    for (index, label) in [
        (0, "Bottom button"),
        (1, "Top button"),
        (2, "Right checkbox"),
        (3, "Bottom switch"),
        (4, "Choices"),
        (5, "Editable value"),
        (6, "Level slider"),
    ] {
        probe.invoke_focus_target(index);
        settle(probe.window());
        let item = root
            .visit_descendants(|item| {
                if item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .is_some_and(|text| text == label)
                {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        let (x, y, width, height) = bounds(&item);
        assert!(
            x >= -0.1 && y >= -0.1 && x + width <= 300.1 && y + height <= 200.1,
            "whole {label} must be revealed: {:?}",
            (x, y, width, height)
        );
    }
    probe.invoke_focus_target(7);
    settle(probe.window());
    let clipped_focus = inner.focus_item.borrow().upgrade().unwrap();
    assert!(
        !clipped_focus.is_visible(),
        "non-Flickable clip control must really be clipped"
    );
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    settle(probe.window());
    assert!(
        inner
            .focus_item
            .borrow()
            .upgrade()
            .is_none_or(|item| item != clipped_focus),
        "a non-Flickable clipping boundary must still clear focus"
    );
    assert_readonly_clipped_shortcuts(&probe);
    assert_hidden_focus_released(&probe);
    probe.invoke_reset_scroll();
    probe.invoke_focus_target(4);
    settle(probe.window());
    for key in [
        slint::platform::Key::Return,
        slint::platform::Key::DownArrow,
        slint::platform::Key::Return,
    ] {
        let text: slint::SharedString = key.into();
        probe
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        probe
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
        settle(probe.window());
    }
    assert_eq!(
        probe.get_popup_selection(),
        1,
        "actual widget popup must keep focus and select its second entry"
    );
    probe.hide().unwrap();
    assert_orphaned_conditional_loses_focus();
}

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

fn assert_readonly_clipped_shortcuts(probe: &FocusScrollProbe) {
    let inner = WindowInner::from_pub(probe.window());
    let root = inner.window_item_rc().unwrap();
    probe.invoke_reset_scroll();
    probe.invoke_focus_target(8);
    settle(probe.window());
    let read_only_focus = inner.focus_item.borrow().upgrade().unwrap();
    probe.invoke_scroll_to_end();
    settle(probe.window());
    assert!(
        !read_only_focus.is_visible(),
        "read-only field must really be scroll-clipped"
    );
    let scroll = probe.get_scroll_y();
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 10.0),
        });
    settle(probe.window());
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&read_only_focus),
        "scroll clipping must preserve read-only focus too"
    );
    assert_eq!(
        probe.get_scroll_y(),
        scroll,
        "pointer movement must not scroll read-only content"
    );
    for pressed in [true, false] {
        probe.window().dispatch_event(if pressed {
            slint::platform::WindowEvent::KeyPressed { text: "k".into() }
        } else {
            slint::platform::WindowEvent::KeyReleased { text: "k".into() }
        });
    }
    settle(probe.window());
    assert_eq!(
        probe.get_scroll_y(),
        scroll,
        "captured shortcut must not reveal or scroll its focused descendant"
    );
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Control.into(),
        });
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Control.into(),
        });
    settle(probe.window());
    assert_eq!(
        probe.get_scroll_y(),
        scroll,
        "a modifier alone must not reveal or scroll an offscreen field"
    );
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "j".into() });
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: "j".into() });
    settle(probe.window());
    assert!(
        inner.focus_item.borrow().upgrade().as_ref() != Some(&read_only_focus),
        "capture handler must actually change focus"
    );
    let bottom = root
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some("Bottom button")
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
    let (x, y, width, height) = bounds(&bottom);
    assert!(
        x >= -0.1 && y >= -0.1 && x + width <= 300.1 && y + height <= 200.1,
        "key reveal must not scroll the old target after capture changes focus: {:?}",
        (x, y, width, height)
    );
}

fn assert_hidden_focus_released(probe: &FocusScrollProbe) {
    let inner = WindowInner::from_pub(probe.window());
    // A genuinely hidden control still loses focus through ordinary key handling.
    probe.invoke_focus_target(0);
    settle(probe.window());
    let hidden_focus = inner.focus_item.borrow().upgrade().unwrap();
    probe.set_hide_button(true);
    settle(probe.window());
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    probe
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Tab.into(),
        });
    settle(probe.window());
    assert!(
        inner
            .focus_item
            .borrow()
            .upgrade()
            .is_none_or(|item| item != hidden_focus),
        "genuinely hidden focus must not be retained"
    );
    eprintln!(
        "Control focus: real Button/CheckBox/Switch, positive extent, \
            ownerless point, oversized owner, nested Flickable and hidden control \
            passed"
    );
}
