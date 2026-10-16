// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Explicit macOS native Quit check. No audio, shipping arguments, or native FFI.
#[cfg(target_os = "macos")]
mod native {
    use slint::ComponentHandle;
    use std::{
        cell::RefCell,
        fs,
        path::Path,
        process::{Child, Command, Stdio},
        rc::Rc,
        time::{Duration, Instant},
    };

    slint::slint! { export component Probe inherits Window { title: "Auditory Lab termination test"; width: 320px; height: 100px; Text { text: "Nonshipping lifetime test; no audio"; } } }

    fn wait_for(path: &Path, child: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !path.exists() {
            if let Some(status) = child.try_wait()? {
                return Err(format!("child exited early: {status}").into());
            }
            if Instant::now() >= deadline {
                return Err(format!("timed out waiting for {}", path.display()).into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
        if let Some(directory) = std::env::var_os("AUDITORY_TERMINATION_CHILD") {
            return child(Path::new(&directory));
        }
        if std::env::var_os("AUDITORY_GUI_TERMINATION_TEST").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            println!(
                "native_termination: SKIPPED (set AUDITORY_GUI_TERMINATION_TEST=1 on macOS with a native display)"
            );
            return Ok(());
        }
        let evidence =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/native-termination");
        fs::create_dir_all(&evidence)?;
        let scratch = tempfile::Builder::new()
            .prefix("native-quit-")
            .tempdir_in(&evidence)?
            .keep();
        println!("native_termination evidence: {}", scratch.display());
        let source = scratch.join("terminate.swift");
        let helper = scratch.join("terminate");
        fs::write(
            &source,
            "import AppKit\nlet pid = Int32(CommandLine.arguments[1])!\nguard let app = NSRunningApplication(processIdentifier: pid) else { exit(2) }\n_ = app.terminate()\n",
        )?;
        if !Command::new("swiftc")
            .arg(&source)
            .arg("-o")
            .arg(&helper)
            .status()?
            .success()
        {
            return Err("Swift native termination helper failed to compile".into());
        }
        for mode in ["idle", "blocked", "repeated", "dialog"] {
            let directory = scratch.join(mode);
            fs::create_dir(&directory)?;
            let output = fs::File::create(directory.join("output.log"))?;
            let mut process = ChildGuard(
                Command::new(std::env::current_exe()?)
                    .env("AUDITORY_TERMINATION_CHILD", &directory)
                    .env("AUDITORY_TERMINATION_MODE", mode)
                    .stdout(output.try_clone()?)
                    .stderr(output)
                    .spawn()?,
            );
            wait_for(&directory.join("ready"), &mut process.0)?;
            if !Command::new(&helper)
                .arg(process.0.id().to_string())
                .stdout(Stdio::null())
                .status()?
                .success()
            {
                return Err("native terminate helper failed".into());
            }
            wait_for(&directory.join("close-request"), &mut process.0)?;
            if mode != "idle" {
                std::thread::sleep(Duration::from_millis(200));
                if process.0.try_wait()?.is_some()
                    || !directory.join("alive").exists()
                    || directory.join("joined").exists()
                {
                    return Err(format!("{mode}: native Quit bypassed lifetime hold").into());
                }
                if mode == "repeated" {
                    if !Command::new(&helper)
                        .arg(process.0.id().to_string())
                        .status()?
                        .success()
                    {
                        return Err("repeated native terminate helper failed".into());
                    }
                    wait_for(&directory.join("second-close-request"), &mut process.0)?;
                    if process.0.try_wait()?.is_some() {
                        return Err("second Quit bypassed lifetime hold".into());
                    }
                }
                fs::write(directory.join("release"), b"release")?;
            }
            let deadline = Instant::now() + Duration::from_secs(20);
            let status = loop {
                if let Some(status) = process.0.try_wait()? {
                    break status;
                }
                if Instant::now() >= deadline {
                    return Err(format!("{mode}: child failed to quit after release").into());
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            let output = fs::read_to_string(directory.join("output.log"))?;
            if !status.success()
                || !directory.join("joined").exists()
                || !directory.join("returned").exists()
            {
                return Err(format!(
                    "{mode}: no proven join and event-loop return: {status}\n{output}"
                )
                .into());
            }
            println!(
                "native_termination {mode}: native close request observed, window retained until release/join, event loop returned\n{output}"
            );
        }
        Ok(())
    }

    struct Lifetime {
        pending: bool,
        finished: bool,
        requests: usize,
        worker: Option<std::thread::JoinHandle<()>>,
        release: Option<std::sync::mpsc::Sender<()>>,
        dialog: bool,
    }
    fn child(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let mode = std::env::var("AUDITORY_TERMINATION_MODE")?;
        let window = Probe::new()?;
        let (send, receive) = std::sync::mpsc::channel();
        let worker = matches!(mode.as_str(), "blocked" | "repeated").then(|| {
            std::thread::spawn(move || {
                let _ = receive.recv();
            })
        });
        let state = Rc::new(RefCell::new(Lifetime {
            pending: false,
            finished: false,
            requests: 0,
            worker,
            release: Some(send),
            dialog: mode == "dialog",
        }));
        let close = Rc::clone(&state);
        let target = directory.to_path_buf();
        window.window().on_close_requested(move || {
            let mut state = close.borrow_mut();
            state.pending = true;
            state.requests += 1;
            let _ = fs::write(target.join("close-request"), b"received");
            if state.requests > 1 {
                let _ = fs::write(target.join("second-close-request"), b"received");
            }
            // The same lifetime contract as Controller::request_close: an actual
            // worker and pending dialog keep the native window present.
            slint::CloseRequestResponse::KeepWindowShown
        });
        window.show()?;
        let weak = window.as_weak();
        let target = directory.to_path_buf();
        let lifetime = Rc::clone(&state);
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(20),
            move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let mut state = lifetime.borrow_mut();
                if state.finished {
                    return;
                }
                if !state.pending {
                    let _ = fs::write(target.join("ready"), b"ready");
                    return;
                }
                assert!(window.window().is_visible());
                let _ = fs::write(target.join("alive"), b"visible");
                if target.join("release").exists() {
                    state.dialog = false;
                    if let Some(release) = state.release.take() {
                        let _ = release.send(());
                    }
                }
                if state
                    .worker
                    .as_ref()
                    .is_some_and(std::thread::JoinHandle::is_finished)
                    && let Some(worker) = state.worker.take()
                {
                    assert!(worker.join().is_ok());
                }
                if state.worker.is_none() && !state.dialog {
                    state.finished = true;
                    let _ = fs::write(target.join("joined"), b"joined");
                    let _ = window.hide();
                    let _ = slint::quit_event_loop();
                }
            },
        );
        slint::run_event_loop()?;
        timer.stop();
        if state.borrow().worker.is_some() {
            return Err("event loop exited before worker joined".into());
        }
        fs::write(directory.join("returned"), b"event loop returned")?;
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    native::run()?;
    #[cfg(not(target_os = "macos"))]
    println!("native_termination: SKIPPED (macOS only)");
    Ok(())
}
