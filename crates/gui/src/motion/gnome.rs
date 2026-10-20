//! GSettings delivery on an owned GLib context; no GTK windows or settings writes.
#![allow(
    unsafe_code,
    reason = "checked GIO/GLib public C ABI; owned context, signal and settings are confined to one worker"
)]
use super::*;
use libloading::Library;
use std::ffi::{c_char, c_void};
static GIO: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static OBJECT: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static GLIB: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
fn library(
    slot: &'static std::sync::OnceLock<Option<Library>>,
    name: &str,
) -> Option<&'static Library> {
    // Native registered types can outlive a settings instance.
    slot.get_or_init(|| unsafe { Library::new(name) }.ok())
        .as_ref()
}
unsafe extern "C" fn changed(_: *mut c_void, _: *mut c_void, _: *mut c_void) {}
struct Context {
    pointer: *mut c_void,
    pop: unsafe extern "C" fn(*mut c_void),
    unref: unsafe extern "C" fn(*mut c_void),
}
impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: this worker owns and pushed the context; all its objects are released first.
        unsafe {
            (self.pop)(self.pointer);
            (self.unref)(self.pointer);
        }
    }
}
pub(super) fn listen(pending: &Mutex<Option<Preference>>, stop: &AtomicBool) -> bool {
    // SAFETY: exact checked signatures; no object leaves this thread, and every
    // successful allocation/connection is released before its context is popped.
    unsafe {
        let Some(gio) = library(&GIO, "libgio-2.0.so.0") else {
            return false;
        };
        let Some(object) = library(&OBJECT, "libgobject-2.0.so.0") else {
            return false;
        };
        let Some(glib) = library(&GLIB, "libglib-2.0.so.0") else {
            return false;
        };
        macro_rules! symbol {
            ($lib:expr, $name:literal, $ty:ty) => {
                match $lib.get::<$ty>(concat!($name, "\0").as_bytes()) {
                    Ok(symbol) => *symbol,
                    Err(_) => return false,
                }
            };
        }
        let new_context = symbol!(
            glib,
            "g_main_context_new",
            unsafe extern "C" fn() -> *mut c_void
        );
        let push = symbol!(
            glib,
            "g_main_context_push_thread_default",
            unsafe extern "C" fn(*mut c_void)
        );
        let pop = symbol!(
            glib,
            "g_main_context_pop_thread_default",
            unsafe extern "C" fn(*mut c_void)
        );
        let context_unref = symbol!(
            glib,
            "g_main_context_unref",
            unsafe extern "C" fn(*mut c_void)
        );
        let iteration = symbol!(
            glib,
            "g_main_context_iteration",
            unsafe extern "C" fn(*mut c_void, i32) -> i32
        );
        let source = symbol!(
            gio,
            "g_settings_schema_source_get_default",
            unsafe extern "C" fn() -> *mut c_void
        );
        let lookup = symbol!(
            gio,
            "g_settings_schema_source_lookup",
            unsafe extern "C" fn(*mut c_void, *const c_char, i32) -> *mut c_void
        );
        let has = symbol!(
            gio,
            "g_settings_schema_has_key",
            unsafe extern "C" fn(*mut c_void, *const c_char) -> i32
        );
        let schema_unref = symbol!(
            gio,
            "g_settings_schema_unref",
            unsafe extern "C" fn(*mut c_void)
        );
        let create = symbol!(
            gio,
            "g_settings_new_full",
            unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void
        );
        let get = symbol!(
            gio,
            "g_settings_get_boolean",
            unsafe extern "C" fn(*mut c_void, *const c_char) -> i32
        );
        let unref = symbol!(object, "g_object_unref", unsafe extern "C" fn(*mut c_void));
        let connect = symbol!(
            object,
            "g_signal_connect_data",
            unsafe extern "C" fn(
                *mut c_void,
                *const c_char,
                unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
                *mut c_void,
                *const c_void,
                u32,
            ) -> u64
        );
        let disconnect = symbol!(
            object,
            "g_signal_handler_disconnect",
            unsafe extern "C" fn(*mut c_void, u64)
        );
        let context = new_context();
        if context.is_null() {
            return false;
        }
        push(context);
        let context = Context {
            pointer: context,
            pop,
            unref: context_unref,
        };
        let source = source();
        if source.is_null() {
            return false;
        }
        let schema = lookup(source, c"org.gnome.desktop.interface".as_ptr(), 1);
        if schema.is_null() {
            return false;
        }
        if has(schema, c"enable-animations".as_ptr()) == 0 {
            schema_unref(schema);
            return false;
        }
        let settings = create(schema, std::ptr::null_mut(), std::ptr::null());
        schema_unref(schema);
        if settings.is_null() {
            return false;
        }
        // Retain the subscription and settings. Creating an unobserved instance
        // for each poll can leave the backend's cached initial value unchanged.
        let signal = connect(
            settings,
            c"changed::enable-animations".as_ptr(),
            changed,
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
        );
        if signal == 0 {
            unref(settings);
            return false;
        }
        let mut last = None;
        while !stop.load(Ordering::Relaxed) {
            for _ in 0..64 {
                if iteration(context.pointer, 0) == 0 || stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            let next = from_reduced(Some(get(settings, c"enable-animations".as_ptr()) == 0));
            if last != Some(next) {
                *pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(next);
                last = Some(next);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        disconnect(settings, signal);
        unref(settings);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    fn set(value: &str) -> bool {
        Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.interface",
                "enable-animations",
                value,
            ])
            .status()
            .is_ok_and(|status| status.success())
    }
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            if !set(&self.0) {
                eprintln!("Failed to restore isolated GNOME enable-animations");
            }
        }
    }
    #[test]
    #[ignore = "requires an explicitly isolated GNOME D-Bus session; changes its motion setting"]
    fn native_motion_changes_reach_the_running_service() {
        assert_eq!(
            std::env::var("AUDITORYLAB_MOTION_TEST_SESSION").as_deref(),
            Ok("isolated"),
            "run only in a disposable desktop session"
        );
        assert!(desktop_has("GNOME") && !desktop_has("KDE"));
        let original = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
            .unwrap();
        assert!(original.status.success());
        let original = String::from_utf8(original.stdout)
            .unwrap()
            .trim()
            .to_owned();
        assert!(matches!(original.as_str(), "true" | "false"));
        let restore = Restore(original);
        let mut service = Service::start();
        for (setting, expected) in [
            ("true", Preference::Full),
            ("false", Preference::Reduced),
            ("true", Preference::Full),
        ] {
            assert!(set(setting));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if service.latest() == Some(expected) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native motion change to {expected:?} was not delivered"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            eprintln!("Native GNOME enable-animations={setting} delivered {expected:?}");
        }
        let start = std::time::Instant::now();
        service.shutdown();
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        assert!(set(&restore.0));
    }
}
