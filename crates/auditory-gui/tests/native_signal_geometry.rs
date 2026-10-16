// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Explicit nonshipping native pixel fixture over the actual production UI.
#[allow(
    unsafe_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "Slint-generated-only UI, identical to the executable"
)]
mod generated_ui {
    slint::include_modules!();
}
#[path = "../src/signal_geometry.rs"]
mod signal_geometry;
use auditory_app::{SampleRate, analyze_samples};
use slint::ComponentHandle;
use std::{cell::RefCell, io::Write as _, path::PathBuf, rc::Rc};
const SIZES: [(u16, u16); 3] = [(600, 640), (900, 800), (1200, 900)];
const SCENES: [&str; 3] = ["anchors", "asymmetric", "gap"];
#[derive(Default)]
struct Capture {
    next: usize,
    waited: bool,
    entries: Vec<String>,
    error: Option<String>,
}
fn scene(ui: &generated_ui::MainWindow, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (left, right) = match name {
        // Explicit presentation sentinels, not invented playback observations.
        "anchors" => (
            "M0 0H1024M0 25H1024M0 75H1024M0 100H1024".to_owned(),
            "M0 0V100M256 0V100M768 0V100M1024 0V100".to_owned(),
        ),
        "gap" => (
            "M0 25H384M640 75H1024".to_owned(),
            "M0 75H384M640 25H1024".to_owned(),
        ),
        "asymmetric" => {
            let mut samples = Vec::new();
            for (minimum, maximum) in [(0.25, 0.75), (-0.75, -0.25), (-0.5, 1.0), (0.0, 0.5)] {
                samples.extend((0..32).map(|i| [if i % 2 == 0 { minimum } else { maximum }, 0.0]));
            }
            let series = analyze_samples(SampleRate::new(8000)?, 123, &samples, 64)?;
            let [left, right] = signal_geometry::paths(&series)?;
            (left, right)
        }
        _ => return Err("unknown fixture scene".into()),
    };
    let (start, end) = if name == "asymmetric" {
        (123_u32, 251_u32)
    } else {
        (0_u32, 1024_u32)
    };
    ui.set_peak_start(format!("frame {start}\n≈{:.3} s", f64::from(start) / 8000.0).into());
    ui.set_peak_end(format!("frame {end}\n≈{:.3} s", f64::from(end) / 8000.0).into());
    ui.set_peak_left(left.into());
    ui.set_peak_right(right.into());
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("AUDITORY_GUI_GEOMETRY_TEST").as_deref() != Some(std::ffi::OsStr::new("1"))
    {
        println!("native_signal_geometry: SKIPPED (explicit native pixel fixture)");
        return Ok(());
    }
    let budget_source = auditory_app::load_bytes(
        b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n",
        auditory_app::Origin::Reader("native geometry fixture".into()),
    )?;
    signal_geometry::validate_source(budget_source.description())?;
    if signal_geometry::reserve_bytes() > auditory_app::SIGNAL_MEMORY_LIMIT {
        return Err("renderer reserve exceeds the whole-product budget".into());
    }
    let directory = PathBuf::from(
        std::env::var_os("AUDITORY_GUI_GEOMETRY_DIR")
            .ok_or("missing owned geometry output directory")?,
    );
    if !directory.is_absolute()
        || !directory.is_dir()
        || std::fs::read_dir(&directory)?.next().is_some()
    {
        return Err("geometry output requires an existing empty absolute owned directory".into());
    }
    let ui = generated_ui::MainWindow::new()?;
    ui.set_source_title("Native geometry fixture — no audio".into());
    ui.set_status("Offline presentation sentinels".into());
    ui.set_peak_context("Known amplitude and source-frame coordinates".into());
    ui.set_signal_context("".into());
    ui.set_signal_through("No playback operation".into());
    scene(&ui, SCENES[0])?;
    ui.show()?;
    resize(&ui, 600, 640);
    let progress = Rc::new(RefCell::new(Capture::default()));
    let state = Rc::clone(&progress);
    let weak = ui.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.error.is_some() || state.next >= 9 {
                return;
            }
            if !state.waited {
                state.waited = true;
                return;
            }
            if let Err(error) = capture(&ui, &directory, &mut state) {
                state.error = Some(error.to_string());
                let _ = slint::quit_event_loop();
            }
        },
    );
    slint::run_event_loop()?;
    timer.stop();
    let state = progress.borrow();
    if let Some(error) = &state.error {
        return Err(error.clone().into());
    }
    if state.next != 9 {
        return Err("native geometry capture incomplete".into());
    }
    Ok(())
}

fn capture(
    ui: &generated_ui::MainWindow,
    directory: &std::path::Path,
    state: &mut Capture,
) -> Result<(), Box<dyn std::error::Error>> {
    let (width, height) = SIZES[state.next / 3];
    let name = SCENES[state.next % 3];
    let image = ui.window().take_snapshot()?;
    if state.next == 0 {
        println!("NATIVE_WINDOW_STARTED: production geometry fixture with actual native snapshot");
    }
    let scale = ui.window().scale_factor();
    let physical = ui.window().size();
    if image.width() != physical.width
        || image.height() != physical.height
        || (f64::from(physical.width) - f64::from(width) * f64::from(scale)).abs() > 2.0
        || (f64::from(physical.height) - f64::from(height) * f64::from(scale)).abs() > 2.0
    {
        return Err(
            "native capture did not retain requested logical size/actual physical scale".into(),
        );
    }
    let filename = format!("{name}-{width}x{height}.ppm");
    let mut file = std::fs::File::create(directory.join(&filename))?;
    write!(file, "P6\n{} {}\n255\n", image.width(), image.height())?;
    let mut rgb = Vec::with_capacity(image.as_slice().len() * 3);
    for pixel in image.as_slice() {
        rgb.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
    }
    file.write_all(&rgb)?;
    state.entries.push(format!("{{\"scene\":\"{name}\",\"file\":\"{filename}\",\"requested_logical\":[{width},{height}],\"window_physical\":[{},{}],\"snapshot_physical\":[{},{}],\"scale_factor\":{scale}}}", physical.width, physical.height, image.width(), image.height()));
    println!(
        "NATIVE_SIGNAL_GEOMETRY_CAPTURED scene={name} logical={width}x{height} physical={}x{} scale={scale}",
        image.width(),
        image.height()
    );
    state.next += 1;
    state.waited = false;
    if state.next == 9 {
        std::fs::write(
            directory.join("manifest.json"),
            format!(
                "{{\"format_version\":1,\"producer\":\"production MainWindow/SignalLane via include_modules\",\"source\":\"explicit geometry sentinels and auditory_app::analyze_samples; no playback\",\"cases\":[{}]}}\n",
                state.entries.join(",")
            ),
        )?;
        println!("NATIVE_SIGNAL_GEOMETRY_COMPLETE cases=9");
        ui.hide()?;
        slint::quit_event_loop()?;
    } else {
        let (next_width, next_height) = SIZES[state.next / 3];
        scene(ui, SCENES[state.next % 3])?;
        resize(ui, next_width, next_height);
        ui.window().request_redraw();
    }
    Ok(())
}

fn resize(ui: &generated_ui::MainWindow, width: u16, height: u16) {
    // The runtime scale override controls UI coordinates, while a live native
    // logical-size request can use the OS backing scale. Request exact physical
    // dimensions derived from the UI's measured scale instead.
    let size = slint::LogicalSize::new(f32::from(width), f32::from(height));
    ui.window()
        .set_size(size.to_physical(ui.window().scale_factor()));
}
