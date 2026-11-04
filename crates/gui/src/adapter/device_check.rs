//! Projection of device-check tests and the opened recording.
use super::projection::sync_rows;
use super::{MainWindow, SlintAdapter, read_only};
use crate::generated::{DeviceAnswerChoice, DeviceTestRow, RecordingFacts};
use auditorylab_presentation::{device_check as check_text, recording as recording_text};
use slint::{Model, ModelRc, VecModel};
use std::{fmt::Write, rc::Rc};

impl SlintAdapter {
    pub(super) fn project_device_check(&self, ui: &MainWindow) {
        let workspace = self.workspace.borrow();
        let Some(workspace) = workspace.as_ref() else {
            return;
        };
        let view = workspace.device_check_view();
        let rows: Vec<DeviceTestRow> = view
            .tests
            .iter()
            .map(|test| DeviceTestRow {
                key: test.test.key().into(),
                label: test.test.label().into(),
                prompt: read_only(&check_text::test_prompt(test.test)),
                selected: test.selected,
                answers: ModelRc::new(VecModel::from(
                    test.test
                        .answers()
                        .iter()
                        .map(|answer| DeviceAnswerChoice {
                            key: answer.key().into(),
                            label: answer.label().into(),
                            selected: test.answer == Some(*answer),
                        })
                        .collect::<Vec<_>>(),
                )),
            })
            .collect();
        let model = ui.get_device_tests();
        if let Some(model) = model.as_any().downcast_ref::<VecModel<DeviceTestRow>>() {
            sync_rows(model, rows);
        } else {
            ui.set_device_tests(Rc::new(VecModel::from(rows)).into());
        }
        ui.set_device_check_introduction(check_text::INTRODUCTION.into());
        ui.set_device_channel_result(read_only(&check_text::channel_result(
            view.assessment.channels,
        )));
        ui.set_device_low_tone_result(read_only(&check_text::low_tone_result(
            view.assessment.low_tones,
        )));
        ui.set_recording_measurements(recording_text::MEASUREMENTS.into());
        ui.set_analyzing_recording(workspace.analyzing_recording());
        ui.set_recording_progress(f32_value(workspace.recording_progress()));
        ui.set_has_recording_analysis(workspace.recording_analysis().is_some());
        if let Some(recording) = workspace.recording_analysis() {
            use auditorylab_app::device_check::{DeviceTest, LowToneResult};
            let lowest = match view.assessment.low_tones {
                LowToneResult::AllHeard => Some(DeviceTest::Tone40Hz.frequency_hz()),
                LowToneResult::LowestHeard { test } => Some(test.frequency_hz()),
                _ => None,
            };
            let f = recording_text::findings(&recording.analysis, lowest);
            ui.set_recording_findings(crate::generated::RecordingFindings {
                facts: read_only(&f.facts),
                summary: read_only(&f.summary),
                tones: read_only(&f.tones),
                ears: read_only(&f.ears),
                pulsing: read_only(&f.pulsing),
                low_tones: read_only(&f.low_tones),
                stereo: read_only(&f.stereo),
                limits: read_only(&f.limits),
                left_trace: spectrum_path(recording.analysis.spectrum.first()).into(),
                right_trace: spectrum_path(recording.analysis.spectrum.get(1)).into(),
            });
            let bars = recording
                .analysis
                .low_energy
                .iter()
                .filter(|share| [50.0, 60.0, 100.0, 300.0].contains(&share.below_hz))
                .map(|share| crate::generated::RecordingEnergy {
                    label: format!(
                        "{} · below {:.0} Hz",
                        if recording.analysis.channels == 1 {
                            "Mono"
                        } else if share.channel == 0 {
                            "Left"
                        } else {
                            "Right"
                        },
                        share.below_hz
                    )
                    .into(),
                    fraction: f32_value(share.fraction.unwrap_or(0.0)),
                })
                .collect::<Vec<_>>();
            ui.set_recording_energy(ModelRc::new(VecModel::from(bars)));
        }
        let recording = workspace.recording();
        ui.set_has_recording(recording.is_some());
        if let Some(recording) = recording {
            ui.set_recording(RecordingFacts {
                name: read_only(&recording_text::recording_name(recording)),
                path: read_only(&recording.path.display().to_string()),
                size: read_only(&recording_text::recording_size(recording)),
                format: recording.format.label().into(),
            });
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounded normalized native drawing coordinates"
)]
fn f32_value(value: f64) -> f32 {
    value as f32
}
fn spectrum_path(points: Option<&Vec<auditorylab_app::recording::SpectrumPoint>>) -> String {
    let Some(points) = points else {
        return String::new();
    };
    let mut path = String::new();
    for (index, point) in points.iter().enumerate() {
        let x = (point.hz / 20.0).log10() / 2.0 * 1000.0;
        let y = (-point.dbfs_per_hz).clamp(0.0, 160.0);
        let _ = write!(
            path,
            "{} {x:.2} {y:.2} ",
            if index == 0 { "M" } else { "L" }
        );
    }
    path
}
