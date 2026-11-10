//! Projection of device-check tests and the opened recording.
use super::projection::sync_rows;
use super::{MainWindow, SlintAdapter, read_only};
use crate::generated::{DeviceAnswerChoice, DeviceTestRow, RecordingFacts};
use auditorylab_presentation::{device_check as check_text, recording as recording_text};
use slint::{Model, ModelRc, VecModel};
use std::{fmt::Write, rc::Rc};

impl SlintAdapter {
    pub(super) fn project_device_check(
        &self,
        ui: &MainWindow,
        actions: &auditorylab_app::workspace::WorkspaceActions,
    ) {
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
        ui.set_recording_measurements(read_only(&recording_text::measurement_guidance(
            workspace.recording(),
            workspace.recording_analysis(),
            workspace.recording_phase(),
            actions,
        )));
        ui.set_analyzing_recording(workspace.analyzing_recording());
        ui.set_recording_progress(f32_value(workspace.recording_progress()));
        ui.set_recording_progress_phase(
            workspace
                .recording_phase()
                .map_or("", auditorylab_presentation::recording::progress_phase)
                .into(),
        );
        ui.set_recording_progress_indeterminate(
            workspace
                .recording_phase()
                .is_some_and(|phase| !phase.determinate()),
        );
        ui.set_has_recording_analysis(workspace.recording_analysis().is_some());
        if let Some(recording) = workspace.recording_analysis() {
            let context = auditorylab_app::recording::InterpretationContext::from(view.assessment);
            Self::project_recording_analysis(ui, &recording.analysis, context);
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

    pub(super) fn project_recording_analysis(
        ui: &MainWindow,
        analysis: &auditorylab_app::recording::Analysis,
        context: auditorylab_app::recording::InterpretationContext,
    ) {
        let f = recording_text::contextual_findings(analysis, context);
        ui.set_recording_findings(crate::generated::RecordingFindings {
            facts: read_only(&f.facts),
            summary: read_only(&f.summary),
            tones: read_only(&f.tones),
            ears: read_only(&f.ears),
            pulsing: read_only(&f.pulsing),
            low_tones: read_only(&f.low_tones),
            stereo: read_only(&f.stereo),
            limits: read_only(&f.limits),
            left_trace: spectrum_path(analysis.spectrum.first()).into(),
            right_trace: spectrum_path(analysis.spectrum.get(1)).into(),
        });
        let bars = recording_text::energy_rows(analysis.samples())
            .map(|share| crate::generated::RecordingEnergy {
                label: recording_text::energy_row(share, analysis.channels).into(),
                defined: share.fraction.is_some(),
                fraction: f32_value(share.fraction.unwrap_or(0.0)),
            })
            .collect::<Vec<_>>();
        ui.set_recording_energy(ModelRc::new(VecModel::from(bars)));
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounded normalized native drawing coordinates"
)]
fn f32_value(value: f64) -> f32 {
    value as f32
}
fn spectrum_path(
    points: Option<&Vec<auditorylab_app::core::measurement::SpectrumPoint>>,
) -> String {
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
