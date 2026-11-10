//! Completed metadata is projected from retained app findings, never current edit fields.
use super::*;

fn silence_file(path: &std::path::Path, rate: u32) {
    use std::io::Write;
    let bytes = rate * 6 * 2;
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
    file.write_all(&rate.to_le_bytes()).unwrap();
    file.write_all(&(rate * 2).to_le_bytes()).unwrap();
    file.write_all(b"\x02\0\x10\0data").unwrap();
    file.write_all(&bytes.to_le_bytes()).unwrap();
    file.set_len(u64::from(bytes) + 44).unwrap();
}
fn open(h: &Harness, path: &std::path::Path) {
    h.ui().invoke_open_recording();
    let [Request::Path(prompt, PathPurpose::OpenRecording, None)] = h.take()[..] else {
        panic!("actual production open request")
    };
    h.adapter
        .respond(prompt, PromptAnswer::Path(Some(path.into())));
    h.wait_for_files();
    h.settle();
}
#[test]
fn nonzero_facts_survive_cancel_and_clear_for_a_different_recording() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let first = dir.path().join("first.wav");
    let second = dir.path().join("second.wav");
    silence_file(&first, 1000);
    silence_file(&second, 2000);
    let h = Harness::new();
    h.ui().invoke_tab_changed(3);
    open(&h, &first);
    h.ui().invoke_analyze_recording("3".into(), "2".into());
    h.wait_for_files();
    h.settle();
    let completed = h.ui().get_recording_findings();
    assert!(completed.facts.contains("Analyzed: 3.0–5.0 s"));
    assert!(completed.facts.contains("Envelope window: 3.1–4.9 s"));
    assert_eq!(completed.facts.matches("Analyzed:").count(), 1);
    assert!(!completed.limits.contains("Window 0."));
    let item = reveal_scrolled_control(&h, completed.facts.as_str(), "Recording content");
    assert_eq!(
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref(),
        Some(completed.facts.as_str())
    );
    h.ui().invoke_analyze_recording("0".into(), "4".into());
    assert!(
        h.adapter
            .inspect(|workspace| workspace.analyzing_recording())
    );
    assert_eq!(h.ui().get_recording_findings().facts, completed.facts);
    h.ui().invoke_cancel_recording_analysis();
    h.wait_for_files();
    h.settle();
    assert_eq!(h.ui().get_recording_findings(), completed);
    assert_eq!(
        h.adapter.inspect(|workspace| workspace
            .recording_analysis()
            .unwrap()
            .analysis
            .window
            .start_seconds),
        3.0
    );
    open(&h, &second);
    assert!(!h.ui().get_has_recording_analysis());
    assert_eq!(h.ui().get_recording().name, "second.wav");
    h.ui().invoke_analyze_recording("0".into(), "2".into());
    h.wait_for_files();
    h.settle();
    let current = h.ui().get_recording_findings();
    assert!(current.facts.contains("2000 Hz"));
    assert!(current.facts.contains("Analyzed: 0.0–2.0 s"));
    assert!(!current.facts.contains("Analyzed: 3.0–5.0 s"));
}
