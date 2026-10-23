// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use auditorylab_core::Carrier;

fn editor_with(components: &[Component]) -> EditorSession {
    let mut draft = Draft::new();
    for component in components {
        let id = draft.add(CarrierKind::Tone).unwrap();
        draft.replace(id, *component);
    }
    EditorSession::new(draft).unwrap()
}

fn tone(frequency_hz: f64) -> Component {
    Component {
        ears: Ears::Both,
        peak_dbfs: -30.0,
        carrier: Carrier::Tone { frequency_hz },
        modulation: None,
    }
}

fn field(editor: &EditorSession, index: usize, parameter: ParameterId) -> FieldHandle {
    editor.handle(EditingField::Parameter(
        editor.draft().components()[index].0,
        parameter,
    ))
}

fn metadata(editor: &EditorSession, field: MetadataField) -> FieldHandle {
    editor.handle(EditingField::Metadata(field))
}

#[test]
fn staging_changes_diagnostics_but_never_committed_values() {
    let mut editor = editor_with(&[tone(440.0)]);
    let frequency = field(&editor, 0, ParameterId::ToneFrequency);
    let committed = editor.committed_analysis(SampleRate::DEFAULT);
    let draft_revision = editor.draft_revision();
    editor.stage(frequency, "880").unwrap();
    editor.stage(frequency, "88x").unwrap();
    editor
        .stage(metadata(&editor, MetadataField::Title), "Renamed")
        .unwrap();
    assert_eq!(
        editor.text(frequency.field),
        "88x",
        "raw text survives exactly"
    );
    assert_eq!(editor.draft().value(frequency.field), Some(440.0));
    assert_eq!(editor.draft().title(), "Untitled sound");
    assert_eq!(editor.draft_revision(), draft_revision);
    assert!(editor.dirty());
    let candidate = editor.candidate();
    assert_eq!(
        candidate.problems(),
        [(frequency.field, InputProblem::Unparseable)]
    );
    assert!(
        Arc::ptr_eq(&committed, &editor.committed_analysis(SampleRate::DEFAULT)),
        "committed evaluation is reused unchanged"
    );
}

#[test]
fn commit_is_atomic_over_the_whole_candidate() {
    let mut editor = editor_with(&[tone(440.0), tone(550.0)]);
    let a = field(&editor, 0, ParameterId::ToneFrequency);
    let b = field(&editor, 1, ParameterId::PeakLevel);
    editor.stage(a, "1000").unwrap();
    editor.stage(b, "loud").unwrap();
    let refusal = editor.commit().unwrap_err();
    assert_eq!(refusal.problems, [(b.field, InputProblem::Unparseable)]);
    assert_eq!(
        editor.draft().value(a.field),
        Some(440.0),
        "no partial commit"
    );
    assert_eq!(editor.staged(a.field), Some("1000"));
    assert_eq!(editor.staged(b.field), Some("loud"));
    // Parsed and finite but acoustically invalid: committed for repair.
    editor.stage(b, "3").unwrap();
    editor.stage(a, "30000").unwrap();
    assert_eq!(editor.commit(), Ok(true));
    assert_eq!(editor.draft().value(a.field), Some(30_000.0));
    assert_eq!(editor.draft().value(b.field), Some(3.0));
    assert!(!editor.has_staged());
    let analysis = editor.committed_analysis(SampleRate::DEFAULT);
    assert!(analysis.target().is_none());
    assert_eq!(
        editor.save_readiness(),
        Ok(()),
        "finite invalid values can be saved"
    );
    // Typed nonfinite input is refused like unparseable input.
    editor.stage(a, "inf").unwrap();
    assert_eq!(
        editor.commit().unwrap_err().problems,
        [(a.field, InputProblem::Nonfinite)]
    );
    editor.stage(a, "440").unwrap();
    editor.stage(b, "-30").unwrap();
    assert_eq!(editor.commit(), Ok(true));
    assert!(
        editor
            .committed_analysis(SampleRate::DEFAULT)
            .target()
            .is_some()
    );
}

