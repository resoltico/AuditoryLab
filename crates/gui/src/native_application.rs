//! Cocoa application-menu actions and asynchronous termination policy.
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
    static TERMINATION_PENDING:Cell<bool> = const {Cell::new(false)};
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
#[allow(
    unsafe_code,
    reason = "registered Cocoa menu action has the matching void/object ABI"
)]
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
    WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
    // Refresh Cocoa's cached optional-delegate capabilities. The retained local
    // keeps the exact same delegate alive throughout both weak-property writes.
    application.setDelegate(None);
    application.setDelegate(Some(&delegate));
    Ok(())
}
/// Slint creates and rebuilds the native menu asynchronously. Retarget only its
/// application About item after creation; refreshes also cover menu replacement.
#[allow(
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
