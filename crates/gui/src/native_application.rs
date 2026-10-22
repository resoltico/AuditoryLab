//! Cocoa application-menu actions and asynchronous termination policy.
//! Keeps winit's delegate, menu routing and context-menu event handler intact.
use objc2::{
    Encode, MainThreadMarker, MainThreadOnly,
    rc::Retained,
    runtime::{AnyObject, Imp, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use objc2_app_kit::{NSButton, NSModalResponseOK, NSOpenPanel, NSSavePanel, NSView};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    ffi::CString,
};
thread_local! {
    static WINDOW:RefCell<Option<slint::Weak<super::MainWindow>>> = const {RefCell::new(None)};
    static AUDIO_CANCEL:RefCell<Option<auditorylab::Cancel>> = const {RefCell::new(None)};
    static TERMINATION_PENDING:Cell<bool> = const {Cell::new(false)};
}
#[expect(
    unsafe_code,
    reason = "Objective-C callback ABI is registered with the matching protocol signature"
)]
unsafe extern "C-unwind" fn should_terminate(
    _this: &AnyObject,
    _selector: Sel,
    _sender: &NSApplication,
) -> NSApplicationTerminateReply {
    if MainThreadMarker::new().is_none() {
        return NSApplicationTerminateReply::TerminateCancel;
    }
    if TERMINATION_PENDING.with(|pending| pending.replace(true)) {
        return NSApplicationTerminateReply::TerminateLater;
    }
    if slint::invoke_from_event_loop(|| {
        let window = WINDOW.with(|slot| slot.borrow().as_ref().and_then(slint::Weak::upgrade));
        if let Some(window) = window {
            window.invoke_quit();
        } else {
            reply_to_termination(false);
        }
    })
    .is_err()
    {
        TERMINATION_PENDING.with(|pending| pending.set(false));
        return NSApplicationTerminateReply::TerminateCancel;
    }
    NSApplicationTerminateReply::TerminateLater
}
#[expect(unsafe_code, reason = "registered Cocoa menu action has the matching void/object ABI")]
unsafe extern "C-unwind" fn show_about(_: &AnyObject, _: Sel, _: &AnyObject) {
    let _ = slint::invoke_from_event_loop(|| {
        if let Some(window) =
            WINDOW.with(|slot| slot.borrow().as_ref().and_then(slint::Weak::upgrade))
        {
            window.invoke_about();
        }
    });
}
/// Extend only the known process-owned winit delegate, without replacing its object or methods.
#[expect(unsafe_code, reason = "registers one optional Cocoa delegate method; no object layout or existing method is changed")]
pub(super) fn install(
    window: slint::Weak<super::MainWindow>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mtm =
        MainThreadMarker::new().ok_or("native Quit must be initialized on the main thread")?;
    let application = NSApplication::sharedApplication(mtm);
    let delegate = application
        .delegate()
        .ok_or("native Quit needs an initialized window delegate")?;
    let object: &AnyObject = AsRef::<AnyObject>::as_ref(&*delegate);
    let class = object.class();
    let selector = sel!(applicationShouldTerminate:);
    if class.name().to_bytes() != b"WinitApplicationDelegate"
        || class.instance_method(selector).is_some()
    {
        return Err("native Quit integration needs review: unexpected delegate or an existing termination handler".into());
    }
    let encoding = CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))?;
    // SAFETY: Objective-C's IMP erases a function signature. The callback uses the
    // protocol's NSUInteger return and (object, selector, application object)
    // arguments. Its encoding matches exactly. Cocoa invokes it on the main
    // thread; the function checks that before using UI-thread-local state.
    let implementation = unsafe {
        std::mem::transmute::<
            unsafe extern "C-unwind" fn(
                &AnyObject,
                Sel,
                &NSApplication,
            ) -> NSApplicationTerminateReply,
            Imp,
        >(should_terminate)
    };
    // SAFETY: this is the existing, process-owned winit delegate class. Adding a
    // previously absent method changes no ivars, ownership or existing methods.
    // The runtime copies the encoding, as in objc2's ClassBuilder::add_method.
    if !unsafe {
        objc2::ffi::class_addMethod(
            (class as *const objc2::runtime::AnyClass).cast_mut(),
            selector,
            implementation,
            encoding.as_ptr(),
        )
    }
    .as_bool()
    {
        return Err("could not install native Quit handling".into());
    }
    let about_selector = sel!(auditorylabShowAbout:);
    if class.instance_method(about_selector).is_some() {
        return Err("native About action already exists; integration needs review".into());
    }
    // SAFETY: same process-owned delegate, absent method, exact v@:@ action ABI.
    let about_imp = unsafe {
        std::mem::transmute::<unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject), Imp>(
            show_about,
        )
    };
    if !unsafe {
        objc2::ffi::class_addMethod(
            (class as *const objc2::runtime::AnyClass).cast_mut(),
            about_selector,
            about_imp,
            c"v@:@".as_ptr(),
        )
    }
    .as_bool()
    {
        return Err("cannot install native About action".into());
    }
    let stop_selector = sel!(auditorylabStopAudio:);
    if class.instance_method(stop_selector).is_some() {
        return Err("native Stop action already exists".into());
    }
    // SAFETY: same absent process-owned delegate method, exact void/object action ABI.
    let stop_imp = unsafe {
        std::mem::transmute::<unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject), Imp>(
            stop_audio,
        )
    };
    if !unsafe {
        objc2::ffi::class_addMethod(
            (class as *const objc2::runtime::AnyClass).cast_mut(),
            stop_selector,
            stop_imp,
            c"v@:@".as_ptr(),
        )
    }
    .as_bool()
    {
        return Err("cannot install native Stop action".into());
    }
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
    // Refresh Cocoa's cached optional-delegate capabilities. The retained local
    // keeps the exact same delegate alive throughout both weak-property writes.
    application.setDelegate(None);
    application.setDelegate(Some(&delegate));
    Ok(())
}
/// Slint creates and rebuilds the native menu asynchronously. Retarget only its
/// application About item after creation; refreshes also cover menu replacement.
#[expect(
    unsafe_code,
    reason = "retargets the process-owned native About item to its retained delegate action"
)]
pub(super) fn refresh_about_menu() -> Result<(), Box<dyn std::error::Error>> {
    let mtm = MainThreadMarker::new().ok_or("About needs the main thread")?;
    let application = NSApplication::sharedApplication(mtm);
    let delegate = application
        .delegate()
        .ok_or("missing application delegate")?;
    let object: &AnyObject = AsRef::<AnyObject>::as_ref(&*delegate);
    let Some(menu) = application.mainMenu() else {
        return Ok(());
    };
    // Menus may be empty during AppKit launch/recovery or a Slint rebuild.
    // itemAtIndex throws for an empty menu despite its nullable return type.
    let Some(app_menu) = menu
        .itemArray()
        .firstObject()
        .and_then(|item| item.submenu())
    else {
        return Ok(());
    };
    let Some(about) = app_menu.itemArray().firstObject() else {
        return Ok(());
    };
    if !about.title().to_string().starts_with("About ")
        || !matches!(about.action(), Some(action) if action == sel!(fireMenuItemAction:) || action == sel!(auditorylabShowAbout:))
    {
        return Err("native About menu structure changed; integration needs review".into());
    }
    // SAFETY: the weak target remains the retained application delegate; action above has its declared ABI.
    unsafe {
        about.setTarget(Some(object));
        about.setAction(Some(sel!(auditorylabShowAbout:)));
        about.setEnabled(true);
    }
    Ok(())
}
/// Complete Cocoa's outstanding request only after the shared policy resolves.
pub(super) fn reply_to_termination(approve: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    if !TERMINATION_PENDING.with(|pending| pending.replace(false)) {
        return false;
    }
    NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(approve);
    true
}

