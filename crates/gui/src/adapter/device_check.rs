//! Projection of device-check tests and the opened recording.
use super::projection::sync_rows;
use super::{MainWindow, SlintAdapter, read_only};
use crate::generated::{DeviceAnswerChoice, DeviceTestRow, RecordingFacts};
use auditorylab_presentation::{device_check as check_text, recording as recording_text};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

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
