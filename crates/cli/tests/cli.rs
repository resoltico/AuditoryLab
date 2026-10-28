//! CLI contracts through the shipped executable.
#![cfg(test)]
use auditorylab_core::test_support as common;
#[cfg(unix)]
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
        auditorylab_app::services::builtin_source("chan-2022-clicks-40-hz")
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
    use auditorylab_app::core::{Component, ComponentForm, ComponentId, Ears, Pair};
    let d = dir();
    let file = d.path().join("meaning.toml");
    let tone = |frequency_hz: f64, ears, peak_dbfs: f64| ComponentForm::Tone {
        frequency_hz: finite(frequency_hz),
        ears,
        peak_dbfs: finite(peak_dbfs),
        modulation: None,
    };
    let forms = |voices: Vec<ComponentForm>| voices.into_iter().collect::<Vec<_>>();
    let click = ComponentForm::Clicks {
        rate_hz: finite(40.0),
        width_seconds: finite(0.001),
        ears: Ears::Both,
        peak_dbfs: finite(-40.0),
        modulation: None,
    };
    let pair = Pair {
        first_hz: finite(380.0),
        offset_hz: finite(40.0),
        first_peak_dbfs: finite(-40.0),
        second_peak_dbfs: finite(-41.0),
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
            vec![{
                ComponentForm::BinauralPair {
                    left_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    left_peak_dbfs: pair.first_peak_dbfs,
                    right_peak_dbfs: pair.second_peak_dbfs,
                }
            }],
            "40 Hz interaural difference",
            "monaural",
        ),
        (
            vec![{
                ComponentForm::MonauralPair {
                    ears: Ears::Both,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    first_peak_dbfs: pair.first_peak_dbfs,
                    second_peak_dbfs: pair.second_peak_dbfs,
                }
            }],
            "40 Hz monaural",
            "interaural",
        ),
    ] {
        let mut recipe = common::silent(1.0, 0.0).into_builder();
        recipe.content.sound = auditorylab_app::core::Sound::new(
            recipe.content.sound.duration_seconds(),
            recipe.content.sound.fade_seconds(),
            components
                .into_iter()
                .enumerate()
                .map(|(index, form)| Component {
                    id: ComponentId::numbered(index).unwrap(),
                    form,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
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
    recipe
        .content
        .sound
        .push(auditorylab_app::core::Component {
            id: auditorylab_app::core::ComponentId::new(1).unwrap(),
            form: auditorylab_app::core::ComponentForm::Clicks {
                rate_hz: finite(40.0),
                width_seconds: finite(0.0001),
                ears: auditorylab_app::core::Ears::Both,
                peak_dbfs: finite(-30.0),
                modulation: None,
            },
        })
        .unwrap();
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
        auditorylab_app::core::Recipe::parse(std::str::from_utf8(silence().as_bytes()).unwrap())
            .unwrap()
            .into_builder();
    recipe.content.description = Some(auditorylab_app::core::Description {
        title: "Rīga \"🎧\"".into(),
        notes: Some("Line one\nLine two \u{1b}[31m \u{202e}reverse".into()),
    });
    let recipe = recipe.build().unwrap();
    let text = recipe.to_toml().unwrap();
    fs::write(&path, &text).unwrap();
    assert_eq!(auditorylab_app::core::Recipe::parse(&text).unwrap(), recipe);
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
        "steady_tone",
    ]);
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().trim().is_empty());
    let serialized = cli(&[
        "list",
        "--collection",
        "builtin",
        "--method",
        "steady_tone",
        "--format",
        "json",
    ]);
    assert!(serialized.status.success());
    let serialized: serde_json::Value = serde_json::from_slice(&serialized.stdout).unwrap();
    let entries = serialized["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert!(
        entries
            .iter()
            .all(|entry| entry["nominal"]["methods"] == serde_json::json!(["steady_tone"]))
    );
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
        ["list", "--method", "steady-tone"],
        ["list", "--rhythms", "unknown"],
    ] {
        assert_eq!(cli(&args).status.code(), Some(2));
    }
}

const fn finite(value: f64) -> auditorylab_app::core::Finite {
    match auditorylab_app::core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[test]
fn nonfinite_recipe_diagnostic_names_the_key_line_and_column() {
    for extra in [0, 29, 57, 91] {
        let directory = dir();
        // Different path lengths exercise wrapping through the location phrase.
        let parent = directory.path().join("x".repeat(20 + extra));
        fs::create_dir(&parent).unwrap();
        let path = parent.join("nonfinite.toml");
        fs::write(
            &path,
            include_bytes!("../../core/tests/fixtures/invalid/nonfinite-offset.toml"),
        )
        .unwrap();
        let output = cli(&["inspect", "--file", path.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let message = String::from_utf8(output.stderr).unwrap();
        let words = message.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(words.contains("line 20, column 13"), "{message}");
        assert!(message.contains("offset_hz = nan"), "{message}");
        assert!(words.contains("expected a finite number"), "{message}");
    }
}

#[test]
fn explicit_sort_values_follow_the_cli_contract() {
    assert_eq!(
        "automatic".parse::<auditorylab_app::core::query::SortBy>(),
        Ok(auditorylab_app::core::query::SortBy::Automatic)
    );
    let automatic = cli(&["list", "--collection", "builtin", "--sort", "automatic"]);
    assert_eq!(automatic.status.code(), Some(2));
    let explicit = cli(&["list", "--collection", "builtin", "--sort", "modified"]);
    assert!(explicit.status.success(), "{explicit:?}");
}

#[test]
fn json_failures_keep_fixed_codes_and_structured_values() {
    let root = dir();
    let missing = root.path().join("must-not-read.toml");
    let range = cli(&[
        "inspect",
        "--file",
        missing.to_str().unwrap(),
        "--sample-rate",
        "0",
        "--format",
        "json",
    ]);
    assert_eq!(range.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&range.stdout).unwrap();
    assert_eq!(value["output_schema_version"], 2);
    assert_eq!(value["operation"], "inspect");
    assert_eq!(value["diagnostics"][0]["code"], "sample_rate.range");
    assert_eq!(
        value["diagnostics"][0]["params"],
        serde_json::json!({"hz":0,"min_hz":1,"max_hz":768000})
    );
    assert!(!missing.exists(), "range refusal precedes source access");

    let component = "\n[[sound.component]]\nid = 7\nkind = \"tone\"\nears = \"both\"\nfrequency_hz = 440.0\npeak_dbfs = -40.0\n";
    let duplicate = format!("{}{}{}", silence(), component, component);
    let unsupported = silence().replacen("schema_version = 4", "schema_version = 3", 1);
    for (name, text, code, parameter, expected) in [
        (
            "duplicate.toml",
            duplicate,
            "structure.identity",
            "component_id",
            7,
        ),
        (
            "version.toml",
            unsupported,
            "recipe.schema_version",
            "found",
            3,
        ),
    ] {
        let file = root.path().join(name);
        fs::write(&file, &text).unwrap();
        let result = cli(&[
            "inspect",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ]);
        assert_eq!(result.status.code(), Some(2), "{result:?}");
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let diagnostic = &report["diagnostics"][0];
        assert_eq!(diagnostic["code"], code);
        assert_eq!(diagnostic["params"][parameter], expected);
        assert_eq!(diagnostic["params"]["origin"], file.to_str().unwrap());
        if name == "version.toml" {
            assert_eq!(diagnostic["params"]["supported"], 4);
        }
        assert_eq!(fs::read(&file).unwrap(), text.as_bytes());
    }
}
