//! Real widget coverage for font-aware multiline cursor scrolling.

use slint::ComponentHandle;
use slint::private_unstable_api::re_exports::{TextInput, WindowInner};
use std::ops::ControlFlow;

slint::slint! {
    import { TextEdit, VerticalBox } from "std-widgets.slint";
    export component CursorScrollProbe inherits Window {
        preferred-width:420px;preferred-height:180px;
        in property <length> text-size:13px;
        in property <string> family;
        in property <string> content;
        out property <length> scroll-y: edit.content-y;
        out property <length> viewport-height: edit.visible-height;
        public function position(offset:int) {edit.set-selection-offsets(offset,offset);}
        public function focus-edit() { edit.focus(); }
        VerticalBox {
            padding:0px;
            edit := TextEdit {
                text:root.content;font-size:root.text-size;font-family:root.family;read-only:true;
            }
        }
    }
}

/// Runs within the GUI's one initialized offscreen platform.
pub(super) fn assert_cursor_rect_scrolls_into_view() {
    let probe = CursorScrollProbe::new().unwrap();
    probe.set_family(crate::typography::resolve().family.into());
    let content = "Ag descenders gjpq\n".repeat(24) + "Last line gjpq Ω🙂";
    probe.set_content(content.clone().into());
    probe.show().unwrap();
    probe.invoke_focus_edit();
    let inner = WindowInner::from_pub(probe.window());
    let root = inner.window_item_rc().unwrap();
    let mut input = None;
    let _ = root.visit_descendants(|item| {
        if item.downcast::<TextInput>().is_some() {
            assert!(input.is_none(), "the real TextEdit has exactly one input");
            input = Some(item.clone());
        }
        ControlFlow::<()>::Continue(())
    });
    let item = input.unwrap();
    for size in [32.666_668_f32, 13.0, 26.0, 40.0] {
        probe.set_text_size(size);
        for height in [180.0_f32, 28.0] {
            probe
                .window()
                .set_size(slint::LogicalSize::new(420.0, height));
            slint::platform::update_timers_and_animations();
            inner.draw_contents(|_, _| ());
            assert!(
                (probe
                    .window()
                    .size()
                    .to_logical(probe.window().scale_factor())
                    .height
                    - height)
                    .abs()
                    < 0.1,
                "test must exercise the requested viewport height"
            );
            for offset in [content.len(), 0, content.len() / 2, content.len()] {
                let offset = i32::try_from(offset).unwrap();
                probe.invoke_position(offset);
                slint::platform::update_timers_and_animations();
                inner.draw_contents(|_, _| ());
                let input = item.downcast::<TextInput>().unwrap();
                let input = input.as_pin_ref();
                let key: slint::SharedString = slint::platform::Key::LeftArrow.into();
                probe
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.clone() });
                probe
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key });
                let actual_offset = usize::try_from(input.cursor_position_byte_offset()).unwrap();
                let rect = inner
                    .window_adapter()
                    .renderer()
                    .text_input_cursor_rect_for_byte_offset(
                        input,
                        &item,
                        actual_offset,
                        input.cursor_position_affinity(),
                    );
                let top = rect.origin.y + probe.get_scroll_y();
                let bottom = top + rect.size.height;
                let viewport = probe.get_viewport_height();
                assert!(viewport > 0.0, "real widget viewport must be laid out");
                assert!(
                    item.geometry().size.width > 0.0 && item.geometry().size.height > 0.0,
                    "real text input must have positive geometry"
                );
                eprintln!(
                    "TextEdit {size}px window{height}px offset{offset}: cursor={rect:?} \
                        scroll={} viewport={viewport}",
                    probe.get_scroll_y()
                );
                assert!(
                    (input.cursor_height().get() - rect.size.height).abs() < 0.01,
                    "published height is the actual shaped cursor extent"
                );
                assert!(
                    bottom <= viewport + 0.05,
                    "complete cursor line must fit the viewport bottom: {bottom}>{viewport}"
                );
                if viewport >= rect.size.height {
                    assert!(
                        top >= -0.05,
                        "complete cursor line must fit the viewport top"
                    );
                } else {
                    assert!(
                        (bottom - viewport).abs() < 0.05,
                        "a viewport shorter than one line aligns its bottom without bouncing"
                    );
                }
            }
        }
    }
    probe.hide().unwrap();
}
