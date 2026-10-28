//! macOS application integration: Quit and About arrive through the Slint
//! backend's typed application handler, and a file panel offers Stop for the
//! audio session that was playing when it opened.
use crate::adapter::SlintAdapter;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::NSObject, sel,
};
use objc2_app_kit::{NSButton, NSModalResponseOK, NSOpenPanel, NSSavePanel, NSView};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};
use slint::ComponentHandle;
use slint::winit_030::winit::{
    event_loop::ActiveEventLoop,
    platform::macos::{TerminationRequest, reply_to_termination as reply},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// The application handler installed with the backend. The adapter is
/// created after the backend, so the handler reaches it through this slot.
pub(crate) struct ApplicationHandler {
    adapter: Rc<RefCell<Weak<SlintAdapter>>>,
}

impl ApplicationHandler {
    /// A handler and the slot that later receives the adapter.
    pub(crate) fn new() -> (Self, Rc<RefCell<Weak<SlintAdapter>>>) {
        let slot = Rc::new(RefCell::new(Weak::new()));
        (
            Self {
                adapter: Rc::clone(&slot),
            },
            slot,
        )
    }
}

impl slint::winit_030::CustomApplicationHandler for ApplicationHandler {
    fn intercepts_termination(&self) -> bool {
        true
    }

    fn termination_requested(&mut self, _: &ActiveEventLoop, request: TerminationRequest) {
        match self.adapter.borrow().upgrade() {
            Some(adapter) => adapter.request_termination(request),
            None => reply_to_termination(request, false),
        }
    }

    fn application_about_title(&self) -> Option<String> {
        Some(auditorylab_presentation::build_info::about_title())
    }

    fn about_requested(&mut self, _: &ActiveEventLoop) {
        if let Some(adapter) = self.adapter.borrow().upgrade() {
            adapter.show_about();
        }
    }
}

/// Answer `AppKit`'s pending termination request.
pub(crate) fn reply_to_termination(request: TerminationRequest, approve: bool) {
    if let Err(error) = reply(request, approve) {
        eprintln!(
            "{}",
            crate::native_text::InterfaceOperation::Quit.failure(error)
        );
    }
}

/// Instance state of the Stop target: the one session it may stop.
struct StopTargetState {
    control: auditorylab_app::services::playback::PlaybackControl,
}

define_class!(
    /// The Stop button's target in a file panel, bound to one audio session.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AuditoryLabPanelStopTarget"]
    #[ivars = StopTargetState]
    struct StopTarget;

    impl StopTarget {
        #[unsafe(method(stopAudio:))]
        fn stop_audio(&self, sender: &NSButton) {
            let requested = matches!(
                self.ivars().control.request_stop(),
                auditorylab_app::services::playback::StopRequest::Accepted |
                    auditorylab_app::services::playback::StopRequest::AlreadyRequested
            );
            sender.setTitle(&NSString::from_str(if requested {
                crate::terms::Status::StopInFilePanelRequested.label()
            } else {
                crate::terms::Status::FilePanelAudioStopped.label()
            }));
            sender.setEnabled(false);
        }
    }
);

impl StopTarget {
    #[expect(
        unsafe_code,
        reason = "msg_send to NSObject's init has no compile-time checked form"
    )]
    fn new(
        mtm: MainThreadMarker,
        control: auditorylab_app::services::playback::PlaybackControl,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(StopTargetState { control });
        // SAFETY: NSObject's designated initializer on a freshly allocated instance.
        unsafe { msg_send![super(this), init] }
    }
}

/// A single native file panel, as a sheet on the main window. With a
/// session's control it carries a Stop button for that session; the panel,
/// its file operation and its sheet are unaffected by Stop.
#[expect(
    unsafe_code,
    deprecated,
    reason = "main-thread AppKit sheet/completion ABI; macOS 15-compatible \
        extension filter and checked owned panel lifetime"
)]
pub(crate) async fn choose_file(
    ui: &crate::MainWindow,
    filename: Option<&str>,
    extension: &str,
    control: Option<auditorylab_app::services::playback::PlaybackControl>,
) -> Result<Option<std::path::PathBuf>, String> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::task::Poll;

    let mtm =
        MainThreadMarker::new().ok_or(crate::terms::Caption::NativeFilePanelUnavailable.label())?;
    let owner = ui.window().window_handle();
    let raw = owner
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw();
    let RawWindowHandle::AppKit(handle) = raw else {
        return Err(crate::terms::Caption::NativeFilePanelUnavailable
            .label()
            .into());
    };
    // SAFETY: the Slint-owned AppKit handle is live for this entire main-thread future.
    let parent = unsafe { handle.ns_view.cast::<NSView>().as_ref() }
        .window()
        .ok_or(crate::terms::Caption::NativeWindowUnavailable.label())?;
    let panel = if let Some(name) = filename {
        let panel = NSSavePanel::savePanel(mtm);
        panel.setNameFieldStringValue(&NSString::from_str(name));
        panel
    } else {
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseDirectories(false);
        panel.setCanChooseFiles(true);
        panel.setAllowsMultipleSelection(false);
        Retained::into_super(panel)
    };
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str(
        extension,
    )])));
    panel.setCanCreateDirectories(true);
    // The button's target is weak; this binding keeps it alive until the
    // panel completes.
    let stop_target = control.map(|control| StopTarget::new(mtm, control));
    if let Some(target) = &stop_target {
        let view = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(360.0, 44.0)),
        );
        // SAFETY: `stopAudio:` is declared on the target's class with the
        // void/object action signature.
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(crate::terms::Action::StopInFilePanel.label()),
                Some(target),
                Some(sel!(stopAudio:)),
                mtm,
            )
        };
        button.setFrame(NSRect::new(
            NSPoint::new(8.0, 6.0),
            NSSize::new(344.0, 32.0),
        ));
        view.addSubview(&button);
        panel.setAccessoryView(Some(&view));
    }
    let result = Rc::new(RefCell::new(None));
    let wake = Rc::new(RefCell::new(None::<std::task::Waker>));
    let (answer, waiter, selected) = (result.clone(), wake.clone(), panel.clone());
    let completion = block2::RcBlock::new(move |response| {
        let path = if response == NSModalResponseOK {
            selected
                .URL()
                .and_then(|url| url.path())
                .map(|path| Some(std::path::PathBuf::from(path.to_string())))
                .ok_or_else(|| {
                    crate::terms::Caption::NativeFilePathUnavailable
                        .label()
                        .to_owned()
                })
        } else {
            Ok(None)
        };
        *answer.borrow_mut() = Some(path);
        if let Some(waker) = waiter.borrow_mut().take() {
            waker.wake();
        }
    });
    // Panel, parent and block stay owned until main-thread completion.
    panel.beginSheetModalForWindow_completionHandler(&parent, &completion);
    let path = std::future::poll_fn(|context| {
        if let Some(answer) = result.borrow_mut().take() {
            Poll::Ready(answer)
        } else {
            *wake.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        }
    })
    .await;
    drop(stop_target);
    path
}
