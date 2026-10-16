// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The built binary, run as a user runs it. Nothing here opens an audio device.

#![cfg(test)]

use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const TONE: &str = r#"schema_version = 2
duration_seconds = 0.02
fade_seconds = 0.001
[[component]]
ears = "both"
kind = "tone"
frequency_hz = 440.0
peak_dbfs = -20.0
"#;

struct CatalogEntry {
    name: String,
    text: &'static str,
}

fn catalog_entries() -> Vec<CatalogEntry> {
    auditory_app::catalog()
        .unwrap()
        .iter()
        .map(|record| {
            let auditory_app::Origin::BuiltIn(name) = record.origin() else {
                panic!("catalog record must have a built-in identity");
            };
            CatalogEntry {
                name: name.clone(),
                text: auditory_app::catalog_source(name).unwrap(),
            }
        })
        .collect()
}
fn first_id() -> String {
    catalog_entries().remove(0).name
}
fn title_text(text: &str) -> String {
    text.split_once("\nSource: ").unwrap().0.to_owned() + "\n"
}

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
}

fn run(args: &[&OsStr]) -> Output {
    binary().args(args).output().unwrap()
}

fn export(preset: &Path, output: &Path) -> Output {
    run(&[
        "export".as_ref(),
        "--file".as_ref(),
        preset.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
    ])
}

/// Run with `input` on standard input.
fn piped(args: &[&OsStr], input: &[u8]) -> Output {
    let mut child = binary()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn export_stdin(output: &Path) -> Output {
    piped(
        &[
            "export".as_ref(),
            "--stdin".as_ref(),
            "--output".as_ref(),
            output.as_os_str(),
        ],
        TONE.as_bytes(),
    )
}

/// The channel count and samples of a RIFF file, found by walking its
/// chunks independently of the writer.
fn wav(bytes: &[u8]) -> (u16, Vec<f32>) {
    assert_eq!((&bytes[..4], &bytes[8..12]), (&b"RIFF"[..], &b"WAVE"[..]));
    let mut channels = 0;
    let mut offset = 12;
    loop {
        let length = usize::try_from(u32::from_le_bytes(
            bytes[offset + 4..offset + 8].try_into().unwrap(),
        ))
        .unwrap();
        let body = &bytes[offset + 8..offset + 8 + length];
        match &bytes[offset..offset + 4] {
            b"fmt " => channels = u16::from_le_bytes(body[2..4].try_into().unwrap()),
            b"data" => {
                let samples = body
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|sample| f32::from_le_bytes(*sample))
                    .collect();
                return (channels, samples);
            }
            _ => {}
        }
        offset += 8 + length + length % 2;
    }
}

#[test]
fn engineering_stimuli_export_short_faded_stereo_at_both_rates() {
    let directory = tempfile::tempdir().unwrap();
    let fixtures = [
        TONE.to_owned(),
        TONE.replace(r#"ears = "both""#, r#"ears = "left""#)
            + r#"
[[component]]
ears="right"
kind="tone"
frequency_hz=446
peak_dbfs=-20
"#,
        TONE.to_owned()
            + r#"
[component.modulation]
kind="sinusoidal"
rate_hz=6
depth=0.5
"#,
    ];
    for (index, text) in fixtures.iter().enumerate() {
        for (rate, frames) in [("48000", 960), ("44100", 882)] {
            let inspected = piped(
                &[
                    "inspect".as_ref(),
                    "--stdin".as_ref(),
                    "--sample-rate".as_ref(),
                    rate.as_ref(),
                ],
                text.as_bytes(),
            );
            assert!(inspected.status.success());
            assert!(inspected.stderr.is_empty());
            if index == 0 {
                let fade = if rate == "48000" { 48 } else { 44 };
                let fade_seconds = if rate == "48000" {
                    "0.001".into()
                } else {
                    (44.0_f64 / 44_100.0).to_string()
                };
                let view = String::from_utf8(inspected.stdout).unwrap();
                assert!(view.starts_with("Source: \"standard input\"\n"), "{view}");
                for fact in [
                    format!("Validation: passed at {rate} Hz\nRate selection: explicit\n"),
                    format!("Resolved duration: 0.02 s ({frames} frames)\n"),
                    format!(
                        "Declared fade: 0.001 s\nResolved fade: {fade_seconds} s ({fade} frames per edge)\n"
                    ),
                    "Component 1: tone 440 Hz, peak -20 dBFS, both ears\n".into(),
                    "Peak bound: left ear -20 dBFS, right ear -20 dBFS\n".into(),
                ] {
                    assert!(view.contains(&fact), "missing {fact:?} in {view}");
                }
            }
            let path = directory.path().join(format!("{index}-{rate}.wav"));
            let exported = piped(
                &[
                    "export".as_ref(),
                    "--sample-rate".as_ref(),
                    rate.as_ref(),
                    "--output".as_ref(),
                    path.as_os_str(),
                    "--stdin".as_ref(),
                ],
                text.as_bytes(),
            );
            assert!(
                exported.status.success(),
                "{}",
                String::from_utf8_lossy(&exported.stderr)
            );
            assert_eq!(
                String::from_utf8(exported.stdout).unwrap(),
                format!(
                    "Export completed: \"standard input\" -> {path:?}: {frames} frames at {rate} Hz (RIFF/WAVE)\n"
                )
            );
            let start = String::from_utf8(exported.stderr).unwrap();
            assert!(
                start.starts_with("Starting export\nSource: \"standard input\"\n"),
                "{start}"
            );
            assert!(
                start.contains(&format!(
                    "Duration: 0.02 s\nExport rate: {rate} Hz\nPress Ctrl-C to stop\n"
                )),
                "{start}"
            );
            // Paths can wrap, but the complete native spelling must remain present.
            assert!(
                start
                    .replace("\n             ", "")
                    .contains(&format!("{path:?}")),
                "{start}"
            );
            let (channels, samples) = wav(&fs::read(path).unwrap());
            assert_eq!(channels, 2);
            assert_eq!(samples.len(), frames * 2);
            assert_eq!(samples[0], 0.0);
            assert_eq!(*samples.last().unwrap(), 0.0);
            let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(peak > 0.05 && peak <= 0.1, "{peak}");
        }
    }
}

#[test]
fn export_refuses_an_existing_destination_and_leaves_it_intact() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("kept.wav");
    fs::write(&output, b"existing").unwrap();
    let result = export_stdin(&output);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("the destination already exists and was left untouched")
    );
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn a_relative_output_is_created_in_the_working_directory() {
    let directory = tempfile::tempdir().unwrap();
    let preset = directory.path().join("tone.toml");
    fs::write(&preset, TONE).unwrap();
    let exported = binary()
        .current_dir(directory.path())
        .args([
            "export".as_ref(),
            "--file".as_ref(),
            preset.as_os_str(),
            "--output".as_ref(),
            "relative.wav".as_ref(),
            "--sample-rate".as_ref(),
            OsStr::new("8000"),
        ])
        .output()
        .unwrap();
    assert!(exported.status.success());
    assert_eq!(
        String::from_utf8(exported.stdout).unwrap(),
        format!(
            "Export completed: {preset:?} -> \"relative.wav\": 160 frames at 8000 Hz (RIFF/WAVE)\n"
        )
    );
    assert_eq!(
        wav(&fs::read(directory.path().join("relative.wav")).unwrap())
            .1
            .len(),
        320
    );
}

