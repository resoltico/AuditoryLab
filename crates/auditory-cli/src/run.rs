// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::args::{Command, InputSource, help};
use crate::failure::{Failure, INTERRUPTED};
use crate::report;
use auditory_app::{self as app, Cancellation, LoadedPreset};
use ctrlc::set_handler;
use std::io::{self, IsTerminal, Write};
use std::process::exit;

pub(crate) fn run(command: Command) -> Result<(), Failure> {
    run_with(
        command,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        cancel_on_interrupt,
    )
}
fn run_with(
    command: Command,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    watch_interrupts: impl FnOnce() -> Result<Cancellation, Failure>,
) -> Result<(), Failure> {
    match command {
        Command::Help(topic) => output(stdout, &format!("{}\n", help(topic))),
        Command::Version => output(
            stdout,
            &format!(
                "{}\n",
                app::BuildIdentity::for_product(app::Product::Cli).summary()
            ),
        ),
        Command::BuildInfo => raw_output(
            stdout,
            format!(
                "{}\n",
                app::BuildIdentity::for_product(app::Product::Cli).to_json()
            )
            .as_bytes(),
        ),
        Command::List => output(stdout, &report::listing(&app::catalog()?)),
        Command::Recipe { name, output: None } => raw_output(
            stdout,
            app::catalog_source(name)
                .map_err(|error| Failure::for_command(error, crate::args::HelpTopic::Recipe))?
                .as_bytes(),
        ),
        Command::Recipe {
            name,
            output: Some(path),
        } => {
            let completed = app::save_recipe(&name, &path, &Cancellation::default())
                .map_err(|error| Failure::for_command(error, crate::args::HelpTopic::Recipe))?;
            report_completed(stdout, stderr, &report::saved(&completed));
            Ok(())
        }
        Command::Describe { preset } => output(
            stdout,
            &report::describe(read(&preset, stderr)?.description()),
        ),
        Command::Inspect {
            preset,
            sample_rate,
            rate_selection,
        } => {
            let facts = read(&preset, stderr)?.inspect(sample_rate)?;
            output(stdout, &report::resolved(&facts, rate_selection))
        }
        Command::Export {
            preset,
            output: path,
            sample_rate,
            rate_selection: _,
        } => {
            let source = read(&preset, stderr)?;
            let prepared = source.prepare_export(sample_rate, &path)?;
            let cancel = watch_interrupts()?;
            notify(stderr, &report::export_start(prepared.view()));
            let completed = prepared.execute(&cancel)?;
            report_completed(stdout, stderr, &report::exported(&completed));
            Ok(())
        }
        Command::Play { preset } => {
            let source = read(&preset, stderr)?;
            let cancel = watch_interrupts()?;
            let prepared = source.prepare_playback()?;
            notify(stderr, &report::playback_start(prepared.view()));
            let completed = prepared.execute(&cancel)?;
            report_completed(stdout, stderr, &report::played(&completed));
            for notice in completed.notices() {
                notify(stderr, &report::notice(*notice));
            }
            Ok(())
        }
    }
}

fn read(input: &InputSource, stderr: &mut impl Write) -> Result<LoadedPreset, Failure> {
    Ok(match input {
        InputSource::Catalog(name) => app::load_catalog(name)?,
        InputSource::File(path) => app::load_file(path)?,
        InputSource::Stdin => {
            let stdin = io::stdin();
            stdin_cue(stderr, stdin.is_terminal());
            app::load_reader(stdin.lock(), app::Origin::Reader("standard input".into()))?
        }
    })
}

