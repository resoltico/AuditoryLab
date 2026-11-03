//! Device-check and recording wording through the public app types.
#![cfg(test)]
use auditorylab_app::device_check::{ChannelResult, DeviceAnswer, DeviceCheck, DeviceTest};
use auditorylab_app::recording::{ContainerFormat, RecordingFile};
use auditorylab_presentation::{device_check, recording, report::Report};

#[test]
fn every_test_prompt_names_its_tone_route_and_question() {
    assert_eq!(
        device_check::test_prompt(DeviceTest::LeftChannel),
        "Plays a 440 Hz tone for 3 seconds at -20 dBFS on the left channel only. Where did you \
        hear it?"
    );
    assert_eq!(
        device_check::test_prompt(DeviceTest::Tone40Hz),
        "Plays a 40 Hz tone for 3 seconds at -20 dBFS on both channels. Did you hear it?"
    );
}

#[test]
fn results_say_what_the_output_means_for_binaural_pairs_and_low_tones() {
    let mut check = DeviceCheck::default();
    let text = |check: &DeviceCheck| device_check::assessment(check.assessment());
    assert!(text(&check).starts_with("Answer both channel tests"));
    check
        .record(DeviceTest::LeftChannel, DeviceAnswer::Both)
        .unwrap();
    check
        .record(DeviceTest::RightChannel, DeviceAnswer::Both)
        .unwrap();
    for test in [DeviceTest::Tone100Hz, DeviceTest::Tone60Hz] {
        check.record(test, DeviceAnswer::Heard).unwrap();
    }
    check
        .record(DeviceTest::Tone40Hz, DeviceAnswer::NotHeard)
        .unwrap();
    assert_eq!(
        text(&check),
        "Both ears hear both channels, as with a single speaker or a mono audio setting. \
        Binaural pairs cannot work on this output: each ear needs its own tone.\nThe lowest \
        tone you heard was 60 Hz. Sounds built on lower tones may be faint or missing on this \
        output. Raising the volume to hear them makes everything else much louder."
    );
    assert!(
        device_check::channel_result(ChannelResult::OneSide(DeviceTest::RightChannel))
            .starts_with("Only the right side made sound. Check the left earpiece")
    );
}

#[test]
fn recordings_render_identity_and_json_without_claiming_measurements() {
    let file = RecordingFile {
        path: "/music/take.flac".into(),
        bytes: 999,
        format: ContainerFormat::Flac,
    };
    assert_eq!(
        recording::recording_description(&file),
        "Recording: take.flac\nFile: /music/take.flac\nSize: 999 bytes\nFormat: FLAC\n\
        Measurements: Auditory Lab identifies the format from the file's first bytes. It does \
        not decode or measure the audio."
    );
    let json = serde_json::to_value(Report::recording(&file)).unwrap();
    assert_eq!(json["operation"], "analyze_recording");
    assert_eq!(
        json["recording"],
        serde_json::json!({"path": "/music/take.flac", "bytes": 999, "format": "flac"})
    );
    let bigger = RecordingFile {
        bytes: 4_398_933,
        ..file
    };
    assert_eq!(recording::recording_size(&bigger), "4.4 MB (4398933 bytes)");
}
