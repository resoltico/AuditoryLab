//! Bridge Cocoa's asynchronous termination request into the ordinary close policy.
//! Keeps winit's delegate, menu routing and context-menu event handler intact.
use objc2::{
    Encode, MainThreadMarker,
    runtime::{AnyObject, Imp, Sel},
    sel,
};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use std::{
    cell::{Cell, RefCell},
    ffi::CString,
};
thread_local! {
    static WINDOW:RefCell<Option<slint::Weak<super::MainWindow>>> = const {RefCell::new(None)};
    static PENDING:Cell<bool> = const {Cell::new(false)};
}
#[allow(
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
    if PENDING.with(|pending| pending.replace(true)) {
        return NSApplicationTerminateReply::TerminateLater;
    }
    if slint::invoke_from_event_loop(|| {
        let window = WINDOW.with(|slot| slot.borrow().as_ref().and_then(slint::Weak::upgrade));
        if let Some(window) = window {
            window.invoke_quit();
        } else {
            reply(false);
        }
    })
    .is_err()
    {
        PENDING.with(|pending| pending.set(false));
        return NSApplicationTerminateReply::TerminateCancel;
    }
    NSApplicationTerminateReply::TerminateLater
}
/// Extend only the known process-owned winit delegate, without replacing its object or methods.
#[allow(
    unsafe_code,
    reason = "registers one optional Cocoa delegate method; no object layout or existing method is changed"
)]
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
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
    // Refresh Cocoa's cached optional-delegate capabilities. The retained local
    // keeps the exact same delegate alive throughout both weak-property writes.
    application.setDelegate(None);
    application.setDelegate(Some(&delegate));
    Ok(())
}
/// Complete Cocoa's outstanding request only after the shared policy resolves.
pub(super) fn reply(approve: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    if !PENDING.with(|pending| pending.replace(false)) {
        return false;
    }
    NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(approve);
    true
}
