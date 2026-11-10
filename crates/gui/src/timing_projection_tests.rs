//! Pair gain meaning through the same model projector and Slint accessibility bindings.
use crate::{adapter_tests::Harness, generated::MainWindow};
use auditorylab_app::{
    core::{
        ChannelRouting, ComponentForm, Modulation, PairKind, PulseShape, PulseSpec, PulseWidth,
        SoundSource,
        test_support::{finite, recipe},
    },
    document::recipe::{Origin, RecipeSnapshot},
    workspace::{Capabilities, Command, EditorCommand},
};
use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
use slint::{ComponentHandle, Model};
use std::ops::ControlFlow;

fn labels(ui: &MainWindow) -> Vec<String> {
    let mut labels = Vec::new();
    let inner = WindowInner::from_pub(ui.window());
    let _ = inner.window_item_rc().unwrap().visit_descendants(|item| {
        if let Some(label) = item.accessible_string_property(AccessibleStringProperty::Label) {
            labels.push(label.to_string());
        }
        ControlFlow::<()>::Continue(())
    });
    labels
}
#[test]
fn binaural_shared_gain_reaches_library_workbench_and_accessible_pattern_captions() {
    assert_pair_projection(PairKind::Binaural);
}
#[test]
fn monaural_shared_gain_reaches_library_workbench_and_accessible_pattern_captions() {
    assert_pair_projection(PairKind::Monaural);
}
fn assert_pair_projection(kind: PairKind) {
    let first_hz = finite(200.0);
    let offset_hz = finite(-7.25);
    let first_peak_dbfs = finite(-10_000.0);
    let second_peak_dbfs = finite(-30.0);
    let source = match kind {
        PairKind::Binaural => SoundSource::BinauralPair {
            first_hz,
            offset_hz,
            first_peak_dbfs,
            second_peak_dbfs,
        },
        PairKind::Monaural => SoundSource::MonauralPair {
            route: ChannelRouting::Right,
            first_hz,
            offset_hz,
            first_peak_dbfs,
            second_peak_dbfs,
        },
    };
    let form = ComponentForm {
        source,
        modulation: Some(Modulation::Pulsed {
            pulse: PulseSpec {
                rate_hz: finite(40.0),
                width: PulseWidth::Duration {
                    seconds: finite(0.001),
                },
                shape: PulseShape::Rectangular {},
            },
        }),
    };
    let snapshot = RecipeSnapshot::new(
        Origin::Stream("shared pair".into()),
        recipe(0.1, 0.0, &[form]),
    );
    let h = Harness::with_catalog(vec![snapshot], Capabilities::no_device_playback());
    let ui = h.ui();
    ui.invoke_collection_filter(3, 0);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 1);
    ui.invoke_select(ui.get_catalog().row_data(0).unwrap().entry);
    ui.set_library_detail(true);
    h.settle();
    let rows = ui.get_model_lanes();
    assert_eq!(rows.row_count(), 1);
    let row = rows.row_data(0).unwrap();
    assert_eq!(row.component, "1");
    assert_eq!(
        row.label,
        "Component 1 · 40 Hz pulsed modulation · applies to both tones"
    );
    assert!(!row.trace.is_empty());
    assert!(
        row.trace.contains(",0.000") && row.trace.contains(",50.000"),
        "shared gain diagram must include open and closed levels, not a flat secondary row"
    );
    assert!(!row.label.contains("steady"));
    let image = format!("{} calculated pattern", row.label);
    assert!(
        labels(ui).contains(&image),
        "Library pattern image must expose shared scope"
    );
    h.adapter
        .command(|workspace| workspace.apply(Command::Editor(EditorCommand::CopySelected)))
        .unwrap()
        .unwrap();
    h.workbench();
    let rows = ui.get_model_lanes();
    assert_eq!(rows.row_count(), 1);
    assert_eq!(rows.row_data(0).unwrap().label, row.label);
    assert!(
        labels(ui).contains(&image),
        "Workbench pattern image must expose shared scope"
    );
    let actual = h
        .adapter
        .inspect(|workspace| workspace.editor().draft().components()[0].form);
    assert_eq!(
        actual, form,
        "projection must preserve both carrier frequencies, levels and routing"
    );
    let header = ui.get_components().row_data(0).unwrap();
    assert!(
        header
            .parameters
            .iter()
            .any(|field| field.label.contains("First carrier") && field.text == "200")
    );
    assert!(header.context.contains("192.75"));
}
