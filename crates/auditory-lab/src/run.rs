// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::args::{Command, InputSource, help};
use crate::failure::{Failure, INTERRUPTED};
use crate::report;
use auditory_core::{Preset, SampleRate, Session, StimulusError, catalog};
use auditory_device::{ListeningAdvice, Output};
use auditory_wav::export;
use ctrlc::set_handler;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn run(command: Command) -> Result<(), Failure> {
    run_with(
        command,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        cancel_on_interrupt,
    )
}

/// Perform a command. `watch_interrupts` is called once, by the commands
/// that can be cancelled, and returns the flag an interrupt sets.
fn run_with(
    command: Command,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    watch_interrupts: impl FnOnce() -> Result<Arc<AtomicBool>, Failure>,
) -> Result<(), Failure> {
    match command {
        Command::Help(topic) => output(stdout, &format!("{}\n", help(topic))),
        Command::Version => output(
            stdout,
            &format!("auditory-lab {}\n", env!("CARGO_PKG_VERSION")),
        ),
        Command::List => {
            let width = catalog::ENTRIES
                .iter()
                .map(|e| e.name.len())
                .max()
                .unwrap_or(0);
            for entry in catalog::ENTRIES {
                let source = Source::built_in(entry)?;
                output(stdout, &report::listing(entry.name, &source.preset, width))?;
            }
            Ok(())
        }
        Command::Show { name, output: None } => output(stdout, entry(&name, false)?.text),
        Command::Show {
            name,
            output: Some(path),
        } => {
            let entry = entry(&name, false)?;
            save_new(&path, entry.text)?;
            report_completed(
                stdout,
                stderr,
                &format!("saved {} -> {path:?}\n", entry.name),
            );
            Ok(())
        }
        Command::About { preset } => {
            let source = Source::read(&preset, stderr)?;
            output(stdout, &report::about(&source.origin, &source.preset))
        }
        Command::Inspect {
            preset,
            sample_rate,
        } => {
            let source = Source::read(&preset, stderr)?;
            let session = source.session(sample_rate)?;
            output(
                stdout,
                &report::resolved(&source.origin, &source.preset, &session),
            )
        }
        Command::Export {
            preset,
            output: path,
            sample_rate,
        } => {
            let source = Source::read(&preset, stderr)?;
            let session = source.session(sample_rate)?;
            // Interrupts are watched only after the preset has been read, so one
            // still ends a program that waits for a preset on a pipe.
            let cancel = watch_interrupts()?;
            notify(
                stderr,
                &format!(
                    "{}\n",
                    report::start(
                        "exporting",
                        &format!("{} -> {path:?}", source.origin),
                        &source.preset,
                        &session
                    )
                ),
            );
            match export(session, &path, &cancel) {
                Ok(exported) => {
                    let result = report::exported(exported, &source.origin, &path);
                    report_completed(stdout, stderr, &result);
                    Ok(())
                }
                Err(error) => Err(Failure::Export { path, error }),
            }
        }
        Command::Play { preset } => {
            let source = Source::read(&preset, stderr)?;
            let cancel = watch_interrupts()?;
            let playback = |error| Failure::Playback {
                origin: source.origin.clone(),
                error,
            };
            let device = Output::default_device().map_err(playback)?;
            let session = source.resolved(device.session(&source.preset.stimulus))?;
            notify(
                stderr,
                &ListeningAdvice::for_stimulus(&source.preset.stimulus).to_string(),
            );
            notify(
                stderr,
                &format!(
                    "{}{}",
                    report::start("playing", &source.origin, &source.preset, &session),
                    report::device(device.description())
                ),
            );
            let played = device.play(session, &cancel).map_err(playback)?;
            report_completed(stdout, stderr, &report::played(played, &source.origin));
            notify(stderr, &report::playback_warnings(played));
            Ok(())
        }
    }
}

