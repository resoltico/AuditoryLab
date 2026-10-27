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
    fail_auxiliary_show: Rc<Cell<bool>>,
    events: Option<std::sync::mpsc::Sender<Event>>,
}

pub(crate) type Event = Box<dyn FnOnce() + Send>;
struct EventProxy(std::sync::mpsc::Sender<Event>);
impl slint::platform::EventLoopProxy for EventProxy {
    fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
        Ok(())
    }
    fn invoke_from_event_loop(&self, event: Event) -> Result<(), slint::EventLoopError> {
        self.0
            .send(event)
            .map_err(|_| slint::EventLoopError::EventLoopTerminated)
    }
}
struct OffscreenWindow {
    window: slint::Window,
    renderer: SoftwareRenderer,
    size: Cell<slint::PhysicalSize>,
    title: RefCell<String>,
    auxiliary_constraints: Constraints,
    fail_auxiliary_show: Rc<Cell<bool>>,
}

impl WindowAdapter for OffscreenWindow {
    fn set_visible(&self, visible: bool) -> Result<(), PlatformError> {
        if visible
            && self.fail_auxiliary_show.get()
            && self.title.borrow().as_str() == "Output monitor"
        {
            return Err(PlatformError::Other(
                "injected Output monitor presentation failure".into(),
            ));
        }
        Ok(())
    }
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
        *self.title.borrow_mut() = properties.title().to_string();
        if matches!(
            properties.title().as_str(),
            "Audio controls" | "Output monitor"
        ) {
            *self.auxiliary_constraints.borrow_mut() = Some(properties.layout_constraints());
        }
    }
}

impl Platform for Offscreen {
    fn new_event_loop_proxy(&self) -> Option<Box<dyn slint::platform::EventLoopProxy>> {
        self.events
            .clone()
            .map(|sender| Box::new(EventProxy(sender)) as Box<dyn slint::platform::EventLoopProxy>)
    }
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(Rc::new_cyclic(|weak| OffscreenWindow {
            window: slint::Window::new(weak.clone()),
            renderer: SoftwareRenderer::new(),
            size: Cell::default(),
            title: RefCell::new(String::new()),
            auxiliary_constraints: self.auxiliary_constraints.clone(),
            fail_auxiliary_show: self.fail_auxiliary_show.clone(),
        }))
    }
}

/// Install the offscreen platform on this thread.
pub(crate) fn install() -> Constraints {
    install_with_auxiliary_failure(Rc::new(Cell::new(false)))
}

pub(crate) fn install_with_auxiliary_failure(fail_auxiliary_show: Rc<Cell<bool>>) -> Constraints {
    install_platform(fail_auxiliary_show, None)
}

/// Exactly one test installs Slint's process-global proxy; others keep thread-local platforms.
pub(crate) fn install_with_events() -> std::sync::mpsc::Receiver<Event> {
    let (sender, events) = std::sync::mpsc::channel();
    install_platform(Rc::new(Cell::new(false)), Some(sender));
    events
}
fn install_platform(
    fail_auxiliary_show: Rc<Cell<bool>>,
    events: Option<std::sync::mpsc::Sender<Event>>,
) -> Constraints {
    let constraints = Rc::new(RefCell::new(None));
    slint::platform::set_platform(Box::new(Offscreen {
        auxiliary_constraints: constraints.clone(),
        fail_auxiliary_show,
        events,
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
