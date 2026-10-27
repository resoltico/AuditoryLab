//! CLI contracts through the shipped executable.
#![cfg(test)]
#[cfg(unix)]
mod common;
use std::time::{Duration, Instant};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(args)
        .output()
        .unwrap()
}
fn dir() -> tempfile::TempDir {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
const ID: &str = "builtin:chan-2022-clicks-40-hz";
fn silence() -> String {
    common::silent(0.01, 0.0).to_toml().unwrap()
}
#[test]
fn commands_sources_and_exit_codes() {
    for args in [
        vec![],
        vec!["--help"],
        vec!["-V"],
        vec!["list", "--collection", "builtin"],
        vec!["describe", ID],
        vec!["inspect", ID],
        vec!["recipe", ID],
    ] {
        let out = cli(&args);
        assert!(out.status.success(), "{args:?}: {:?}", out);
        assert!(!out.stdout.is_empty());
    }
    for args in [
        vec!["inspect"],
        vec!["inspect", ID, "--stdin"],
        vec!["inspect", "--file", "x", "--stdin"],
        vec!["export", ID],
        vec!["inspect", ID, "--sample-rate", "768001"],
        vec!["--build-info"],
        vec!["unknown"],
    ] {
        assert_eq!(cli(&args).status.code(), Some(2), "{args:?}");
    }
    let unknown = cli(&["inspect", "builtin:unknown"]);
    assert_eq!(unknown.status.code(), Some(2));
    let text = String::from_utf8(unknown.stderr).unwrap();
    assert_eq!(
        text,
        "unknown built-in recipe \"unknown\"; run `auditorylab list`, or use --file PATH\n"
    );
    let list = String::from_utf8(cli(&["list", "--collection", "builtin"]).stdout).unwrap();
    assert!(list.contains("1 h") && list.contains("5 min 7.2 s"));
    assert_eq!(
        cli(&["inspect", "--file", "/missing/auditorylab.toml"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        cli(&["recipe", ID]).stdout,
        auditorylab::recipe::builtin_source("chan-2022-clicks-40-hz")
            .unwrap()
            .as_bytes()
    );
}
#[test]
fn stdin_export_and_no_clobber() {
    let dir = dir();
    let path = dir.path().join("sound.wav");
    for success in [true, false] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .args(["export", "--stdin", "--output"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(silence().as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.success(), success);
        if !success {
            assert_eq!(out.status.code(), Some(1));
        }
    }
    assert_eq!(fs::read(&path).unwrap().len(), 58 + 480 * 8);
    let recipe = dir.path().join("bad.toml");
    fs::write(&recipe, b"invalid").unwrap();
    assert_eq!(
        cli(&["inspect", "--file", recipe.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}
#[test]
fn broken_pipe_is_quiet_success() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["inspect", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Input cannot finish until after the output reader has closed.
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(silence().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
}
#[test]
#[ignore = "plays one second of component-free silence on the default output"]
fn play_reads_stdin_and_reports_native_silent_playback() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["play", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(common::silent(1.0, 0.0).to_toml().unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Played "));
    assert!(out.stdout.is_empty());
    let start = String::from_utf8(out.stderr).unwrap();
    assert!(start.contains("Press Ctrl-C to stop") && start.contains("Turn the volume down"));
}
#[cfg(unix)]
#[test]
fn sigint_cancels_a_long_silent_export_and_removes_the_stage() {
    let dir = dir();
    let input = dir.path().join("silence.toml");
    let output = dir.path().join("long.wav");
    fs::write(
        &input,
        common::silent(1000000.0, 0.0).to_toml().unwrap().as_bytes(),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["export", "--file"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read_dir(dir.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".partial")
    }) {
        assert!(child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(130));
    assert!(!output.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn public_cli_renders_explicit_pairs_and_keeps_raw_tones_independent() {
    use auditorylab::core::{Carrier, Component, ComponentForm, ComponentId, Ears, Pair, Voice};
    let d = dir();
    let file = d.path().join("meaning.toml");
    let tone = |frequency_hz, ears, peak_dbfs| Voice {
        ears,
        peak_dbfs,
        carrier: Carrier::Tone { frequency_hz },
        modulation: None,
    };
    let forms = |voices: Vec<Voice>| {
        voices
            .into_iter()
            .map(ComponentForm::Voice)
            .collect::<Vec<_>>()
    };
    let click = Voice {
        ears: Ears::Both,
        peak_dbfs: -40.0,
        carrier: Carrier::Clicks {
            rate_hz: 40.0,
            width_seconds: 0.001,
        },
        modulation: None,
    };
    let pair = Pair {
        first_hz: 380.0,
        offset_hz: 40.0,
        first_peak_dbfs: -40.0,
        second_peak_dbfs: -41.0,
    };
    for (components, needle, rejected) in [
        (
            forms(vec![click, tone(440.0, Ears::Both, -40.0)]),
            "40 Hz clicks",
            "400 Hz",
        ),
        (
            forms(vec![
                tone(380.0, Ears::Left, -40.0),
                tone(420.0, Ears::Right, -40.0),
            ]),
            "380 Hz steady tone",
            "interaural difference",
        ),
        (
            forms(vec![
                tone(380.0, Ears::Both, -40.0),
                tone(420.0, Ears::Both, -40.0),
            ]),
            "380 Hz steady tone",
            "monaural",
        ),
        (
            forms(vec![
                tone(380.0, Ears::Both, -40.0),
                tone(420.0, Ears::Both, -41.0),
            ]),
            "380 Hz steady tone",
            "monaural",
        ),
        (
            vec![ComponentForm::BinauralPair(pair)],
            "40 Hz interaural difference",
            "monaural",
        ),
        (
            vec![ComponentForm::MonauralPair {
                pair,
                ears: Ears::Both,
            }],
            "40 Hz monaural",
            "interaural",
        ),
    ] {
        let mut recipe = common::silent(1.0, 0.0).into_builder();
        recipe.sound.components = components
            .into_iter()
            .enumerate()
            .map(|(index, form)| Component {
                id: ComponentId::numbered(index).unwrap(),
                form,
            })
            .collect();
        fs::write(&file, recipe.build().unwrap().to_toml().unwrap()).unwrap();
        for command in ["describe", "inspect"] {
            let out = cli(&[command, "--file", file.to_str().unwrap()]);
            assert!(out.status.success(), "{out:?}");
            let text = String::from_utf8(out.stdout).unwrap();
            assert!(text.contains(needle) && !text.contains(rejected), "{text}");
        }
    }
}

#[test]
fn public_analysis_grammar_rejects_partial_and_reports_classifier_revision_two() {
    let rejected = cli(&["list", "--collection", "builtin", "--analysis", "partial"]);
    assert_eq!(rejected.status.code(), Some(2));
    let report = cli(&["list", "--collection", "builtin", "--format", "json"]);
    assert!(report.status.success());
    let value: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(value["classifier_revision"], 2);
}

#[test]
fn inspection_uses_its_selected_rate_throughout_and_states_requested_and_realized_widths() {
    let dir = dir();
    let path = dir.path().join("pulse.toml");
    let mut recipe = common::silent(0.05, 0.0).into_builder();
    recipe.sound.components.push(auditorylab::core::Component {
        id: auditorylab::core::ComponentId::new(1).unwrap(),
        form: auditorylab::core::ComponentForm::Voice(auditorylab::core::Voice {
            ears: auditorylab::core::Ears::Both,
            peak_dbfs: -30.0,
            carrier: auditorylab::core::Carrier::Clicks {
                rate_hz: 40.0,
                width_seconds: 0.0001,
            },
            modulation: None,
        }),
    });
    fs::write(&path, recipe.build().unwrap().to_toml().unwrap()).unwrap();
    let out = cli(&[
        "inspect",
        "--file",
        path.to_str().unwrap(),
        "--sample-rate",
        "8000",
    ]);
    assert!(out.status.success(), "{out:?}");
    // Terminal wrapping may break long lines; compare the words.
    let text = String::from_utf8(out.stdout).unwrap().replace('\n', " ");
    assert!(text.contains("Sample rate: 8000 Hz"), "{text}");
    assert!(text.contains("Frames: 400"), "{text}");
    assert!(
        text.contains("requested 0.1 ms · realized 1 frame (0.125 ms) at 8000 Hz"),
        "{text}"
    );
    assert!(
        text.contains("40 Hz clicks · 0.125 ms"),
        "the model view states the realized width: {text}"
    );
    assert!(
        !text.contains("48000"),
        "no default-rate description leaks into inspection: {text}"
    );
    let invalid = cli(&[
        "inspect",
        "--file",
        path.to_str().unwrap(),
        "--sample-rate",
        "100",
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("at 100 Hz"));
}

#[test]
fn unicode_notes_roundtrip_and_terminal_controls_are_escaped_at_display_boundary() {
    let dir = dir();
    let path = dir.path().join("notes.toml");
    let mut recipe =
        auditorylab::core::Recipe::parse(std::str::from_utf8(silence().as_bytes()).unwrap())
            .unwrap()
            .into_builder();
    recipe.description = Some(auditorylab::core::Description {
        title: "Rīga \"🎧\"".into(),
        notes: Some("Line one\nLine two \u{1b}[31m \u{202e}reverse".into()),
    });
    let recipe = recipe.build().unwrap();
    let text = recipe.to_toml().unwrap();
    fs::write(&path, &text).unwrap();
    assert_eq!(auditorylab::core::Recipe::parse(&text).unwrap(), recipe);
    for command in ["describe", "inspect"] {
        let out = cli(&[command, "--file", path.to_str().unwrap()]);
        assert!(out.status.success(), "{out:?}");
        let display = String::from_utf8(out.stdout).unwrap();
        assert!(
            display.contains("Rīga") && display.contains("🎧"),
            "{display}"
        );
        assert!(
            display.contains("\\u{1b}") && display.contains("\\u{202e}"),
            "{display}"
        );
        assert!(!display.contains('\u{1b}') && !display.contains('\u{202e}'));
    }
}

#[test]
fn nominal_list_filters_use_the_shared_band_and_method_contract() {
    let out = cli(&[
        "list",
        "--collection",
        "builtin",
        "--band",
        "delta",
        "--method",
        "binaural",
    ]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("ioannou-2015-binaural-2-hz"), "{text}");
    assert!(!text.contains("binaural-10-hz"));
    let out = cli(&[
        "list",
        "--collection",
        "builtin",
        "--band",
        "gamma",
        "--method",
        "steady-tone",
    ]);
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().trim().is_empty());
    let search = cli(&["list", "--collection", "builtin", "--search", "202"]);
    assert!(search.status.success());
    let text = String::from_utf8(search.stdout).unwrap();
    assert!(
        text.contains("ioannou-2015-binaural-2-hz")
            && !text.contains("binaural-20-hz")
            && !text.contains("orozco-perez-2020"),
        "{text}"
    );
    let out = cli(&["list", "--collection", "builtin", "--rhythms", "none"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("pure-tone-200-hz") && !text.contains("binaural-2-hz"));
    for args in [
        ["list", "--band", "sleep"],
        ["list", "--method", "unknown"],
        ["list", "--rhythms", "unknown"],
    ] {
        assert_eq!(cli(&args).status.code(), Some(2));
    }
}