/// The parsed document and the input identity used in diagnostics.
struct Source {
    origin: String,
    preset: Preset,
}
fn entry(name: &OsStr, accepts_files: bool) -> Result<&'static catalog::Entry, Failure> {
    name.to_str()
        .and_then(catalog::find)
        .ok_or_else(|| Failure::UnknownPreset {
            name: name.to_owned(),
            accepts_files,
        })
}
impl Source {
    fn built_in(entry: &catalog::Entry) -> Result<Self, Failure> {
        let origin = format!("{} (built-in)", entry.name);
        let preset = Preset::parse(entry.text).map_err(|error| Failure::Preset {
            origin: origin.clone(),
            error,
        })?;
        Ok(Self { origin, preset })
    }
    fn read(input: &InputSource, stderr: &mut impl Write) -> Result<Self, Failure> {
        match input {
            InputSource::Catalog(name) => Self::built_in(entry(name, true)?),
            InputSource::Stdin => {
                let stdin = io::stdin();
                stdin_cue(stderr, stdin.is_terminal());
                Self::from_reader("standard input".into(), stdin.lock())
            }
            InputSource::File(path) => {
                let origin = format!("{path:?}");
                let file = File::open(path).map_err(|error| Failure::Read {
                    origin: origin.clone(),
                    error,
                })?;
                Self::from_reader(origin, file)
            }
        }
    }
    fn from_reader(origin: String, reader: impl Read) -> Result<Self, Failure> {
        let mut bytes = Vec::new();
        reader
            .take(Preset::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| Failure::Read {
                origin: origin.clone(),
                error,
            })?;
        let preset = Preset::from_bytes(&bytes).map_err(|error| Failure::Preset {
            origin: origin.clone(),
            error,
        })?;
        Ok(Self { origin, preset })
    }
    fn session(&self, rate: SampleRate) -> Result<Session, Failure> {
        self.resolved(self.preset.stimulus.session(rate))
    }
    fn resolved(&self, session: Result<Session, StimulusError>) -> Result<Session, Failure> {
        session.map_err(|error| Failure::Stimulus {
            origin: self.origin.clone(),
            error,
        })
    }
}

/// Write exact text to a new file, reporting an incomplete destination if cleanup fails.
fn save_new(path: &std::path::Path, text: &str) -> Result<(), Failure> {
    save_new_with(path, text, |mut file, text| {
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
    })
}

fn save_new_with(
    path: &std::path::Path,
    text: &str,
    persist: impl FnOnce(File, &str) -> io::Result<()>,
) -> Result<(), Failure> {
    let failed = |error, removal| Failure::Save {
        path: path.to_owned(),
        error,
        removal,
    };
    let file = File::create_new(path).map_err(|error| failed(error, None))?;
    let written = persist(file, text);
    match written {
        Ok(()) => Ok(()),
        Err(error) => Err(failed(error, std::fs::remove_file(path).err())),
    }
}

/// Write and flush; a closed pipe is an expected way for the reader to stop.
pub(crate) fn output(stdout: &mut impl Write, text: &str) -> Result<(), Failure> {
    match stdout
        .write_all(terminal_text(text).as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(Failure::Output(error)),
    }
}

/// Escape untrusted terminal controls once, preserving multiline diagnostics.
pub(crate) fn terminal_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if (ch.is_control() && ch != '\n' && ch != '\t')
            || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

/// Report a finished recipe save, export or playback. A reporting failure does not undo it.
fn report_completed(stdout: &mut impl Write, stderr: &mut impl Write, result: &str) {
    if let Err(error) = output(stdout, result) {
        notify(
            stderr,
            &format!("warning: {}; {error}\n", result.trim_end()),
        );
    }
}

/// Explain the bounded read before waiting on an interactive input stream.
fn stdin_cue(stderr: &mut impl Write, is_terminal: bool) {
    if is_terminal {
        #[cfg(unix)]
        let cue = "Reading a preset from stdin until EOF. On Unix, press Ctrl-D on an empty line to finish input.\n";
        #[cfg(not(unix))]
        let cue = "Reading a preset from stdin until EOF.\n";
        notify(stderr, cue);
    }
}