/// Write and flush; a closed pipe is an expected way for the reader to stop.
pub(crate) fn output(stdout: &mut impl Write, text: &str) -> Result<(), Failure> {
    write_bytes(stdout, terminal_text(text).as_bytes())
}
/// Exact trusted catalog source, with no human-text transformation.
fn raw_output(stdout: &mut impl Write, bytes: &[u8]) -> Result<(), Failure> {
    write_bytes(stdout, bytes)
}
fn write_bytes(stdout: &mut impl Write, bytes: &[u8]) -> Result<(), Failure> {
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
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

/// Escape a value embedded within one human output line.
pub(crate) fn inline_text(text: &str) -> String {
    terminal_text(text)
        .replace('\n', "\\n")
        .replace('\t', "\\t")
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
fn cancel_on_interrupt() -> Result<Cancellation, Failure> {
    let cancel = Cancellation::default();
    let requested = cancel.clone();
    set_handler(move || {
        if requested.request() {
            exit(i32::from(INTERRUPTED));
        }
    })
    .map_err(Failure::Interrupts)?;
    Ok(cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn output_failure_and_broken_pipe_preserve_completed_effects() {
        for on_flush in [false, true] {
            for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("recipe.toml");
                let name = match app::catalog().unwrap()[0].origin() {
                    app::Origin::BuiltIn(name) => name.clone(),
                    _ => unreachable!(),
                };
                let mut warning = Vec::new();
                run_with(
                    Command::Recipe {
                        name: name.clone().into(),
                        output: Some(path.clone()),
                    },
                    &mut Failing { kind, on_flush },
                    &mut warning,
                    || panic!("save must not install signal handler"),
                )
                .unwrap();
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    app::catalog_source(&name).unwrap().as_bytes()
                );
                assert_eq!(warning.is_empty(), kind == io::ErrorKind::BrokenPipe);
                let result = run_with(
                    Command::Recipe {
                        name: name.clone().into(),
                        output: None,
                    },
                    &mut Failing { kind, on_flush },
                    &mut Vec::new(),
                    || panic!("source output must not install signal handler"),
                );
                assert_eq!(result.is_ok(), kind == io::ErrorKind::BrokenPipe);
            }
        }
    }
    #[test]
    fn stderr_failure_and_completed_export_reporting_failure_do_not_reverse_export() {
        for on_flush in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("input.toml");
            std::fs::write(
                &source,
                b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
            )
            .unwrap();
            let destination = dir.path().join("output.wav");
            run_with(
                Command::Export {
                    preset: InputSource::File(source.clone()),
                    output: destination.clone(),
                    sample_rate: app::SampleRate::DEFAULT,
                    rate_selection: crate::args::RateSelection::Default,
                },
                &mut Failing {
                    kind: io::ErrorKind::Other,
                    on_flush,
                },
                &mut Failing {
                    kind: io::ErrorKind::Other,
                    on_flush,
                },
                || {
                    std::fs::remove_file(&source).unwrap();
                    Ok(Cancellation::default())
                },
            )
            .unwrap();
            assert_eq!(std::fs::metadata(destination).unwrap().len(), 58 + 480 * 8);
        }
    }
    #[test]
    fn stdin_cue_and_notices_are_best_effort() {
        let mut bytes = Vec::new();
        stdin_cue(&mut bytes, false);
        assert!(bytes.is_empty());
        stdin_cue(&mut bytes, true);
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains("stdin until EOF")
        );
        stdin_cue(
            &mut Failing {
                kind: io::ErrorKind::Other,
                on_flush: false,
            },
            true,
        );
    }
    #[test]
    fn terminal_controls_bidi_and_quotes_are_escaped_once() {
        assert_eq!(
            terminal_text("plain\n\tΩ\u{1b}[2J\r\u{0085}\u{202e}"),
            "plain\n\tΩ\\u{1b}[2J\\r\\u{85}\\u{202e}"
        );
        assert_eq!(terminal_text("\\u{1b}"), "\\u{1b}");
        for entry in app::catalog().unwrap() {
            let app::Origin::BuiltIn(name) = entry.origin() else {
                panic!("catalog source must be built-in");
            };
            let source = app::catalog_source(name).unwrap();
            assert_eq!(terminal_text(source), source);
        }
    }
    #[test]
    fn raw_and_human_writers_have_distinct_control_contracts() {
        let fixture = "ESC\u{1b} bidi\u{202e} CR\r TAB\t LF\n";
        let mut raw = Vec::new();
        raw_output(&mut raw, fixture.as_bytes()).unwrap();
        assert_eq!(raw, fixture.as_bytes());
        let mut human = Vec::new();
        output(&mut human, fixture).unwrap();
        assert_eq!(human, b"ESC\\u{1b} bidi\\u{202e} CR\\r TAB\t LF\n");
        assert_ne!(human, raw);
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::Other] {
            for on_flush in [false, true] {
                assert_eq!(
                    raw_output(&mut Failing { kind, on_flush }, fixture.as_bytes()).is_ok(),
                    kind == io::ErrorKind::BrokenPipe
                );
            }
        }
    }
    #[test]
    fn interrupt_handler_probe() {
        let Some(directory) = std::env::var_os("AUDITORYLAB_SIGNAL_PROBE") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        let cancel = cancel_on_interrupt().unwrap();
        std::fs::write(directory.join("ready"), b"ready").unwrap();
        while !cancel.is_requested() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::fs::write(directory.join("ack"), b"cancel acknowledged").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    #[cfg(unix)]
    #[test]
    fn second_native_interrupt_forces_exit_after_first_acknowledgement() {
        use std::{
            process::{Child, Command, Stdio},
            time::{Duration, Instant},
        };
        struct Reap(Child);
        impl Drop for Reap {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "run::tests::interrupt_handler_probe",
                "--nocapture",
            ])
            .env("AUDITORYLAB_SIGNAL_PROBE", directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = Reap(child);
        let wait = |file: &str, child: &mut Child| {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !directory.path().join(file).exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "probe exited before {file}"
                );
                assert!(Instant::now() < deadline, "no {file} acknowledgement");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        wait("ready", &mut child.0);
        assert!(
            Command::new("kill")
                .args(["-INT", &child.0.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        wait("ack", &mut child.0);
        assert!(
            Command::new("kill")
                .args(["-TERM", &child.0.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert_eq!(status.code(), Some(130));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "second interrupt did not force exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[cfg(windows)]
    #[test]
    fn second_console_interrupt_forces_exit_after_first_acknowledgement() {
        let directory = tempfile::tempdir().unwrap();
        let script = r#"
import ctypes, os, pathlib, subprocess, sys, time
from ctypes import wintypes
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
HANDLER=ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.DWORD)
handler=HANDLER(lambda event: True)
null_handler=HANDLER()
for name in ('FreeConsole','AllocConsole'):
    getattr(kernel,name).argtypes=[]
    getattr(kernel,name).restype=wintypes.BOOL
kernel.SetConsoleCtrlHandler.argtypes=[HANDLER,wintypes.BOOL]
kernel.SetConsoleCtrlHandler.restype=wintypes.BOOL
kernel.GenerateConsoleCtrlEvent.argtypes=[wintypes.DWORD,wintypes.DWORD]
kernel.GenerateConsoleCtrlEvent.restype=wintypes.BOOL
directory=pathlib.Path(sys.argv[2])
process=None
console_owned=False
phase='initialization'
def enter(name):
    global phase
    phase=name
    print('console probe: '+name,flush=True)
def checked(result):
    if not result:
        raise ctypes.WinError(ctypes.get_last_error())
def wait_file(name):
    enter('waiting for '+name)
    deadline=time.monotonic()+20
    while not (directory/name).exists():
        assert process.poll() is None, 'probe exited before '+name
        assert time.monotonic()<deadline, 'timeout before '+name
        time.sleep(0.005)
try:
    enter('allocating private console')
    kernel.FreeConsole()
    checked(kernel.AllocConsole())
    console_owned=True
    # Establish a known inherited-ignore state, then explicitly normalize it.
    # Registering a non-NULL handler alone does not enable Ctrl+C processing.
    enter('normalizing inherited Ctrl+C processing')
    checked(kernel.SetConsoleCtrlHandler(null_handler,True))
    checked(kernel.SetConsoleCtrlHandler(null_handler,False))
    checked(kernel.SetConsoleCtrlHandler(handler,True))
    enter('spawning probe in the private console')
    process=subprocess.Popen([sys.argv[1],'--exact','run::tests::interrupt_handler_probe','--nocapture'],env=os.environ.copy()|{'AUDITORYLAB_SIGNAL_PROBE':str(directory)},stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    wait_file('ready')
    enter('sending first Ctrl+C')
    checked(kernel.GenerateConsoleCtrlEvent(0,0))
    wait_file('ack')
    enter('sending second Ctrl+Break')
    checked(kernel.GenerateConsoleCtrlEvent(1,0))
    stdout,stderr=process.communicate(timeout=20)
    assert process.returncode==130,(process.returncode,stdout,stderr)
    enter('observed cancellation acknowledgement and forced exit 130')
except BaseException:
    print('console probe failed during '+phase,file=sys.stderr,flush=True)
    raise
finally:
    try:
        if process is not None:
            if process.poll() is None:
                process.kill()
            stdout,stderr=process.communicate(timeout=20)
            print('console probe child: exit='+str(process.returncode)+' stdout='+repr(stdout)+' stderr='+repr(stderr),file=sys.stderr,flush=True)
    finally:
        if console_owned and not kernel.FreeConsole():
            print('console probe cleanup: '+str(ctypes.WinError(ctypes.get_last_error())),file=sys.stderr,flush=True)

"#;
        let result = std::process::Command::new("python")
            .args(["-c", script])
            .arg(std::env::current_exe().unwrap())
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
