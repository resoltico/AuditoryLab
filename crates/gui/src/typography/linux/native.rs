//! Optional native Pango parser and GTK/XSettings listener; no font installation or settings writes.
#![allow(
    unsafe_code,
    reason = "bounded native C ABI calls; symbols/objects remain live on one settings thread and owned strings/descriptions are freed"
)]
use super::*;
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_void};
// Native type registration and event sources can outlive the listener. Keep
// their library code mapped for the process lifetime, exactly as static linking would.
static PANGO: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static GTK: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static GOBJECT: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static GLIB: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
fn library(
    slot: &'static std::sync::OnceLock<Option<Library>>,
    name: &str,
) -> Option<&'static Library> {
    // SAFETY: fixed OS library names; every symbol is checked before use.
    slot.get_or_init(|| unsafe { Library::new(name) }.ok())
        .as_ref()
}
static GIO: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
static GDK: std::sync::OnceLock<Option<Library>> = std::sync::OnceLock::new();
#[derive(Debug, PartialEq)]
pub(super) struct PangoFont {
    family: String,
    pixels: f32,
    pub(super) absolute: bool,
}
impl PangoFont {
    pub(super) fn at_font_scale(self, scale: f32) -> (String, f32) {
        (
            self.family,
            self.pixels * if self.absolute { 1.0 } else { scale },
        )
    }
}
/// Parse with Pango itself: family names may contain spaces, digits and style words.
#[allow(
    clippy::cast_precision_loss,
    reason = "native Pango scaled point size fits f32"
)]
pub(super) fn pango(text: &str) -> Option<PangoFont> {
    if text.len() > 4096 {
        return None;
    }
    let text = CString::new(text).ok()?;
    // SAFETY: all symbols use Pango's public C signatures; description lives until free.
    unsafe {
        let lib = library(&PANGO, "libpango-1.0.so.0")?;
        let parse = lib
            .get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(
                b"pango_font_description_from_string\0",
            )
            .ok()?;
        let family = lib
            .get::<unsafe extern "C" fn(*const c_void) -> *const c_char>(
                b"pango_font_description_get_family\0",
            )
            .ok()?;
        let size = lib
            .get::<unsafe extern "C" fn(*const c_void) -> i32>(b"pango_font_description_get_size\0")
            .ok()?;
        let absolute = lib
            .get::<unsafe extern "C" fn(*const c_void) -> i32>(
                b"pango_font_description_get_size_is_absolute\0",
            )
            .ok()?;
        let free = lib
            .get::<unsafe extern "C" fn(*mut c_void)>(b"pango_font_description_free\0")
            .ok()?;
        let desc = parse(text.as_ptr());
        if desc.is_null() {
            return None;
        }
        let f = family(desc);
        let family = if f.is_null() {
            None
        } else {
            CStr::from_ptr(f).to_str().ok().map(str::to_owned)
        };
        let absolute = absolute(desc) != 0;
        let pixels = size(desc) as f32 / 1024.0 * if absolute { 1.0 } else { 96.0 / 72.0 };
        free(desc);
        family
            .filter(|s| !s.trim().is_empty())
            .filter(|_| pixels.is_finite() && (1.0..=256.0).contains(&pixels))
            .map(|family| PangoFont {
                family,
                pixels,
                absolute,
            })
    }
}
// GValue's stable public ABI on the two supported 64-bit Linux architectures.
#[repr(C)]
struct NativeValue {
    kind: usize,
    data: [u64; 2],
}
unsafe extern "C" fn changed(_: *mut c_void, _: *mut c_void, data: *mut c_void) {
    // SAFETY: a borrowed AtomicBool is connected only while its enclosing GTK listener lives.
    unsafe { &*data.cast::<AtomicBool>() }.store(true, Ordering::Relaxed);
}
#[allow(
    clippy::cast_precision_loss,
    reason = "validated native Xft DPI converts scaled pixels to logical pixels once"
)]
pub(super) fn gtk_listen(
    sender: &TypographySink,
    stop: &AtomicBool,
    fonts: &mut fontique::Collection,
) -> bool {
    // SAFETY: GTK is initialized and used exclusively on this worker. Winit/RFD portal
    // do not initialize GTK. No widget/window is created and no preference is written.
    unsafe {
        let Some(gtk) = library(&GTK, "libgtk-3.so.0") else {
            return false;
        };
        let Some(gobject) = library(&GOBJECT, "libgobject-2.0.so.0") else {
            return false;
        };
        let Some(glib) = library(&GLIB, "libglib-2.0.so.0") else {
            return false;
        };
        macro_rules! symbol {
            ($lib:expr,$name:literal,$ty:ty) => {
                match $lib.get::<$ty>(concat!($name, "\0").as_bytes()) {
                    Ok(s) => s,
                    Err(_) => return false,
                }
            };
        }
        let init = symbol!(
            gtk,
            "gtk_init_check",
            unsafe extern "C" fn(*mut i32, *mut *mut *mut c_char) -> i32
        );
        let settings = symbol!(
            gtk,
            "gtk_settings_get_default",
            unsafe extern "C" fn() -> *mut c_void
        );
        let get = symbol!(
            gobject,
            "g_object_get",
            unsafe extern "C" fn(*mut c_void, *const c_char, ...)
        );
        let connect = symbol!(
            gobject,
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
            gobject,
            "g_signal_handler_disconnect",
            unsafe extern "C" fn(*mut c_void, u64)
        );
        let iteration = symbol!(
            glib,
            "g_main_context_iteration",
            unsafe extern "C" fn(*mut c_void, i32) -> i32
        );
        let free = symbol!(glib, "g_free", unsafe extern "C" fn(*mut c_void));
        if init(std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
            return false;
        }
        let Some(gdk) = library(&GDK, "libgdk-3.so.0") else {
            return false;
        };
        let screen = symbol!(
            gdk,
            "gdk_screen_get_default",
            unsafe extern "C" fn() -> *mut c_void
        );
        let setting = symbol!(
            gdk,
            "gdk_screen_get_setting",
            unsafe extern "C" fn(*mut c_void, *const c_char, *mut NativeValue) -> i32
        );
        let value_init = symbol!(
            gobject,
            "g_value_init",
            unsafe extern "C" fn(*mut NativeValue, usize) -> *mut NativeValue
        );
        let value_unset = symbol!(
            gobject,
            "g_value_unset",
            unsafe extern "C" fn(*mut NativeValue)
        );
        let screen = screen();
        if screen.is_null() {
            return false;
        }
        let mut value = NativeValue {
            kind: 0,
            data: [0; 2],
        };
        value_init(&raw mut value, 64); // G_TYPE_STRING
        let present = setting(screen, c"gtk-font-name".as_ptr(), &raw mut value) != 0;
        value_unset(&raw mut value);
        if !present {
            return false;
        }
        let settings = settings();
        if settings.is_null() {
            return false;
        }
        let dirty = AtomicBool::new(true);
        let ptr = (&raw const dirty).cast_mut().cast();
        let one = connect(
            settings,
            c"notify::gtk-font-name".as_ptr(),
            changed,
            ptr,
            std::ptr::null(),
            0,
        );
        let two = connect(
            settings,
            c"notify::gtk-xft-dpi".as_ptr(),
            changed,
            ptr,
            std::ptr::null(),
            0,
        );
        let mut last = None;
        while !stop.load(Ordering::Relaxed) {
            for _ in 0..64 {
                if iteration(std::ptr::null_mut(), 0) == 0 || stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            if dirty.swap(false, Ordering::Relaxed) {
                let mut text: *mut c_char = std::ptr::null_mut();
                let mut dpi: i32 = 0;
                get(
                    settings,
                    c"gtk-font-name".as_ptr(),
                    &raw mut text,
                    c"gtk-xft-dpi".as_ptr(),
                    &raw mut dpi,
                    std::ptr::null::<c_char>(),
                );
                let value = if text.is_null() {
                    None
                } else {
                    CStr::from_ptr(text).to_str().ok().and_then(pango)
                };
                if !text.is_null() {
                    free(text.cast());
                }
                let next = value
                    .and_then(|font| {
                        // GTK supplies font DPI independently of GDK window scaling.
                        let scale = if dpi > 0 {
                            dpi as f32 / (1024.0 * 96.0)
                        } else {
                            1.0
                        };
                        super::resolved(
                            fonts,
                            font.at_font_scale(scale),
                            "GTK native XSettings/Pango",
                        )
                    })
                    .unwrap_or_else(|| Typography {
                        family: system_family(fonts),
                        size: 13.0,
                        origin: "SystemUi fallback: invalid/unavailable GTK font".into(),
                    });
                if last.as_ref() != Some(&next) {
                    last = Some(next.clone());
                    if sender.send(next).is_err() {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        disconnect(settings, one);
        disconnect(settings, two);
        true
    }
}

/// Native GNOME family/point-size/text-scale source when the portal lacks necessary keys.
#[allow(
    clippy::cast_possible_truncation,
    reason = "validated native text scaling"
)]
pub(super) fn gnome_listen(
    sender: &TypographySink,
    stop: &AtomicBool,
    fonts: &mut fontique::Collection,
) -> bool {
    // SAFETY: schemas are checked before creating GSettings; objects, strings and
    // detailed signals use their documented C ABIs on this single settings worker.
    unsafe {
        let Some(gio) = library(&GIO, "libgio-2.0.so.0") else {
            return false;
        };
        let Some(gobject) = library(&GOBJECT, "libgobject-2.0.so.0") else {
            return false;
        };
        let Some(glib) = library(&GLIB, "libglib-2.0.so.0") else {
            return false;
        };
        macro_rules! symbol {
            ($lib:expr,$name:literal,$ty:ty) => {
                match $lib.get::<$ty>(concat!($name, "\0").as_bytes()) {
                    Ok(s) => s,
                    Err(_) => return false,
                }
            };
        }
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
        let create = symbol!(
            gio,
            "g_settings_new_full",
            unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void
        );
        let schema_unref = symbol!(
            gio,
            "g_settings_schema_unref",
            unsafe extern "C" fn(*mut c_void)
        );
        let has_key = symbol!(
            gio,
            "g_settings_schema_has_key",
            unsafe extern "C" fn(*mut c_void, *const c_char) -> i32
        );
        let string = symbol!(
            gio,
            "g_settings_get_string",
            unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char
        );
        let double = symbol!(
            gio,
            "g_settings_get_double",
            unsafe extern "C" fn(*mut c_void, *const c_char) -> f64
        );
        let connect = symbol!(
            gobject,
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
            gobject,
            "g_signal_handler_disconnect",
            unsafe extern "C" fn(*mut c_void, u64)
        );
        let unref = symbol!(gobject, "g_object_unref", unsafe extern "C" fn(*mut c_void));
        let iteration = symbol!(
            glib,
            "g_main_context_iteration",
            unsafe extern "C" fn(*mut c_void, i32) -> i32
        );
        let free = symbol!(glib, "g_free", unsafe extern "C" fn(*mut c_void));
        let source = source();
        if source.is_null() {
            return false;
        }
        let schema = lookup(source, c"org.gnome.desktop.interface".as_ptr(), 1);
        if schema.is_null() {
            return false;
        }
        if has_key(schema, c"font-name".as_ptr()) == 0
            || has_key(schema, c"text-scaling-factor".as_ptr()) == 0
        {
            schema_unref(schema);
            return false;
        }
        let settings = create(schema, std::ptr::null_mut(), std::ptr::null());
        schema_unref(schema);
        if settings.is_null() {
            return false;
        }
        let dirty = AtomicBool::new(true);
        let data = (&raw const dirty).cast_mut().cast();
        let one = connect(
            settings,
            c"changed::font-name".as_ptr(),
            changed,
            data,
            std::ptr::null(),
            0,
        );
        let two = connect(
            settings,
            c"changed::text-scaling-factor".as_ptr(),
            changed,
            data,
            std::ptr::null(),
            0,
        );
        let mut last = None;
        while !stop.load(Ordering::Relaxed) {
            for _ in 0..64 {
                if iteration(std::ptr::null_mut(), 0) == 0 || stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            if dirty.swap(false, Ordering::Relaxed) {
                let text = string(settings, c"font-name".as_ptr());
                let scale = double(settings, c"text-scaling-factor".as_ptr());
                let value = if text.is_null() {
                    None
                } else {
                    CStr::from_ptr(text).to_str().ok().and_then(pango)
                };
                if !text.is_null() {
                    free(text.cast());
                }
                let next = value
                    .filter(|_| scale.is_finite() && (0.5..=4.0).contains(&scale))
                    .and_then(|font| {
                        super::resolved(
                            fonts,
                            font.at_font_scale(scale as f32),
                            "GNOME native GSettings/Pango",
                        )
                    })
                    .unwrap_or_else(|| Typography {
                        family: system_family(fonts),
                        size: 13.0,
                        origin: "SystemUi fallback: invalid GNOME family/size/scale".into(),
                    });
                if last.as_ref() != Some(&next) {
                    last = Some(next.clone());
                    if sender.send(next).is_err() {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        disconnect(settings, one);
        disconnect(settings, two);
        unref(settings);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::pango;
    #[test]
    fn native_pango_parses_family_style_points_and_absolute_pixels() {
        assert_eq!(
            pango("DejaVu Serif Bold Italic 15").map(|font| font.at_font_scale(1.0)),
            Some(("DejaVu Serif".into(), 20.0))
        );
        assert_eq!(
            pango("Noto Sans 11.25").map(|font| font.at_font_scale(1.0)),
            Some(("Noto Sans".into(), 15.0))
        );
        assert_eq!(
            pango("Cantarell 16px").map(|font| font.at_font_scale(1.75)),
            Some(("Cantarell".into(), 16.0))
        );
        assert_eq!(
            pango("DejaVu Sans 12").unwrap().at_font_scale(1.75),
            ("DejaVu Sans".into(), 28.0)
        );
        assert_eq!(pango("Cantarell"), None);
        assert_eq!(pango("Cantarell 0"), None);
        assert_eq!(pango("bad\0family 12"), None);
    }
}