/// Write notices or warnings; nothing depends on whether they could be written.
fn notify(stderr: &mut impl Write, text: &str) {
    let _ = output(stderr, text);
}

/// Set a flag on the first interrupt or termination request. A second one
/// ends the program at once, for when the first has not taken effect.
fn cancel_on_interrupt() -> Result<Arc<AtomicBool>, Failure> {
    let cancel = Arc::new(AtomicBool::new(false));
    let requested = Arc::clone(&cancel);
    set_handler(move || {
        if requested.swap(true, Ordering::Relaxed) {
            exit(i32::from(INTERRUPTED));
        }
    })
    .map_err(Failure::Interrupts)?;
    Ok(cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditory_core::PresetError;
    struct Failing {
        kind: io::ErrorKind,
        on_flush: bool,
    }
    impl Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.on_flush {
                Ok(bytes.len())
            } else {
                Err(self.kind.into())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(self.kind.into())
        }
    }
    #[test]
    fn failed_save_cleans_up_after_write_or_sync_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("copy.toml");
        for stage in ["write failed", "sync failed"] {
            let failure = save_new_with(&path, "exact", |mut file, text| {
                if stage == "sync failed" {
                    file.write_all(text.as_bytes())?;
                }
                Err(io::Error::other(stage))
            })
            .unwrap_err();
            assert_eq!(failure.exit_code(), 1);
            assert!(failure.diagnostic().contains(stage));
            assert!(!failure.diagnostic().contains("incomplete output"));
            assert!(!path.exists());
        }
    }

    #[test]
    fn failed_save_reports_both_failure_and_removal_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("copy.toml");
        let failure = save_new_with(&path, "exact", |file, _| {
            // Windows delays deletion until open handles close. Replace the path
            // only after closing ours, so real removal rejects the directory.
            drop(file);
            std::fs::remove_file(&path)?;
            std::fs::create_dir(&path)?;
            Err(io::Error::other("sync failed"))
        })
        .unwrap_err();
        assert_eq!(failure.exit_code(), 1);
        let diagnostic = failure.diagnostic();
        assert!(diagnostic.contains("sync failed"));
        assert!(diagnostic.contains(&format!("incomplete output remains at {path:?}")));
        assert!(diagnostic.contains("cannot remove it:"));
        assert!(matches!(
            failure,
            Failure::Save {
                removal: Some(_),
                ..
            }
        ));
        assert!(path.is_dir());
    }

    #[test]
    fn terminal_stdin_cue_is_best_effort_and_pipes_are_quiet() {
        let mut notice = Vec::new();
        stdin_cue(&mut notice, false);
        assert!(notice.is_empty());
        stdin_cue(&mut notice, true);
        let text = String::from_utf8(notice).unwrap();
        assert!(text.contains("stdin until EOF"));
        assert_eq!(text.lines().count(), 1);
        stdin_cue(
            &mut Failing {
                kind: io::ErrorKind::Other,
                on_flush: false,
            },
            true,
        );
    }

    #[test]
    fn unknown_catalog_recovery_matches_the_commands_input_contract() {
        let name = OsStr::new("no-such-preset");
        let show = entry(name, false).unwrap_err().to_string();
        let preset = entry(name, true).unwrap_err().to_string();
        assert!(show.contains("auditory-lab list"));
        assert!(!show.contains("file"));
        assert!(preset.contains("file such as ./x"));
    }
    #[test]
    fn reader_is_bounded_and_io_failures_remain_distinct() {
        assert!(matches!(
            Source::from_reader("input".into(), io::repeat(b' ')),
            Err(Failure::Preset {
                error: PresetError::TooLarge,
                ..
            })
        ));
        struct Bad;
        impl Read for Bad {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("unavailable"))
            }
        }
        let e = Source::from_reader("input".into(), Bad).err().unwrap();
        assert_eq!(e.exit_code(), 1);
        assert!(e.to_string().contains("unavailable"));
    }
    #[test]
    fn actual_catalog_text_is_terminal_invariant_and_a_bidi_comment_is_rejected() {
        for entry in catalog::ENTRIES {
            assert_eq!(terminal_text(entry.text), entry.text, "{}", entry.name);
        }
        let text = format!("{}\n# \u{202e} hidden\n", catalog::ENTRIES[0].text);
        assert!(Preset::parse(&text).is_ok());
        assert_ne!(terminal_text(&text), text);
    }
    #[test]
    fn terminal_boundary_escapes_controls_without_double_escaping_quotes() {
        assert_eq!(
            terminal_text("plain\n\tΩ\u{1b}[2J\r\u{0085}\u{202e}"),
            "plain\n\tΩ\\u{1b}[2J\\r\\u{85}\\u{202e}"
        );
        assert_eq!(terminal_text("\\u{1b}"), "\\u{1b}");
    }
    #[test]
    fn writes_flush_and_failure_semantics_are_preserved() {
        for on_flush in [false, true] {
            for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
                let result = output(&mut Failing { kind, on_flush }, "whole\n");
                assert_eq!(result.is_ok(), kind == io::ErrorKind::BrokenPipe);
                notify(&mut Failing { kind, on_flush }, "notice\n");
            }
        }
        let mut warnings = Vec::new();
        report_completed(
            &mut Failing {
                kind: io::ErrorKind::Other,
                on_flush: false,
            },
            &mut warnings,
            "exported done\n",
        );
        assert!(
            String::from_utf8(warnings)
                .unwrap()
                .starts_with("warning: exported done;")
        );
    }
    #[test]
    fn completed_recipe_save_survives_write_or_flush_reporting_failure() {
        let entry = &catalog::ENTRIES[0];
        for on_flush in [false, true] {
            for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("copy.toml");
                let mut warnings = Vec::new();
                run_with(
                    Command::Show {
                        name: entry.name.into(),
                        output: Some(path.clone()),
                    },
                    &mut Failing { kind, on_flush },
                    &mut warnings,
                    || panic!("recipe save installed an interrupt handler"),
                )
                .unwrap();
                assert_eq!(std::fs::read(&path).unwrap(), entry.text.as_bytes());
                let expected = if kind == io::ErrorKind::BrokenPipe {
                    String::new()
                } else {
                    format!(
                        "warning: saved {} -> {path:?}; cannot write to standard output: {}\n",
                        entry.name,
                        io::Error::from(kind)
                    )
                };
                assert_eq!(String::from_utf8(warnings).unwrap(), expected);
            }
        }
    }
    #[test]
    fn raw_recipe_show_still_reports_write_or_flush_output_failure() {
        for on_flush in [false, true] {
            for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
                let mut warnings = Vec::new();
                let result = run_with(
                    Command::Show {
                        name: catalog::ENTRIES[0].name.into(),
                        output: None,
                    },
                    &mut Failing { kind, on_flush },
                    &mut warnings,
                    || panic!("raw recipe output installed an interrupt handler"),
                );
                if kind == io::ErrorKind::BrokenPipe {
                    assert!(result.is_ok());
                } else {
                    let failure = result.unwrap_err();
                    assert_eq!(failure.exit_code(), 1);
                    assert!(matches!(failure, Failure::Output(_)));
                }
                assert!(warnings.is_empty());
            }
        }
    }
    #[test]
    fn stderr_failure_does_not_stop_export_and_interrupts_are_installed_after_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.toml");
        std::fs::write(
            &path,
            "schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
        )
        .unwrap();
        let destination = directory.path().join("output.wav");
        let command = Command::Export {
            preset: InputSource::File(path),
            output: destination.clone(),
            sample_rate: SampleRate::DEFAULT,
        };
        let called = std::cell::Cell::new(false);
        let mut stdout = Vec::new();
        super::run_with(
            command,
            &mut stdout,
            &mut Failing {
                kind: io::ErrorKind::Other,
                on_flush: false,
            },
            || {
                called.set(true);
                Ok(Arc::default())
            },
        )
        .unwrap();
        assert!(called.get());
        assert!(destination.exists());
        assert!(String::from_utf8(stdout).unwrap().contains("exported"));
        for command in [
            Command::List,
            Command::Help(crate::args::HelpTopic::Global),
            Command::Show {
                name: catalog::ENTRIES[0].name.into(),
                output: None,
            },
            Command::Inspect {
                preset: InputSource::Catalog(catalog::ENTRIES[0].name.into()),
                sample_rate: SampleRate::DEFAULT,
            },
        ] {
            super::run_with(command, &mut Vec::new(), &mut Vec::new(), || {
                panic!("non-cancellable command installed handler")
            })
            .unwrap();
        }
        let command = Command::Export {
            preset: InputSource::File(directory.path().join("absent.toml")),
            output: destination,
            sample_rate: SampleRate::DEFAULT,
        };
        assert!(
            super::run_with(command, &mut Vec::new(), &mut Vec::new(), || panic!(
                "handler installed before input"
            ))
            .is_err()
        );
    }
}