#[test]
fn inherited_nonfinite_values_block_commit_save_until_repaired() {
    let text = "schema_version=2\nduration_seconds=nan\nfade_seconds=0\n[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=inf\n[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=440\n";
    let snapshot = crate::load_reader(text.as_bytes(), "nonfinite import").unwrap();
    let mut editor = EditorSession::new(Draft::from_snapshot(&snapshot)).unwrap();
    assert!(
        editor.draft().duration_seconds().is_nan(),
        "kept for repair"
    );
    let other = field(&editor, 1, ParameterId::PeakLevel);
    let broken = field(&editor, 0, ParameterId::ToneFrequency);
    let duration = metadata(&editor, MetadataField::Duration);
    editor.stage(other, "-20").unwrap();
    let problems = editor.commit().unwrap_err().problems;
    assert!(problems.contains(&(broken.field, InputProblem::Nonfinite)));
    assert!(problems.contains(&(duration.field, InputProblem::Nonfinite)));
    assert_eq!(editor.draft().value(other.field), Some(-30.0));
    assert!(matches!(
        editor.save_readiness(),
        Err(SaveRefusal::Input(_))
    ));
    editor.stage(broken, "220").unwrap();
    assert!(editor.commit().is_err(), "the duration is still not finite");
    editor.stage(duration, "1").unwrap();
    assert_eq!(editor.commit(), Ok(true));
    assert_eq!(editor.draft().value(other.field), Some(-20.0));
    assert_eq!(editor.save_readiness(), Ok(()));
    // Programmatic nonfinite values are diagnosed the same way.
    let mut draft = Draft::new();
    let id = draft.add(CarrierKind::Tone).unwrap();
    draft.replace(
        id,
        Component {
            peak_dbfs: f64::NEG_INFINITY,
            ..tone(440.0)
        },
    );
    let mut editor = EditorSession::new(draft).unwrap();
    assert_eq!(
        editor.candidate().problems(),
        [(
            EditingField::Parameter(id, ParameterId::PeakLevel),
            InputProblem::Nonfinite
        )]
    );
}

#[test]
fn structural_choices_discard_only_inapplicable_or_removed_staging() {
    let mut editor = editor_with(&[tone(440.0), tone(550.0)]);
    let document = editor.document();
    let first = editor.draft().components()[0].0;
    let frequency = field(&editor, 0, ParameterId::ToneFrequency);
    let level = field(&editor, 0, ParameterId::PeakLevel);
    let other = field(&editor, 1, ParameterId::ToneFrequency);
    editor.stage(frequency, "bad").unwrap();
    editor.stage(level, "also bad").unwrap();
    editor.stage(other, "660").unwrap();
    editor
        .set_carrier(document, first, CarrierKind::Clicks)
        .unwrap();
    assert_eq!(
        editor.staged(frequency.field),
        None,
        "inapplicable staging discarded"
    );
    assert_eq!(editor.staged(level.field), Some("also bad"));
    assert_eq!(editor.staged(other.field), Some("660"));
    assert_eq!(
        editor.draft().component(first).unwrap().carrier,
        controls::default_carrier(CarrierKind::Clicks)
    );
    assert_eq!(
        editor.stage(frequency, "440"),
        Err(EditRefusal::Inapplicable),
        "a stale field of the old carrier is rejected"
    );
    editor
        .set_modulation(
            document,
            first,
            Some(ModulationKind::Rectangular),
            SampleRate::DEFAULT,
        )
        .unwrap();
    let gate = editor.handle(EditingField::Parameter(first, ParameterId::GateWidth));
    editor.stage(gate, "2").unwrap();
    editor
        .set_modulation(document, first, None, SampleRate::DEFAULT)
        .unwrap();
    assert_eq!(editor.staged(gate.field), None);
    assert_eq!(editor.restore(level).unwrap(), "-30");
    assert_eq!(editor.staged(level.field), None);
    editor.remove_component(document, first).unwrap();
    assert_eq!(
        editor.staged(other.field),
        Some("660"),
        "unrelated staging survives"
    );
    assert_eq!(
        editor.remove_component(document, first),
        Err(EditRefusal::UnknownComponent)
    );
}