#[test]
fn a_preset_that_breaks_the_schema_exits_2_before_any_file_or_device_is_touched() {
    let directory = tempfile::tempdir().unwrap();
    let preset = directory.path().join("preset.toml");
    let output = directory.path().join("absent.wav");
    let oversized = format!("{TONE}{}", " ".repeat(65_537 - TONE.len()));
    for bytes in [
        TONE.replace("peak_dbfs = -20.0", "").into_bytes(),
        TONE.replace("schema_version = 2", "schema_version = 1")
            .into_bytes(),
        format!("{TONE}\nunknown_field = 2").into_bytes(),
        oversized.into_bytes(),
        vec![0xff],
    ] {
        fs::write(&preset, bytes).unwrap();
        for result in [
            export(&preset, &output),
            run(&["inspect".as_ref(), "--file".as_ref(), preset.as_os_str()]),
            // Playback reads the preset before it looks for a device.
            run(&["play".as_ref(), "--file".as_ref(), preset.as_os_str()]),
        ] {
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stdout.is_empty());
            let diagnostics = String::from_utf8_lossy(&result.stderr);
            let named = format!("error: {}: preset ", format_args!("{preset:?}"));
            assert!(diagnostics.starts_with(&named), "{diagnostics}");
            assert!(!diagnostics.contains("volume"));
        }
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn a_value_the_core_rejects_exits_2_and_names_its_field() {
    let directory = tempfile::tempdir().unwrap();
    let preset = directory.path().join("preset.toml");
    let output = directory.path().join("absent.wav");
    fs::write(
        &preset,
        TONE.replace("peak_dbfs = -20.0", "peak_dbfs = 2.0"),
    )
    .unwrap();
    for result in [
        export(&preset, &output),
        run(&["inspect".as_ref(), "--file".as_ref(), preset.as_os_str()]),
    ] {
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(
            error.starts_with(&format!("error: {preset:?}: ")),
            "{error}"
        );
        assert!(error.contains("peak_dbfs"), "{error}");
        assert!(!error.contains("cannot be rendered at"));
    }
    let component = TONE.split_once("[[component]]").unwrap().1;
    for (text, field) in [
        (
            format!("{TONE}\n[component.modulation]\nkind=\"sinusoidal\"\nrate_hz=6\ndepth=2\n"),
            "depth",
        ),
        (
            format!(
                "schema_version=2\nduration_seconds=0.02\nfade_seconds=0\n{}",
                format!("[[component]]{component}").repeat(17)
            ),
            "components",
        ),
    ] {
        fs::write(&preset, text).unwrap();
        for result in [
            export(&preset, &output),
            run(&["inspect".as_ref(), "--file".as_ref(), preset.as_os_str()]),
        ] {
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stdout.is_empty());
            let error = String::from_utf8(result.stderr).unwrap();
            assert!(error.starts_with(&format!("error: {preset:?}: ")));
            assert!(error.contains(field), "{error}");
            assert!(!error.contains("cannot be rendered at"));
        }
    }
    // A 30 kHz tone needs a higher sample rate than the default.
    fs::write(
        &preset,
        TONE.replace("frequency_hz = 440.0", "frequency_hz = 30000.0"),
    )
    .unwrap();
    assert_eq!(export(&preset, &output).status.code(), Some(2));
    let at_96k = run(&[
        "inspect".as_ref(),
        "--file".as_ref(),
        preset.as_os_str(),
        "--sample-rate".as_ref(),
        "96000".as_ref(),
    ]);
    assert!(at_96k.status.success());
    assert!(!output.exists());
}

#[test]
fn a_missing_preset_exits_1() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.toml");
    for verb in ["inspect", "play"] {
        let result = run(&[verb.as_ref(), "--file".as_ref(), missing.as_os_str()]);
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .starts_with(&format!(
                    "error: {}: preset cannot be read: ",
                    format_args!("{missing:?}")
                ))
        );
    }
}

#[test]
fn a_preset_of_exactly_the_size_limit_is_read() {
    let directory = tempfile::tempdir().unwrap();
    let preset = directory.path().join("tōne.toml");
    fs::write(
        &preset,
        format!("{TONE}{}", " ".repeat(65_536 - TONE.len())),
    )
    .unwrap();
    assert!(
        run(&["inspect".as_ref(), "--file".as_ref(), preset.as_os_str()])
            .status
            .success()
    );
}

#[test]
fn help_and_version_print_to_stdout_and_usage_errors_exit_2() {
    let help = run(&["--help".as_ref()]);
    assert!(help.status.success() && help.stderr.is_empty());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("auditorylab COMMAND [OPTIONS]")
    );
    let version = run(&["--version".as_ref()]);
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!(
            "{}\n",
            auditory_app::BuildIdentity::for_product(auditory_app::Product::Cli).summary()
        )
    );
    for misuse in [
        &["render".as_ref(), "x.toml".as_ref()][..],
        &[
            "inspect".as_ref(),
            "x.toml".as_ref(),
            "--sample-rate".as_ref(),
            "0".as_ref(),
        ],
        &["inspect".as_ref()],
    ] {
        let misuse = run(misuse);
        assert_eq!(misuse.status.code(), Some(2));
        assert!(misuse.stdout.is_empty());
        let diagnostics = String::from_utf8(misuse.stderr).unwrap();
        assert!(diagnostics.starts_with("error: "));
        assert!(diagnostics.contains("Usage:"));
    }
}

