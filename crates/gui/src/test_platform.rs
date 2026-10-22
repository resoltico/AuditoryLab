//! An offscreen Slint platform for one test thread. Slint's platform is
//! per thread, so each test installs its own and runs independently.
use slint::platform::{
    LayoutConstraints, Platform, PlatformError, Renderer, WindowAdapter, WindowProperties,
    software_renderer::SoftwareRenderer,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Layout constraints most recently reported for the audio-controls and
/// output-monitor windows.
pub(crate) type Constraints = Rc<RefCell<Option<LayoutConstraints>>>;

struct Offscreen {
    auxiliary_constraints: Constraints,
}

struct OffscreenWindow {
    window: slint::Window,
    renderer: SoftwareRenderer,
    size: Cell<slint::PhysicalSize>,
    auxiliary_constraints: Constraints,
}

impl WindowAdapter for OffscreenWindow {
    fn window(&self) -> &slint::Window {
        &self.window
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn size(&self) -> slint::PhysicalSize {
        self.size.get()
    }
    fn set_size(&self, size: slint::WindowSize) {
        let scale = self.window.scale_factor();
        self.size.set(size.to_physical(scale));
        self.window
            .dispatch_event(slint::platform::WindowEvent::Resized {
                size: size.to_logical(scale),
            });
    }
    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        if matches!(
            properties.title().as_str(),
            "Audio controls" | "Output monitor"
        ) {
            *self.auxiliary_constraints.borrow_mut() = Some(properties.layout_constraints());
        }
    }
}

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(Rc::new_cyclic(|weak| OffscreenWindow {
            window: slint::Window::new(weak.clone()),
            renderer: SoftwareRenderer::new(),
            size: Cell::default(),
            auxiliary_constraints: self.auxiliary_constraints.clone(),
        }))
    }
}

/// Install the offscreen platform on this thread.
pub(crate) fn install() -> Constraints {
    let constraints = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Offscreen {
        auxiliary_constraints: constraints.clone(),
    }))
    .expect("each test thread installs its platform once");
    constraints
}

/// Run timers and materialize conditional elements, as a native frame does.
pub(crate) fn settle(window: &slint::Window) {
    slint::platform::update_timers_and_animations();
    slint::private_unstable_api::re_exports::WindowInner::from_pub(window).draw_contents(|_, _| ());
    slint::platform::update_timers_and_animations();
}