#[test]
fn the_last_removal_can_be_restored_in_place_with_its_staged_text() {
    let mut editor = editor_with(&[tone(440.0), tone(550.0), tone(660.0)]);
    let document = editor.document();
    editor.record_saved(document, &editor.draft().recipe());
    let middle = editor.draft().components()[1].0;
    let frequency = field(&editor, 1, ParameterId::ToneFrequency);
    let other = field(&editor, 0, ParameterId::ToneFrequency);
    assert_eq!(
        editor.restore_removed(document),
        Err(EditRefusal::NothingRemoved)
    );
    editor.stage(frequency, "bad").unwrap();
    editor.remove_component(document, middle).unwrap();
    assert_eq!(editor.removed(), Some((1, &tone(550.0))));
    assert_eq!(editor.staged(frequency.field), None);
    editor.stage(other, "450").unwrap();

    let stale = EditorSession::new(Draft::new()).unwrap().document();
    assert_eq!(
        editor.restore_removed(stale),
        Err(EditRefusal::StaleDocument)
    );
    assert_eq!(editor.restore_removed(document), Ok(middle));
    let ids: Vec<_> = editor
        .draft()
        .components()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(ids[1], middle, "the same identity returns to its place");
    assert_eq!(editor.draft().component(middle), Some(&tone(550.0)));
    assert_eq!(editor.staged(frequency.field), Some("bad"));
    assert_eq!(
        editor.staged(other.field),
        Some("450"),
        "later staging stays"
    );
    assert_eq!(editor.removed(), None);
    assert_eq!(
        editor.restore_removed(document),
        Err(EditRefusal::NothingRemoved),
        "one removal restores once"
    );
    editor.restore(frequency).unwrap();
    editor.restore(other).unwrap();
    assert!(!editor.dirty(), "restoring returns the saved content");

    // A newer removal replaces the older one, and the limit keeps it waiting.
    let first = editor.draft().components()[0].0;
    let last = editor.draft().components()[2].0;
    editor.remove_component(document, first).unwrap();
    editor.remove_component(document, last).unwrap();
    assert_eq!(editor.removed(), Some((1, &tone(660.0))));
    while editor.draft().components().len() < Stimulus::MAX_COMPONENTS {
        editor.add_component(document, CarrierKind::Tone).unwrap();
    }
    assert_eq!(
        editor.restore_removed(document),
        Err(EditRefusal::ComponentLimit)
    );
    assert_eq!(editor.removed(), Some((1, &tone(660.0))));
    let added = editor.draft().components()[2].0;
    editor.remove_component(document, added).unwrap();
    assert_eq!(editor.restore_removed(document), Ok(added));
}

#[test]
fn stale_document_handles_are_rejected_everywhere() {
    let mut old = editor_with(&[tone(440.0)]);
    let handle = field(&old, 0, ParameterId::ToneFrequency);
    let title = metadata(&old, MetadataField::Title);
    let id = old.draft().components()[0].0;
    let old_document = old.document();
    let mut replacement = EditorSession::new(old.draft().clone()).unwrap();
    assert_ne!(old_document, replacement.document());
    assert_eq!(
        replacement.stage(handle, "1"),
        Err(EditRefusal::StaleDocument)
    );
    assert_eq!(
        replacement.stage(title, "x"),
        Err(EditRefusal::StaleDocument)
    );
    assert_eq!(
        replacement.commit_field(title, "x"),
        Err(EditRefusal::StaleDocument)
    );
    assert_eq!(
        replacement.set_ears(old_document, id, Ears::Left),
        Err(EditRefusal::StaleDocument)
    );
    assert!(!replacement.has_staged());
    assert!(!replacement.record_saved(old_document, &Draft::new().recipe()));
    assert!(old.stage(handle, "1").is_ok());
}

#[test]
fn saved_baselines_compare_content_and_belong_to_one_document() {
    let mut editor = EditorSession::new(Draft::new()).unwrap();
    assert!(!editor.dirty(), "an untouched blank document is clean");
    let title = metadata(&editor, MetadataField::Title);
    editor.commit_field(title, "A").unwrap().unwrap();
    let saved_a = editor.draft().recipe();
    editor.commit_field(title, "B").unwrap().unwrap();
    assert!(editor.record_saved(editor.document(), &saved_a));
    assert!(editor.dirty(), "newer edits stay dirty after saving A");
    editor.commit_field(title, "A").unwrap().unwrap();
    assert!(
        !editor.dirty(),
        "equal saved content is clean whatever the revision"
    );
    editor.stage(title, "C").unwrap();
    assert!(editor.dirty(), "staged text is unsaved");
    let copy = EditorSession::new(Draft::from_snapshot(&crate::catalog().unwrap()[0])).unwrap();
    assert!(copy.dirty(), "a copy needs saving");
}