/// The native button requests only the independent audio cancellation flag.
pub(super) fn audio_cancel(cancel: Option<auditorylab::Cancel>) {
    AUDIO_CANCEL.with(|slot| *slot.borrow_mut() = cancel);
}
#[expect(unsafe_code, reason = "registered native action ABI; sender is checked before using NSButton methods")]
unsafe extern "C-unwind" fn stop_audio(_: &AnyObject, _: Sel, sender: &AnyObject) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    let requested = AUDIO_CANCEL.with(|slot| {
        slot.borrow().as_ref().is_some_and(|cancel| {
            cancel.request();
            true
        })
    });
    if let Some(button) = sender.downcast_ref::<NSButton>() {
        button.setTitle(&NSString::from_str(if requested {
            "Stop requested · file dialog stays open"
        } else {
            "Audio already stopped · file dialog stays open"
        }));
        button.setEnabled(false);
    }
}
/// A single native file panel. Its audio-only accessory remains usable within modal scope.
#[expect(
    unsafe_code,
    deprecated,
    reason = "main-thread AppKit sheet/completion ABI; macOS 15-compatible extension filter and checked owned panel lifetime"
)]
pub(super) async fn choose_file(
    ui: &super::MainWindow,
    filename: Option<&str>,
    extension: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    use std::{rc::Rc, task::Poll};
    let mtm = MainThreadMarker::new().ok_or("File panels require the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let owner = ui.window().window_handle();
    let raw = owner
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw();
    let RawWindowHandle::AppKit(handle) = raw else {
        return Err("File panel requires an AppKit document handle".into());
    };
    // SAFETY: the Slint-owned AppKit handle is live for this entire main-thread future.
    let parent = unsafe { handle.ns_view.cast::<NSView>().as_ref() }
        .window()
        .ok_or("The document has no native window")?;
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
    if AUDIO_CANCEL.with(|slot| slot.borrow().is_some()) {
        let delegate = app
            .delegate()
            .ok_or("No native application delegate is available")?;
        let target = AsRef::<AnyObject>::as_ref(&*delegate);
        let view = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(360.0, 44.0)),
        );
        // SAFETY: selector is installed on this retained delegate with the v@:@ native action ABI.
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Stop audio · file dialog stays open"),
                Some(target),
                Some(sel!(auditorylabStopAudio:)),
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
                .ok_or_else(|| "The native panel returned no local file path".to_owned())
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
    std::future::poll_fn(|context| {
        if let Some(answer) = result.borrow_mut().take() {
            Poll::Ready(answer)
        } else {
            *wake.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        }
    })
    .await
}
/// Clear the audio-only command after both workers have drained.
pub(super) fn shutdown() {
    audio_cancel(None);
}
