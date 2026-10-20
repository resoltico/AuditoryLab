//! Portal/native GTK font preference adapters. All I/O and native parsing stays on one worker.
use super::{Typography, TypographySink, system_family};
use futures_lite::{StreamExt, future};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
mod native;
/// QFont's documented comma-separated family/point/pixel fields; no Pango-style guessing.
/// KDE emits QFont::toString. Remaining fields are validated rather than treated as a family.
fn qfont(value: &str) -> Option<(String, f32)> {
    if value.len() > 4096 {
        return None;
    }
    let fields: Vec<_> = value.split(',').collect();
    if !(10..=18).contains(&fields.len()) || fields[0].trim().is_empty() {
        return None;
    }
    let points = fields[1].parse::<f32>().ok()?;
    let pixels = fields[2].parse::<f32>().ok()?;
    for field in &fields[3..10] {
        field.parse::<i32>().ok()?;
    }
    if !matches!(fields[5], "0" | "1" | "2") {
        return None;
    }
    let size = if points > 0.0 {
        points * 96.0 / 72.0
    } else {
        pixels
    };
    (size.is_finite() && (1.0..=256.0).contains(&size)).then(|| (fields[0].trim().to_owned(), size))
}
fn resolved(
    fonts: &mut fontique::Collection,
    value: (String, f32),
    origin: &str,
) -> Option<Typography> {
    let (family, size) = value;
    for name in family.split(',').map(str::trim) {
        let id = if name.eq_ignore_ascii_case("sans") || name.eq_ignore_ascii_case("sans-serif") {
            fonts
                .generic_families(fontique::GenericFamily::SansSerif)
                .next()
        } else if name.eq_ignore_ascii_case("system-ui") {
            fonts
                .generic_families(fontique::GenericFamily::SystemUi)
                .next()
        } else {
            fonts.family_id(name)
        };
        if let Some(name) = id.and_then(|id| fonts.family_name(id)) {
            return Some(Typography {
                family: name.to_owned(),
                size,
                origin: origin.into(),
            });
        }
    }
    None
}
async fn read(
    proxy: &zbus::Proxy<'_>,
    namespace: &str,
    key: &str,
) -> zbus::Result<zbus::zvariant::OwnedValue> {
    match proxy.call("ReadOne", &(namespace, key)).await {
        Err(zbus::Error::MethodError(name, ..))
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod" =>
        {
            proxy.call("Read", &(namespace, key)).await
        }
        result => result,
    }
}
pub(super) fn listen(sender: TypographySink, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        listen_once(sender.clone(), stop.clone());
        // Reconnect after service startup/restart, without frame polling or a busy loop.
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
fn listen_once(sender: TypographySink, stop: Arc<AtomicBool>) {
    let mut fonts = fontique::Collection::new(fontique::CollectionOptions::default());
    let fallback = Typography {
        family: system_family(&mut fonts),
        size: 13.0,
        origin: "SystemUi fallback: desktop font preference unavailable or invalid".into(),
    };
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let kde = desktop.split(':').any(|s| s.eq_ignore_ascii_case("KDE"));
    let gnome = desktop.split(':').any(|s| s.eq_ignore_ascii_case("GNOME"));
    if !kde && !gnome {
        if !native::gtk_listen(&sender, &stop, &mut fonts) {
            let _ = sender.send(fallback);
        }
        return;
    }
    // GNOME's own settings include effective text scaling. Older GTK portals
    // can return a stale scale even while native GSettings has changed.
    if gnome && native::gnome_listen(&sender, &stop, &mut fonts) {
        return;
    }
    async_io::block_on(async {
        let connect = async {
            zbus::connection::Builder::session()?
                .method_timeout(Duration::from_secs(2))
                .build()
                .await
        };
        let conn = future::race(connect, async {
            async_io::Timer::after(Duration::from_secs(2)).await;
            Err(zbus::Error::Failure(
                "font settings connection timed out".into(),
            ))
        })
        .await;
        if let Ok(conn) = conn
            && let Ok(proxy) = zbus::Proxy::new(
                &conn,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
            )
            .await
            && let Ok(mut changes) = proxy.receive_signal("SettingChanged").await
        {
            let mut last = None;
            while !stop.load(Ordering::Relaxed) {
                let (namespace, key) = if kde {
                    ("org.kde.kdeglobals.General", "font")
                } else {
                    ("org.gnome.desktop.interface", "font-name")
                };
                let value: Result<zbus::zvariant::OwnedValue, _> =
                    read(&proxy, namespace, key).await;
                let preference = value
                    .ok()
                    .and_then(|v| v.downcast_ref::<&str>().ok().map(str::to_owned))
                    .and_then(|v| if kde { qfont(&v) } else { native::pango(&v) });
                let mut next = preference.and_then(|p| {
                    resolved(
                        &mut fonts,
                        p,
                        if kde {
                            "KDE portal QFont"
                        } else {
                            "GTK/GNOME portal Pango"
                        },
                    )
                });
                let mut has_scale = kde;
                if let Some(font) = next.as_mut()
                    && !kde
                {
                    let scaling: Result<zbus::zvariant::OwnedValue, _> =
                        read(&proxy, namespace, "text-scaling-factor").await;
                    if let Some(scale) = scaling
                        .ok()
                        .and_then(|v| v.downcast_ref::<f64>().ok())
                        .filter(|s| s.is_finite() && (0.5..=4.0).contains(s))
                    {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "validated native text scale"
                        )]
                        {
                            font.size *= scale as f32;
                            has_scale = true;
                        }
                    }
                }
                if !kde
                    && (next.is_none() || !has_scale)
                    && native::gnome_listen(&sender, &stop, &mut fonts)
                {
                    return;
                }
                if next.is_none() && !kde {
                    // GTK's XSettings-backed property is authoritative on non-GNOME GTK desktops.
                    if native::gtk_listen(&sender, &stop, &mut fonts) {
                        return;
                    }
                }
                let next = next.unwrap_or_else(|| fallback.clone());
                if last.as_ref() != Some(&next) {
                    last = Some(next.clone());
                    if sender.send(next).is_err() {
                        return;
                    }
                }
                // Coalesce changes; never race two preference queries or let a late old reply win.
                loop {
                    let change = future::race(async { Some(changes.next().await) }, async {
                        async_io::Timer::after(Duration::from_millis(200)).await;
                        None
                    })
                    .await;
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if matches!(change, Some(None)) {
                        return;
                    }
                    if let Some(Some(message)) = change
                        && let Ok((ns, _, _)) =
                            message
                                .body()
                                .deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
                        && ns == namespace
                    {
                        break;
                    }
                }
            }
            return;
        }
        if gnome && native::gnome_listen(&sender, &stop, &mut fonts) {
            return;
        }
        if !kde && native::gtk_listen(&sender, &stop, &mut fonts) {
            return;
        }
        let _ = sender.send(fallback);
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qfont_is_not_a_pango_last_token_parser() {
        assert_eq!(
            qfont("Noto Sans 2,11,-1,5,400,0,0,0,0,0"),
            Some(("Noto Sans 2".into(), 11.0 * 96.0 / 72.0))
        );
        assert_eq!(
            qfont("Some Bold Family,-1,18,5,400,0,0,0,0,0").map(|v| v.1),
            Some(18.0)
        );
        for bad in [
            "Noto Sans 11",
            "Noto,NaN,-1,5,400,0,0,0,0,0",
            "Foo,12,-1,5,400,42,0,0,0,0",
            "Foo,0,-1,5,400,0,0,0,0,0",
        ] {
            assert!(qfont(bad).is_none(), "{bad}");
        }
    }
    #[test]
    #[ignore = "requires an explicitly isolated GNOME D-Bus session; changes its font and scale"]
    fn native_gnome_font_and_scale_changes_reach_the_running_service() {
        use std::process::Command;
        assert_eq!(
            std::env::var("AUDITORYLAB_FONT_TEST_SESSION").as_deref(),
            Ok("isolated"),
            "run only in a disposable desktop session"
        );
        fn get(key: &str) -> String {
            let output = Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", key])
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }
        fn set(key: &str, value: &str) -> bool {
            Command::new("gsettings")
                .args(["set", "org.gnome.desktop.interface", key, value])
                .status()
                .is_ok_and(|s| s.success())
        }
        struct Restore(String, String);
        impl Drop for Restore {
            fn drop(&mut self) {
                if !set("font-name", &self.0) || !set("text-scaling-factor", &self.1) {
                    eprintln!("Failed to restore isolated GNOME typography");
                }
            }
        }
        let restore = Restore(get("font-name"), get("text-scaling-factor"));
        assert!(set("font-name", "DejaVu Sans 12"));
        assert!(set("text-scaling-factor", "1.0"));
        let mut service = crate::typography::TypographyService::start();
        for (scale, size) in [("1.0", 16.0), ("2.0", 32.0), ("1.0", 16.0)] {
            assert!(set("text-scaling-factor", scale));
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(font) = service.latest()
                    && font.family == "DejaVu Sans"
                    && (font.size - size).abs() < 0.01
                {
                    eprintln!(
                        "Native GNOME font/scale delivered {} at {} logical px",
                        font.family, font.size
                    );
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "native GNOME font scale {scale} was not delivered"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        service.shutdown();
        assert!(set("font-name", &restore.0));
        assert!(set("text-scaling-factor", &restore.1));
    }
}