#[test]
fn evaluations_are_cached_by_revision_and_rate_including_invalid_results() {
    let mut editor = editor_with(&[tone(30_000.0)]);
    let at_48 = SampleRate::new(48_000).unwrap();
    let at_96 = SampleRate::new(96_000).unwrap();
    let first = editor.committed_analysis(at_48);
    assert!(first.target().is_none(), "30 kHz is invalid at 48 kHz");
    assert!(
        editor.committed_analysis(at_96).target().is_some(),
        "valid at 96 kHz"
    );
    let count = editor.evaluations();
    for _ in 0..10 {
        assert!(Arc::ptr_eq(&first, &editor.committed_analysis(at_48)));
        editor.committed_analysis(at_96);
        editor.candidate_analysis(at_48);
    }
    assert_eq!(
        editor.evaluations(),
        count,
        "unchanged invalid input is not evaluated again"
    );
    let level = field(&editor, 0, ParameterId::PeakLevel);
    editor.stage(level, "-20").unwrap();
    editor.candidate_analysis(at_48);
    assert_eq!(editor.evaluations(), count + 1);
    assert!(
        Arc::ptr_eq(&first, &editor.committed_analysis(at_48)),
        "staging leaves the committed evaluation"
    );
}

#[test]
fn identities_are_never_reused_and_the_limit_is_shared() {
    let mut d = Draft::new();
    let a = d.add(CarrierKind::Tone).unwrap();
    let b = d.add(CarrierKind::Clicks).unwrap();
    assert!(d.remove(a).is_some());
    let c = d.add(CarrierKind::Tone).unwrap();
    assert_ne!(a, c);
    assert!(d.replace(
        b,
        Component {
            peak_dbfs: -50.0,
            ..controls::default_component(CarrierKind::Tone)
        }
    ));
    assert!(!d.replace(a, controls::default_component(CarrierKind::Clicks)));
    while d.components().len() < Stimulus::MAX_COMPONENTS {
        d.add(CarrierKind::Tone).unwrap();
    }
    assert_eq!(d.add(CarrierKind::Clicks), Err(EditRefusal::ComponentLimit));
    let mut exhausted = Draft::new();
    exhausted.next_id = u32::MAX;
    assert_eq!(
        exhausted.add(CarrierKind::Tone),
        Err(EditRefusal::IdentitiesExhausted)
    );
}

#[test]
fn copies_keep_source_attribution_without_research_claims_and_match_recipe_ids() {
    let snapshot = crate::catalog().unwrap().remove(0);
    let d = Draft::from_snapshot(&snapshot);
    assert_eq!(d.title(), format!("{} (edited)", snapshot.title()));
    assert_eq!(d.based_on().unwrap().id(), snapshot.id());
    let ids: Vec<_> = d.components().iter().map(|(id, _)| *id).collect();
    let target = snapshot.stimulus().target(SampleRate::DEFAULT).unwrap();
    let recipe_ids: Vec<_> = target.components().map(|(id, _)| id).collect();
    assert_eq!(ids, recipe_ids);
    let about = d.recipe().about.unwrap();
    assert_eq!(about.summary.as_deref(), Some(""));
    assert!(about.citation.is_none() && about.study.is_none() && about.studied_for.is_none());
}

#[test]
fn save_round_trips_and_keeps_existing_destinations() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("recipe.toml");
    let mut d = Draft::new();
    d.add(CarrierKind::Clicks).unwrap();
    d.set_title("Quotes \" 🎧".into());
    save_content(&d.recipe(), &path, crate::Replace::Never).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let loaded = crate::load_file(&path).unwrap();
    assert_eq!(loaded.stimulus(), &d.stimulus());
    assert_eq!(
        save_content(&d.recipe(), &path, crate::Replace::Never)
            .unwrap_err()
            .kind(),
        ErrorKind::DestinationExists
    );
    d.set_title("🎧".repeat(Recipe::MAX_BYTES));
    assert_eq!(
        save_content(&d.recipe(), &path, crate::Replace::Allowed)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidRecipe
    );
    d.set_title("Small".into());
    d.set_duration_seconds(f64::NAN);
    assert!(save_content(&d.recipe(), &path, crate::Replace::Allowed).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