#[test]
fn a_preset_is_read_from_standard_input() {
    let inspected = piped(&["inspect".as_ref(), "--stdin".as_ref()], TONE.as_bytes());
    assert!(inspected.status.success());
    assert!(
        String::from_utf8(inspected.stdout)
            .unwrap()
            .contains("Resolved duration: 0.02 s (960 frames)")
    );
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("piped.wav");
    let exported = piped(
        &[
            "export".as_ref(),
            "--stdin".as_ref(),
            "--output".as_ref(),
            output.as_os_str(),
            "--sample-rate".as_ref(),
            "8000".as_ref(),
        ],
        TONE.as_bytes(),
    );
    assert!(exported.status.success());
    assert_eq!(wav(&fs::read(&output).unwrap()).1.len(), 320);
    let invalid = piped(
        &["play".as_ref(), "--stdin".as_ref()],
        b"schema_version = 1\n",
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid.stderr).unwrap().starts_with(
            "error: \"standard input\": preset has schema_version 1; this build reads 2"
        )
    );
}

#[test]
fn a_closed_stdout_pipe_is_not_a_failure() {
    for flag in ["--help", "--version", "--build-info", "list"] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let result = binary()
            .arg(flag)
            .stdout(Stdio::from(writer))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
    }
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("completed.wav");
    for args in [
        vec!["inspect".as_ref(), "--stdin".as_ref()],
        vec![
            "export".as_ref(),
            "--stdin".as_ref(),
            "--output".as_ref(),
            output.as_os_str(),
        ],
    ] {
        let mut child = binary()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // The preset arrives only after the reading end is certainly closed.
        drop(child.stdout.take());
        child
            .stdin
            .take()
            .unwrap()
            .write_all(TONE.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.stderr.is_empty()
                || String::from_utf8_lossy(&result.stderr).starts_with("Starting export\n")
        );
    }
    assert_eq!(wav(&fs::read(output).unwrap()).1.len(), 1920);
}

#[cfg(target_os = "linux")]
#[test]
fn paths_that_are_not_unicode_reach_the_filesystem_unchanged() {
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    let name = |bytes: &[u8]| {
        directory
            .path()
            .join(std::ffi::OsString::from_vec(bytes.to_vec()))
    };
    let preset = name(b"preset-\xff.toml");
    let output = name(b"output-\xff.wav");
    fs::write(&preset, TONE).unwrap();
    assert!(export(&preset, &output).status.success());
    assert!(output.is_file());
}

