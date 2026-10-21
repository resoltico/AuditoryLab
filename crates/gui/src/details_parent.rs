//! Modeless native parent binding for the Details window.

#[cfg(target_os = "linux")]
pub(crate) fn bind<C: slint::ComponentHandle + 'static>(child: &crate::DetailsWindow, parent: &C) {
    use slint::{
        ComponentHandle,
        winit_030::{EventResult, WinitWindowAccessor},
    };

    let parent = parent.as_weak();
    let mut pending = !apply(child.window(), &parent);
    // Details exclusively owns this filter. A new invocation replaces its
    // binding, including a deferred binding to a monitor that is about to hide.
    // Slint recreates Wayland windows after hide; apply to the new native
    // window on its first event, before the backend draws its first buffer.
    child.window().on_winit_window_event(move |child, _event| {
        if pending {
            pending = !apply(child, &parent);
        }
        EventResult::Propagate
    });
}

#[cfg(target_os = "linux")]
// True means applied or no longer relevant; only missing native handles defer.
fn apply<C: slint::ComponentHandle + 'static>(
    child: &slint::Window,
    parent: &slint::Weak<C>,
) -> bool {
    use slint::winit_030::{WinitWindowAccessor, winit::platform::wayland::WindowExtWayland};

    let Some(parent) = parent.upgrade() else {
        return true;
    };
    if !child.is_visible() || !parent.window().is_visible() {
        return true;
    }
    // Borrow native handles only for the request. Retaining a native window
    // across Slint hide would keep its Wayland surface alive.
    child
        .with_winit_window(|child| {
            if child.xdg_toplevel().is_none() {
                return true;
            }
            parent
                .window()
                .with_winit_window(|parent| {
                    if let Err(error) = child.set_transient_for(Some(parent)) {
                        eprintln!("Cannot parent Details window: {error}");
                    }
                    true
                })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn bind<C: slint::ComponentHandle + 'static>(
    _child: &crate::DetailsWindow,
    _parent: &C,
) {
}