#[cfg(all(test, unix))]
mod interrupt_tests {
    use std::io::{BufRead, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    struct ChildCleanup<'process>(&'process mut Child);
    impl Drop for ChildCleanup<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn second_interrupt_exits_after_acknowledged_first_request() {
        if std::env::var_os("AUDITORY_INTERRUPT_CHILD").is_some() {
            let cancel = super::cancel_on_interrupt().unwrap();
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "READY").unwrap();
            stdout.flush().unwrap();
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            writeln!(stdout, "ACK").unwrap();
            stdout.flush().unwrap();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        for signal in ["-INT", "-TERM"] {
            let mut process = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "run::interrupt_tests::second_interrupt_exits_after_acknowledged_first_request",
                    "--nocapture",
                ])
                .env("AUDITORY_INTERRUPT_CHILD", "1")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let child = ChildCleanup(&mut process);
            let stdout = child.0.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in std::io::BufReader::new(stdout).lines() {
                    if send.send(line).is_err() {
                        break;
                    }
                }
            });
            let wait = |expected| {
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                loop {
                    let line = receive
                        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                        .unwrap()
                        .unwrap();
                    if line == expected {
                        break;
                    }
                }
            };
            wait("READY");
            let send_signal = || {
                assert!(
                    Command::new("kill")
                        .args([signal, &child.0.id().to_string()])
                        .status()
                        .unwrap()
                        .success()
                );
            };
            send_signal();
            wait("ACK");
            send_signal();
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert_eq!(status.code(), Some(130));
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            reader.join().unwrap();
        }
    }

    #[test]
    fn child_cleanup_terminates_and_reaps_a_still_running_process() {
        use std::os::unix::process::ExitStatusExt;
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "run::interrupt_tests::second_interrupt_exits_after_acknowledged_first_request",
                "--nocapture",
            ])
            .env("AUDITORY_INTERRUPT_CHILD", "1")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = process.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut ready = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while let Ok(Ok(line)) =
            receive.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            if line == "READY" {
                ready = true;
                break;
            }
        }
        {
            let _cleanup = ChildCleanup(&mut process);
        }
        let status = process.try_wait().unwrap();
        // This fallback also cleans up the deliberately broken guard mutant.
        let _ = process.kill();
        let _ = process.wait();
        reader.join().unwrap();
        assert!(ready, "child did not acknowledge installed handler");
        assert!(
            status.is_some_and(|status| status.signal() == Some(9)),
            "guard did not terminate and reap its child: {status:?}"
        );
    }
}