/// Behaviour that needs signals.
#[cfg(unix)]
mod on_unix {
    use super::binary;
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    /// Kills the child if a test fails while it is still running.
    struct Running(Child);

    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn signal(name: &str, child: &Child) {
        let sent = Command::new("kill")
            .args([name, &child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success());
    }

    fn exit_status(running: &mut Running) -> ExitStatus {
        let mut status = None;
        wait_until("the program to stop", || {
            status = running.0.try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }

    #[test]
    fn an_interrupt_during_export_exits_130_and_leaves_no_file() {
        // The interrupt arrives within milliseconds; no-component silence keeps the
        // fixture cheap. A completed stereo file would contain 230.4 MB of samples.
        let long = "schema_version=2\nduration_seconds=600\nfade_seconds=0\n";
        for name in ["-INT", "-TERM"] {
            let directory = tempfile::tempdir().unwrap();
            let preset = directory.path().join("long.toml");
            let outputs = directory.path().join("outputs");
            fs::write(&preset, long).unwrap();
            fs::create_dir(&outputs).unwrap();
            let output = outputs.join("long.wav");
            let child = binary()
                .arg("export")
                .arg("--file")
                .arg(&preset)
                .arg("--output")
                .arg(&output)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut running = Running(child);
            // Stage activity occurs only after the interrupt handler is installed.
            wait_until("the owned staging file", || {
                let created = fs::read_dir(&outputs).unwrap().next().is_some();
                if !created {
                    assert!(
                        running.0.try_wait().unwrap().is_none(),
                        "export exited before creating its output file"
                    );
                }
                created
            });
            signal(name, &running.0);
            assert_eq!(exit_status(&mut running).code(), Some(130), "after {name}");
            assert_eq!(fs::read_dir(&outputs).unwrap().count(), 0);
            let mut reported = String::new();
            running
                .0
                .stdout
                .take()
                .unwrap()
                .read_to_string(&mut reported)
                .unwrap();
            assert!(reported.is_empty());
            let mut diagnostics = String::new();
            running
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut diagnostics)
                .unwrap();
            assert!(diagnostics.starts_with("Starting export\n"));
            assert!(diagnostics.ends_with(&format!("Export cancelled: {output:?}\n")));
        }
    }

    #[test]
    fn an_interrupt_while_waiting_for_a_preset_ends_the_program_as_the_system_does() {
        for (name, number) in [("-INT", 2), ("-TERM", 15)] {
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("pending.wav");
            let mut child = binary()
                .args(["export", "--stdin", "--output"])
                .arg(&output)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(b"schema_version = 2\n").unwrap();
            let mut running = Running(child);
            std::thread::sleep(Duration::from_millis(100));
            assert!(running.0.try_wait().unwrap().is_none());
            signal(name, &running.0);
            assert_eq!(exit_status(&mut running).signal(), Some(number));
            assert!(!output.exists());
            drop(stdin);
        }
    }
}

#[test]
fn command_help_is_side_effect_free_and_delimiters_preserve_input_resolution() {
    let directory = tempfile::tempdir().unwrap();
    for verb in ["play", "export", "inspect", "list", "recipe"] {
        for flag in ["-h", "--help"] {
            let result = binary()
                .current_dir(directory.path())
                .args([verb, "absent.toml", "--output", "absent.wav", flag])
                .output()
                .unwrap();
            assert!(result.status.success());
            assert!(result.stderr.is_empty());
            assert!(
                String::from_utf8_lossy(&result.stdout)
                    .contains(&format!("Usage: auditorylab {verb}"))
            );
        }
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    fs::write(directory.path().join("--help"), TONE).unwrap();
    let result = binary()
        .current_dir(directory.path())
        .args(["inspect", "--", "--help"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("no built-in preset is named"));
    fs::write(directory.path().join("-"), TONE).unwrap();
    assert!(
        binary()
            .current_dir(directory.path())
            .args(["inspect", "--file", "./-"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_file(directory.path().join("-")).unwrap();
    let preset = directory.path().join("tone.toml");
    fs::write(&preset, TONE).unwrap();
    let result = binary()
        .current_dir(directory.path())
        .arg("export")
        .arg("--output")
        .arg("-")
        .arg("--file")
        .arg(preset)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        wav(&fs::read(directory.path().join("-")).unwrap()).1.len(),
        1920
    );
}

#[test]
fn malicious_toml_diagnostics_cannot_emit_terminal_controls() {
    let fixture = TONE.replace(
        "[[component]]",
        "[[component]]\n\"\\u001b[2J\\u0085\\u202e\" = 1",
    );
    let result = piped(
        &["inspect".as_ref(), "--stdin".as_ref()],
        fixture.as_bytes(),
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    let diagnostic = String::from_utf8(result.stderr).unwrap();
    assert!(diagnostic.contains("\\u{1b}"), "{diagnostic}");
    assert!(
        diagnostic
            .chars()
            .all(|ch| !ch.is_control() || ch == '\n' || ch == '\t')
    );
    assert!(!diagnostic.contains('\u{202e}'));
}

#[test]
fn quoted_paths_with_controls_still_reach_real_files_and_are_not_double_escaped() {
    let directory = tempfile::tempdir().unwrap();
    let preset = directory.path().join("tōne with spaces.toml");
    fs::write(&preset, TONE).unwrap();
    let result = run(&["inspect".as_ref(), "--file".as_ref(), preset.as_os_str()]);
    assert!(result.status.success());
    let report = String::from_utf8_lossy(&result.stdout);
    let identity = report
        .split_once("Validation: ")
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(identity, format!("Source: {preset:?}"));
    #[cfg(unix)]
    {
        let preset = directory.path().join("tone\n\".toml");
        let destination = directory.path().join("output\n\".wav");
        fs::write(&preset, TONE).unwrap();
        let result = export(&preset, &destination);
        assert!(result.status.success());
        assert!(destination.is_file());
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            format!(
                "Export completed: {preset:?} -> {destination:?}: 960 frames at 48000 Hz (RIFF/WAVE)\n"
            )
        );
    }
}

#[cfg(unix)]
#[test]
fn export_refuses_a_dangling_destination_link_without_replacing_it() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("dangling.wav");
    std::os::unix::fs::symlink("missing-target", &output).unwrap();
    let result = export_stdin(&output);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("the destination already exists and was left untouched")
    );
    assert_eq!(fs::read_link(&output).unwrap(), Path::new("missing-target"));
}

#[test]
fn catalog_listing_recipe_inspection_and_short_exports_use_real_entries() {
    let listed = run(&["list".as_ref()]);
    assert!(listed.status.success());
    assert!(listed.stderr.is_empty());
    let listing = String::from_utf8(listed.stdout).unwrap();
    let rows: Vec<_> = listing
        .lines()
        .filter(|line| !line.starts_with(' '))
        .collect();
    assert_eq!(rows.len(), catalog_entries().len() + 1);
    assert!(rows[0].starts_with("PRESET") && rows[0].ends_with("DURATION"));
    assert_eq!(
        listing
            .lines()
            .filter(|line| line.starts_with("  Sound: "))
            .count(),
        catalog_entries().len()
    );
    let actual: Vec<_> = rows[1..]
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    let expected: Vec<_> = catalog_entries()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(actual, expected);
    let directory = tempfile::tempdir().unwrap();
    for entry in catalog_entries() {
        assert_eq!(
            listing
                .lines()
                .filter(|line| line.split_whitespace().next() == Some(entry.name.as_str()))
                .count(),
            1
        );
        let shown = run(&["recipe".as_ref(), entry.name.as_ref()]);
        assert!(shown.status.success());
        assert!(shown.stderr.is_empty());
        assert_eq!(shown.stdout, entry.text.as_bytes());
        let path = directory.path().join("copy.toml");
        fs::write(&path, &shown.stdout).unwrap();
        for rate in ["48000", "44100"] {
            let named = run(&[
                "inspect".as_ref(),
                entry.name.as_ref(),
                "--sample-rate".as_ref(),
                rate.as_ref(),
            ]);
            let file = run(&[
                "inspect".as_ref(),
                "--file".as_ref(),
                path.as_os_str(),
                "--sample-rate".as_ref(),
                rate.as_ref(),
            ]);
            let stdin = piped(
                &[
                    "inspect".as_ref(),
                    "--stdin".as_ref(),
                    "--sample-rate".as_ref(),
                    rate.as_ref(),
                ],
                entry.text.as_bytes(),
            );
            let expected = auditory_app::load_catalog(&entry.name).unwrap();
            let about = expected.description().about().unwrap();
            let mut semantic = None;
            for (result, origin) in [
                (named, format!("{} (built-in)", entry.name)),
                (file, format!("{path:?}")),
                (stdin, "\"standard input\"".into()),
            ] {
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(result.stderr.is_empty());
                let text = String::from_utf8(result.stdout).unwrap();
                assert!(
                    text.replace("\n        ", "")
                        .contains(&format!("Source: {origin}\n")),
                    "{text}"
                );
                let (_, rest) = text.split_once("Validation: ").unwrap();
                if let Some(reference) = &semantic {
                    assert_eq!(rest, reference);
                } else {
                    semantic = Some(rest.to_owned());
                }
                assert!(
                    title_text(&text)
                        .split_whitespace()
                        .collect::<String>()
                        .contains(&about.title.split_whitespace().collect::<String>())
                );
                assert!(!rest.contains("Summary:"));
                assert!(rest.contains("Peak bound: left ear"));
            }
        }
        let mut short = String::new();
        for line in entry.text.lines() {
            if line.starts_with("duration_seconds =") {
                short.push_str("duration_seconds = 0.02\n");
            } else if line.starts_with("fade_seconds =") {
                short.push_str("fade_seconds = 0.001\n");
            } else {
                short.push_str(line);
                short.push('\n');
            }
        }
        let output = directory.path().join("short.wav");
        let exported = piped(
            &[
                "export".as_ref(),
                "--stdin".as_ref(),
                "--output".as_ref(),
                output.as_os_str(),
            ],
            short.as_bytes(),
        );
        assert!(
            exported.status.success(),
            "{}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let (channels, samples) = wav(&fs::read(&output).unwrap());
        assert_eq!(channels, 2);
        assert_eq!(samples.len(), 1920);
        fs::remove_file(output).unwrap();
    }
}

#[test]
fn dotless_files_require_an_explicit_path_even_after_the_option_delimiter() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("mine"), TONE).unwrap();
    for args in [
        ["inspect", "mine"].as_slice(),
        ["inspect", "--", "mine"].as_slice(),
    ] {
        let result = binary()
            .current_dir(directory.path())
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains("run auditorylab list"));
        assert!(error.contains("--file PATH"));
    }
    assert!(
        binary()
            .current_dir(directory.path())
            .args(["inspect", "--file", "./mine"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn hostile_metadata_is_escaped_in_real_inspection_and_export_start_context() {
    let text = format!(
        "{TONE}\n[about]\ntitle = \"hello\\nexported forged\\t\\r\\u001b[2J\\u202e\"\ncitation = \"authors\"\nsource_url = \"link\"\nstudy = \"role\"\nadaptation = \"limits\"\n"
    );
    let inspected = piped(&["inspect".as_ref(), "--stdin".as_ref()], text.as_bytes());
    assert!(inspected.status.success());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("quoted.wav");
    let exported = piped(
        &[
            "export".as_ref(),
            "--stdin".as_ref(),
            "--output".as_ref(),
            path.as_os_str(),
            "--sample-rate".as_ref(),
            "8000".as_ref(),
        ],
        text.as_bytes(),
    );
    assert!(exported.status.success());
    for bytes in [inspected.stdout, exported.stderr] {
        let out = String::from_utf8(bytes).unwrap();
        assert!(
            out.replace("\n", "")
                .replace(" ", "")
                .contains("hello\\nexportedforged\\t\\r\\u{1b}[2J\\u{202e}"),
            "{out}"
        );
        assert!(!out.contains("\nexported forged"));
        assert!(out.chars().all(|c| !c.is_control() || c == '\n'));
        assert!(!out.contains('\u{202e}'));
    }
    assert!(
        String::from_utf8(exported.stdout)
            .unwrap()
            .starts_with("Export completed: \"standard input\" ->")
    );
}

#[test]
fn explicit_file_selection_preserves_dot_and_backslash_names() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let names = ["tone.toml", "tone\\copy"];
    #[cfg(not(unix))]
    let names = ["tone.toml"];
    for name in names {
        fs::write(directory.path().join(name), TONE).unwrap();
        let inspected = binary()
            .current_dir(directory.path())
            .args(["inspect", "--file", name])
            .output()
            .unwrap();
        assert!(
            inspected.status.success(),
            "{}",
            String::from_utf8_lossy(&inspected.stderr)
        );
        assert!(inspected.stderr.is_empty());
        let text = String::from_utf8(inspected.stdout).unwrap();
        assert!(text.starts_with(&format!("Source: {name:?}\n")));
        assert!(text.contains("Resolved duration: 0.02 s (960 frames)\n"));
    }
}

#[test]
fn recipe_saves_the_exact_entry_to_a_new_file_and_never_replaces_one() {
    let directory = tempfile::tempdir().unwrap();
    let entry = catalog_entries().remove(0);
    let path = directory.path().join("copy.toml");
    let saved = run(&[
        "recipe".as_ref(),
        entry.name.as_ref(),
        "--output".as_ref(),
        path.as_os_str(),
    ]);
    assert!(saved.status.success());
    assert!(saved.stderr.is_empty());
    assert_eq!(
        String::from_utf8(saved.stdout).unwrap(),
        format!("Recipe saved: {} (built-in) -> {path:?}\n", entry.name)
    );
    assert_eq!(fs::read(&path).unwrap(), entry.text.as_bytes());
    fs::write(&path, b"mine").unwrap();
    let refused = run(&[
        "recipe".as_ref(),
        "--output".as_ref(),
        path.as_os_str(),
        entry.name.as_ref(),
    ]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        String::from_utf8(refused.stderr)
            .unwrap()
            .contains("the destination already exists and was left untouched")
    );
    assert_eq!(fs::read(&path).unwrap(), b"mine");
    let unknown = run(&[
        "recipe".as_ref(),
        "missing".as_ref(),
        "--output".as_ref(),
        directory.path().join("missing.toml").as_os_str(),
    ]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn describe_renders_every_catalog_entry_in_plain_wrapped_text() {
    for entry in catalog_entries() {
        let result = run(&["describe".as_ref(), entry.name.as_ref()]);
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).unwrap();
        let loaded = auditory_app::load_catalog(&entry.name).unwrap();
        let about = loaded.description().about().unwrap();
        assert!(text.starts_with(&format!("{}\n", about.title)), "{text}");
        let words = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            words(&text).contains(&words(about.summary.as_deref().unwrap())),
            "{}",
            entry.name
        );
        assert!(
            text.lines().all(|line| line.chars().count() <= 78),
            "{text}"
        );
    }
    let bare = piped(
        &["describe".as_ref(), "--stdin".as_ref()],
        b"schema_version = 2\nduration_seconds = 1\nfade_seconds = 0\n",
    );
    assert!(bare.status.success());
    let bare = String::from_utf8(bare.stdout).unwrap();
    assert!(bare.starts_with("Source: \"standard input\"\nDeclared duration: 1 s\n"));
    assert!(bare.contains("No [about] descriptive metadata is present."));
}

#[test]
fn a_utf16_preset_is_named_as_such() {
    let bytes: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(TONE.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let result = piped(&["inspect".as_ref(), "--stdin".as_ref()], &bytes);
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "error: \"standard input\": preset is UTF-16 text; save it as UTF-8\n"
    );
}

#[test]
fn describe_wraps_the_final_escaped_bytes_even_for_long_words() {
    for title in [
        "x".repeat(79),
        format!("{}\\u001b", "a".repeat(76)),
        format!("{} \\u001b", "a".repeat(72)),
        format!("{}\\u202e", "b".repeat(76)),
        format!("{}\\n\\r\\t\\u0000\\u0085", "c".repeat(76)),
    ] {
        let fixture = format!(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle=\"{title}\"\n"
        );
        let result = piped(
            &["describe".as_ref(), "--stdin".as_ref()],
            fixture.as_bytes(),
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let displayed = title_text(&String::from_utf8(result.stdout).unwrap());
        assert!(
            displayed.lines().all(|line| line.chars().count() <= 78),
            "{displayed}"
        );
        assert!(displayed.chars().all(|ch| !ch.is_control() || ch == '\n'));
        assert!(!displayed.contains('\u{202e}'));
        // Line filling may move whitespace; no source word or escape is lost.
        let expected = title
            .replace("\\u001b", "\\u{1b}")
            .replace("\\u202e", "\\u{202e}")
            .replace("\\u0000", "\\u{0}")
            .replace("\\u0085", "\\u{85}");
        let compact = |text: &str| {
            text.chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
        assert_eq!(compact(&displayed), compact(&expected));
    }
}

#[test]
fn describe_wraps_unicode_columns_without_splitting_graphemes() {
    // Independent fixture oracle: these specific Unicode clusters occupy the
    // stated terminal cells. Do not call either production Unicode helper here.
    let columns = |mut line: &str| {
        let mut width = 0;
        while !line.is_empty() {
            let mut matched = false;
            for (cluster, cells) in [
                ("👩‍👩‍👧‍👦", 2),
                ("👍🏽", 2),
                ("🇱🇻", 2),
                ("e\u{301}", 1),
                (" \u{301}", 1),
                ("界\u{301}", 2),
                ("❤️", 2),
                ("界", 2),
            ] {
                if let Some(rest) = line.strip_prefix(cluster) {
                    width += cells;
                    line = rest;
                    matched = true;
                    break;
                }
            }
            if !matched {
                let first = line.chars().next().unwrap();
                assert!(
                    first.is_ascii() && !first.is_control(),
                    "split cluster: {line:?}"
                );
                width += 1;
                line = &line[first.len_utf8()..];
            }
        }
        width
    };
    for (title, displayed, expected_widths) in [
        ("界".repeat(79), "界".repeat(79), vec![78, 78, 2]),
        ("界\u{301}".repeat(40), "界\u{301}".repeat(40), vec![78, 2]),
        ("❤️".repeat(40), "❤️".repeat(40), vec![78, 2]),
        (
            format!("{} \u{301}界", "x".repeat(77)),
            format!("{} \u{301}界", "x".repeat(77)),
            vec![78, 2],
        ),
        ("e\u{301}".repeat(79), "e\u{301}".repeat(79), vec![78, 1]),
        ("👩‍👩‍👧‍👦".repeat(40), "👩‍👩‍👧‍👦".repeat(40), vec![78, 2]),
        ("👍🏽".repeat(40), "👍🏽".repeat(40), vec![78, 2]),
        ("🇱🇻".repeat(40), "🇱🇻".repeat(40), vec![78, 2]),
        (
            format!("{}👩‍👩‍👧‍👦", "x".repeat(77)),
            format!("{}👩‍👩‍👧‍👦", "x".repeat(77)),
            vec![77, 2],
        ),
        (
            format!("{}\\u001b", "界".repeat(38)),
            format!("{}\\u{{1b}}", "界".repeat(38)),
            vec![78, 4],
        ),
    ] {
        let fixture = format!(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle=\"{title}\"\n"
        );
        let result = piped(
            &["describe".as_ref(), "--stdin".as_ref()],
            fixture.as_bytes(),
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        let text = title_text(&String::from_utf8(result.stdout).unwrap());
        assert_eq!(
            text.lines().map(columns).collect::<Vec<_>>(),
            expected_widths,
            "{text}"
        );
        assert_eq!(text.replace('\n', ""), displayed);
    }
}

#[test]
fn describe_preserves_the_separator_after_a_zero_column_word() {
    for zero_width in ["\u{301}", "\u{200d}"] {
        let fixture = format!(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle=\"{zero_width} alpha\"\n"
        );
        let result = piped(
            &["describe".as_ref(), "--stdin".as_ref()],
            fixture.as_bytes(),
        );
        assert!(result.status.success());
        assert_eq!(
            title_text(&String::from_utf8(result.stdout).unwrap()),
            format!("{zero_width} alpha\n")
        );
    }
}

#[test]
fn describe_escapes_only_an_indivisible_cluster_wider_than_the_line() {
    // A long Devanagari conjunct is one extended grapheme with more than 78
    // columns under the width model; preserving it literally cannot fit a line.
    let title = "क्".repeat(79) + "क";
    let fixture = format!(
        "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle=\"{title}\"\n"
    );
    let result = piped(
        &["describe".as_ref(), "--stdin".as_ref()],
        fixture.as_bytes(),
    );
    assert!(result.status.success());
    let text = title_text(&String::from_utf8(result.stdout).unwrap());
    assert!(text.is_ascii(), "{text}");
    assert!(text.lines().all(|line| line.len() <= 78));
    assert_eq!(
        text.replace('\n', ""),
        title
            .chars()
            .flat_map(char::escape_default)
            .collect::<String>()
    );
}

#[cfg(unix)]
#[test]
fn recipe_refuses_dangling_symlinks_without_changing_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("copy.toml");
    std::os::unix::fs::symlink("missing", &path).unwrap();
    let result = binary()
        .args(["recipe", first_id().as_str(), "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(fs::read_link(&path).unwrap(), Path::new("missing"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("left untouched"));
}

#[cfg(target_os = "macos")]
#[test]
fn recipe_write_failure_reports_real_acl_cleanup_denial() {
    struct AclCleanup<'a>(&'a Path);
    impl Drop for AclCleanup<'_> {
        fn drop(&mut self) {
            assert!(
                Command::new("chmod")
                    .arg("-RN")
                    .arg(self.0)
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("copy.toml");
    for denied in [false, true] {
        let cleanup = AclCleanup(directory.path());
        if denied {
            for rule in [
                "everyone deny delete_child",
                "everyone deny delete,file_inherit",
            ] {
                assert!(
                    Command::new("chmod")
                        .args(["+a", rule])
                        .arg(directory.path())
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
        // The shell passes an ignored SIGXFSZ and zero file-size limit to the real CLI.
        let result = Command::new("/bin/sh")
            .args([
                "-c",
                "trap '' XFSZ; ulimit -f 0; exec \"$@\"",
                "recipe-write-limit",
            ])
            .arg(env!("CARGO_BIN_EXE_auditorylab"))
            .args(["recipe", first_id().as_str(), "--output"])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(diagnostic.contains("File too large"), "{diagnostic}");
        assert!(!path.exists());
        let stages: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(stages.len(), usize::from(denied));
        if denied {
            let stage = &stages[0];
            assert_eq!(fs::metadata(stage).unwrap().len(), 0);
            assert!(
                diagnostic.contains(&format!("staging file remains at {stage:?}")),
                "{diagnostic}"
            );
            assert!(diagnostic.contains("cannot remove it:"));
        } else {
            assert!(!diagnostic.contains("staging file remains"));
        }
        drop(cleanup);
        if denied {
            fs::remove_file(&stages[0]).unwrap();
        }
    }
}

/// Keep stdin open until exit; a parser/help regression must time out rather
/// than accidentally complete after the harness sends EOF. Reap on every path.
fn with_open_stdin(args: &[&OsStr], directory: &Path) -> Output {
    use std::process::Child;
    use std::time::{Duration, Instant};
    struct Reap(Option<Child>);
    impl Drop for Reap {
        fn drop(&mut self) {
            if let Some(child) = &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let child = binary()
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = Reap(Some(child));
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "invocation read stdin or failed to exit: {args:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    child.0.take().unwrap().wait_with_output().unwrap()
}
#[test]
fn discovery_help_and_version_finish_without_reading_an_open_input_pipe() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec![],
        vec!["--help"],
        vec!["-h"],
        vec!["--version"],
        vec!["list"],
        vec!["list", "--"],
    ] {
        let args: Vec<_> = args.iter().map(OsStr::new).collect();
        let result = with_open_stdin(&args, directory.path());
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}
#[test]
fn usage_rejections_finish_before_stdin_or_output_side_effects() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["--"],
        vec!["about", "id"],
        vec!["show", "id"],
        vec!["render", "--help"],
        vec!["play"],
        vec!["export", "--stdin", "--output", "--file=x"],
        vec!["export", "--stdin", "--stdin", "--output=absent.wav"],
        vec!["export", "id", "--stdin", "--output=absent.wav"],
        vec!["export", "--file=x", "--stdin", "--output=absent.wav"],
        vec!["export", "id", "id", "--output=absent.wav"],
        vec!["export", "--stdin", "--output="],
        vec!["export", "--stdin", "--output=a", "--output=b"],
        vec![
            "export",
            "--stdin",
            "--output=absent.wav",
            "--sample-rate=0",
        ],
        vec![
            "export",
            "--stdin",
            "--output=absent.wav",
            "--sample-rate=48000",
            "--sample-rate=44100",
        ],
        vec!["play", "--stdin=yes"],
        vec!["play", "--file="],
        vec!["play", "-hh"],
        vec!["play", "-hfoo"],
        vec!["play", "-h=foo"],
        vec!["play", "--help=foo"],
        vec!["recipe", "--stdin"],
        vec!["recipe", "--file=x"],
        vec!["list", "--", "extra"],
    ] {
        let native: Vec<_> = args.iter().map(OsStr::new).collect();
        let result = with_open_stdin(&native, directory.path());
        assert_eq!(
            result.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(diagnostic.starts_with("error: ") && diagnostic.contains("Usage: auditorylab"));
        assert!(!diagnostic.contains("Starting playback"));
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            0,
            "{args:?}"
        );
    }
}
#[test]
fn help_precedence_and_equals_paths_do_not_confuse_source_selection() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["play", "--file", "--help"],
        vec!["play", "-hfoo", "--help"],
        vec!["export", "--stdin", "--output", "-h"],
    ] {
        let native: Vec<_> = args.iter().map(OsStr::new).collect();
        let result = with_open_stdin(&native, directory.path());
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stdout)
                .starts_with(&format!("Usage: auditorylab {}", args[0]))
        );
    }
    for name in ["--help", "--", "-", "-draft.toml"] {
        fs::write(directory.path().join(name), TONE).unwrap();
        let selector = format!("--file={name}");
        let result = with_open_stdin(&["inspect".as_ref(), selector.as_ref()], directory.path());
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout)
                .contains("Resolved duration: 0.02 s (960 frames)")
        );
    }
    let result = with_open_stdin(
        &["inspect".as_ref(), "--file".as_ref(), "-".as_ref()],
        directory.path(),
    );
    assert!(result.status.success());
    let output_directory = tempfile::tempdir().unwrap();
    let mut child = binary()
        .current_dir(output_directory.path())
        .args(["export", "--stdin", "--output=-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(TONE.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        wav(&fs::read(output_directory.path().join("-")).unwrap())
            .1
            .len(),
        1920
    );
}
#[test]
fn positional_ids_ignore_local_files_and_punctuation_without_implicit_stdin() {
    let directory = tempfile::tempdir().unwrap();
    let id = first_id();
    for name in [id.as_str(), "tone.toml", "mine", "-"] {
        fs::write(directory.path().join(name), TONE).unwrap();
    }
    let catalog = with_open_stdin(&["inspect".as_ref(), id.as_ref()], directory.path());
    assert!(catalog.status.success());
    assert!(String::from_utf8_lossy(&catalog.stdout).contains(&format!("{} (built-in)", id)));
    let local = with_open_stdin(
        &["inspect".as_ref(), "--file".as_ref(), id.as_ref()],
        directory.path(),
    );
    assert!(local.status.success());
    assert!(
        String::from_utf8_lossy(&local.stdout).contains("Resolved duration: 0.02 s (960 frames)")
    );
    for name in ["tone.toml", "./mine", "-"] {
        let result = with_open_stdin(&["inspect".as_ref(), name.as_ref()], directory.path());
        assert_eq!(result.status.code(), Some(2));
        let text = String::from_utf8(result.stderr).unwrap();
        assert!(text.contains("no built-in preset is named") && text.contains("--file PATH"));
    }
}
#[test]
fn an_explicit_default_rate_is_distinct_from_an_omitted_rate() {
    let implicit = piped(&["inspect".as_ref(), "--stdin".as_ref()], TONE.as_bytes());
    let explicit = piped(
        &[
            "inspect".as_ref(),
            "--stdin".as_ref(),
            "--sample-rate=48000".as_ref(),
        ],
        TONE.as_bytes(),
    );
    assert!(implicit.status.success() && explicit.status.success());
    let implicit = String::from_utf8(implicit.stdout).unwrap();
    let explicit = String::from_utf8(explicit.stdout).unwrap();
    assert!(implicit.contains("Rate selection: default"));
    assert!(explicit.contains("Rate selection: explicit"));
    assert_eq!(
        implicit.replace("Rate selection: default", "Rate selection: explicit"),
        explicit
    );
}

#[cfg(any(unix, windows))]
#[test]
fn native_file_and_output_paths_reach_the_filesystem_in_equals_and_separated_forms() {
    use std::ffi::OsString;
    #[cfg(unix)]
    let native = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(b"native-\xff".to_vec())
    };
    #[cfg(windows)]
    let native = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[110, 97, 116, 105, 118, 101, 45, 0xd800])
    };
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join(&native).with_extension("toml");
    if let Err(error) = fs::write(&source, TONE) {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(
                error.raw_os_error(),
                Some(92),
                "unexpected native path rejection: {error}"
            );
            return;
        }
        #[cfg(not(target_os = "macos"))]
        panic!("native input path rejected by filesystem: {error}");
    }
    for attached_file in [false, true] {
        for attached_output in [false, true] {
            let mut suffix = native.clone();
            suffix.push(format!("-{attached_file}-{attached_output}.wav"));
            let destination = directory.path().join(suffix);
            let mut invocation: Vec<OsString> = vec!["export".into()];
            for (name, path, attached) in [
                ("file", &source, attached_file),
                ("output", &destination, attached_output),
            ] {
                if attached {
                    let mut value = OsString::from(format!("--{name}="));
                    value.push(path);
                    invocation.push(value);
                } else {
                    invocation.push(format!("--{name}").into());
                    invocation.push(path.as_os_str().to_owned());
                }
            }
            let result = binary().args(invocation).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(wav(&fs::read(&destination).unwrap()).1.len(), 1920);
        }
    }
}
#[test]
fn hostile_argument_controls_cannot_forge_diagnostic_lines() {
    let directory = tempfile::tempdir().unwrap();
    for args in [
        vec!["inspect", "bad\nforged\tlabel"],
        vec!["inspect", "--bad\nforged"],
        vec!["inspect", "-\n"],
    ] {
        let native: Vec<_> = args.iter().map(OsStr::new).collect();
        let result = with_open_stdin(&native, directory.path());
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(diagnostic.starts_with("error: "), "{diagnostic}");
        assert!(!diagnostic.contains('\t'));
        assert!(
            !diagnostic.lines().any(|line| line.starts_with("forged")),
            "{diagnostic}"
        );
        assert!(diagnostic.contains("\\n"), "{diagnostic}");
    }
}

#[test]
fn unknown_catalog_recovery_respects_recipe_and_custom_source_contexts() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("custom.toml");
    fs::write(&source, TONE).unwrap();
    let destination = directory.path().join("result.wav");
    for output in [None, Some(directory.path().join("recipe.toml"))] {
        let mut command = binary();
        command.args(["recipe", "missing"]);
        if let Some(path) = &output {
            command.arg("--output").arg(path);
        }
        let rejected = command.output().unwrap();
        assert_eq!(rejected.status.code(), Some(2));
        assert!(rejected.stdout.is_empty());
        let hint = String::from_utf8(rejected.stderr).unwrap();
        assert!(hint.contains("run auditorylab list"), "{hint}");
        assert!(
            hint.contains("Usage: auditorylab recipe PRESET_ID"),
            "{hint}"
        );
        assert!(
            !hint.contains("--file"),
            "impossible recipe recovery: {hint}"
        );
        if let Some(path) = output {
            assert!(!path.exists());
        }
    }
    let listed = run(&["list".as_ref()]);
    assert!(listed.status.success());
    let entry = &catalog_entries()[0];
    let recovered = run(&["recipe".as_ref(), entry.name.as_ref()]);
    assert!(recovered.status.success());
    assert_eq!(recovered.stdout, entry.text.as_bytes());
    let rejected = run(&["recipe".as_ref(), "--file".as_ref(), source.as_os_str()]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(
        String::from_utf8(rejected.stderr)
            .unwrap()
            .contains("does not accept --file")
    );

    for command in ["describe", "inspect", "export", "play"] {
        let mut unknown = binary();
        unknown.args([command, "missing"]);
        if command == "export" {
            unknown.arg("--output").arg(&destination);
        }
        let result = unknown.output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        let hint = String::from_utf8(result.stderr).unwrap();
        assert!(hint.contains("--file PATH"), "{command}: {hint}");
        let mut recovery = binary();
        recovery.args([command, "--file"]);
        if command == "play" {
            // Reach strict decoding through the suggested selector without ever
            // discovering an output or starting an audible playback operation.
            let invalid = directory.path().join("invalid.toml");
            fs::write(&invalid, "schema_version=1").unwrap();
            recovery.arg(&invalid);
            let decoded = recovery.output().unwrap();
            assert_eq!(decoded.status.code(), Some(2));
            let diagnostic = String::from_utf8(decoded.stderr).unwrap();
            assert!(
                diagnostic.contains("schema") && diagnostic.contains("invalid.toml"),
                "{diagnostic}"
            );
            assert!(!diagnostic.contains("does not accept"));
        } else {
            recovery.arg(&source);
            if command == "export" {
                recovery.arg("--output").arg(&destination);
            }
            let recovered = recovery.output().unwrap();
            assert!(
                recovered.status.success(),
                "{command}: {}",
                String::from_utf8_lossy(&recovered.stderr)
            );
        }
    }
}

#[test]
fn build_info_is_standalone_exact_identity_and_requires_no_input_or_device() {
    let directory = tempfile::tempdir().unwrap();
    let result = with_open_stdin(&["--build-info".as_ref()], directory.path());
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert_eq!(
        result.stdout,
        format!(
            "{}\n",
            auditory_app::BuildIdentity::for_product(auditory_app::Product::Cli).to_json()
        )
        .as_bytes()
    );
    for args in [
        vec!["--build-info", "extra"],
        vec!["--build-info=yes"],
        vec!["list", "--build-info"],
    ] {
        let result = run(&args.iter().map(OsStr::new).collect::<Vec<_>>());
        assert_eq!(result.status.code(), Some(2));
    }
}
