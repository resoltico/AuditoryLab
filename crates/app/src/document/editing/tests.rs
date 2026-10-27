// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::services::save_content;
use auditorylab_core::{Component, ComponentForm};

fn editor_with(components: &[ComponentForm]) -> EditorSession {
    let mut draft = Draft::new().unwrap();
    for component in components {
        let id = draft.add(CarrierKind::Tone).unwrap();
        draft.replace(id, *component);
    }
    EditorSession::new(draft).unwrap()
}

fn tone(frequency_hz: f64) -> ComponentForm {
    ComponentForm::Tone {
        frequency_hz,
        ears: Ears::Both,
        peak_dbfs: -30.0,
        modulation: None,
    }
}

fn field(editor: &EditorSession, index: usize, parameter: ParameterId) -> FieldHandle {
    editor
        .handle(EditingField::Parameter(
            editor.draft().components()[index].id,
            parameter,
        ))
        .unwrap()
}

fn metadata(editor: &EditorSession, field: MetadataField) -> FieldHandle {
    editor.handle(EditingField::Metadata(field)).unwrap()
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
    let text = "schema_version=4\n[sound]\nduration_seconds=nan\nfade_seconds=0\n[[sound.component]]\nid=1\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=inf\n[[sound.component]]\nid=2\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=440\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n";
    assert!(
        crate::storage::load::load_reader(text.as_bytes(), "nonfinite import").is_err(),
        "nonfinite portable files are rejected"
    );
    let mut draft = Draft::new().unwrap();
    draft.add(CarrierKind::Tone).unwrap();
    draft.add(CarrierKind::Tone).unwrap();
    let first = draft.components()[0].id;
    draft.set_duration_seconds(f64::NAN);
    draft.set_value(
        EditingField::Parameter(first, ParameterId::ToneFrequency),
        f64::INFINITY,
    );
    let mut editor = EditorSession::new(draft).unwrap();
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
    assert!(
        editor.save_readiness().is_ok(),
        "repairing all inherited programmatic values makes the whole document serializable"
    );
    // Programmatic nonfinite values are diagnosed the same way.
    let mut draft = Draft::new().unwrap();
    let id = draft.add(CarrierKind::Tone).unwrap();
    draft.replace(id, {
        let mut form = tone(440.0);
        form.set(auditorylab_core::ParameterId::PeakLevel, f64::NEG_INFINITY);
        form
    });
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
    let first = editor.draft().components()[0].id;
    let first_row = editor.component_key(first).unwrap();
    let frequency = field(&editor, 0, ParameterId::ToneFrequency);
    let level = field(&editor, 0, ParameterId::PeakLevel);
    let other = field(&editor, 1, ParameterId::ToneFrequency);
    editor.stage(frequency, "bad").unwrap();
    editor.stage(level, "also bad").unwrap();
    editor.stage(other, "660").unwrap();
    editor
        .set_carrier(editor.component_key(first).unwrap(), CarrierKind::Clicks)
        .unwrap();
    assert_eq!(
        editor.staged(frequency.field),
        None,
        "inapplicable staging discarded"
    );
    assert_eq!(editor.staged(level.field), Some("also bad"));
    assert_eq!(editor.staged(other.field), Some("660"));
    assert_eq!(
        editor
            .draft()
            .component(first)
            .unwrap()
            .get(ParameterId::ClickRate),
        Some(40.0)
    );
    assert_eq!(
        editor.stage(frequency, "440"),
        Err(EditRefusal::Inapplicable),
        "a stale field of the old carrier is rejected"
    );
    editor
        .set_modulation(
            editor.component_key(first).unwrap(),
            Some(ModulationKind::Gate),
            SampleRate::DEFAULT,
        )
        .unwrap();
    let gate = editor
        .handle(EditingField::Parameter(first, ParameterId::GateWidth))
        .unwrap();
    editor.stage(gate, "2").unwrap();
    editor
        .set_modulation(
            editor.component_key(first).unwrap(),
            None,
            SampleRate::DEFAULT,
        )
        .unwrap();
    assert_eq!(editor.staged(gate.field), None);
    assert_eq!(editor.restore(level).unwrap(), "-30");
    assert_eq!(editor.staged(level.field), None);
    editor.remove_component(first_row).unwrap();
    assert_eq!(
        editor.staged(other.field),
        Some("660"),
        "unrelated staging survives"
    );
    assert_eq!(
        editor.remove_component(first_row),
        Err(EditRefusal::UnknownComponent)
    );
}

#[test]
fn the_last_removal_can_be_restored_in_place_with_its_staged_text() {
    let mut editor = editor_with(&[tone(440.0), tone(550.0), tone(660.0)]);
    let document = editor.document();
    editor.record_saved(document, &editor.draft().recipe().unwrap());
    let middle = editor.draft().components()[1].id;
    let frequency = field(&editor, 1, ParameterId::ToneFrequency);
    let other = field(&editor, 0, ParameterId::ToneFrequency);
    assert_eq!(
        editor.restore_removed(editor.document_key()),
        Err(EditRefusal::NothingRemoved)
    );
    editor.stage(frequency, "bad").unwrap();
    editor
        .remove_component(editor.component_key(middle).unwrap())
        .unwrap();
    assert_eq!(
        editor.removed(),
        Some((
            1,
            &Component {
                id: middle,
                form: tone(550.0)
            }
        ))
    );
    assert_eq!(editor.staged(frequency.field), None);
    editor.stage(other, "450").unwrap();

    let stale = EditorSession::new(Draft::new().unwrap())
        .unwrap()
        .document_key();
    assert_eq!(
        editor.restore_removed(stale),
        Err(EditRefusal::StaleDocument)
    );
    assert_eq!(editor.restore_removed(editor.document_key()), Ok(middle));
    let ids: Vec<_> = editor.draft().components().iter().map(|c| c.id).collect();
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
        editor.restore_removed(editor.document_key()),
        Err(EditRefusal::NothingRemoved),
        "one removal restores once"
    );
    assert_eq!(editor.restore(frequency), Err(EditRefusal::StaleRow));
    editor
        .restore(editor.handle(frequency.field).unwrap())
        .unwrap();
    editor.restore(other).unwrap();
    assert!(!editor.dirty(), "restoring returns the saved content");

    // A newer removal replaces the older one, and the limit keeps it waiting.
    let first = editor.draft().components()[0].id;
    let last = editor.draft().components()[2].id;
    editor
        .remove_component(editor.component_key(first).unwrap())
        .unwrap();
    editor
        .remove_component(editor.component_key(last).unwrap())
        .unwrap();
    assert_eq!(
        editor.removed(),
        Some((
            1,
            &Component {
                id: last,
                form: tone(660.0)
            }
        ))
    );
    while editor.draft().components().len() < Sound::MAX_COMPONENTS {
        editor
            .add_component(editor.document_key(), CarrierKind::Tone)
            .unwrap();
    }
    assert_eq!(
        editor.restore_removed(editor.document_key()),
        Err(EditRefusal::BudgetExceeded {
            components: 16,
            voices: 16,
            additional_voices: 1
        })
    );
    assert_eq!(
        editor.removed(),
        Some((
            1,
            &Component {
                id: last,
                form: tone(660.0)
            }
        ))
    );
    let added = editor.draft().components()[2].id;
    editor
        .remove_component(editor.component_key(added).unwrap())
        .unwrap();
    assert_eq!(editor.restore_removed(editor.document_key()), Ok(added));
}

#[test]
fn stale_document_handles_are_rejected_everywhere() {
    let mut old = editor_with(&[tone(440.0)]);
    let handle = field(&old, 0, ParameterId::ToneFrequency);
    let title = metadata(&old, MetadataField::Title);
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
        replacement.set_ears(handle.row, Ears::Left),
        Err(EditRefusal::StaleDocument)
    );
    assert!(!replacement.has_staged());
    assert!(!replacement.record_saved(old_document, &Draft::new().unwrap().recipe().unwrap()));
    assert!(old.stage(handle, "1").is_ok());
}

#[test]
fn saved_baselines_compare_content_and_belong_to_one_document() {
    let mut editor = EditorSession::new(Draft::new().unwrap()).unwrap();
    assert!(!editor.dirty(), "an untouched blank document is clean");
    let title = metadata(&editor, MetadataField::Title);
    editor.commit_field(title, "A").unwrap().unwrap();
    let saved_a = editor.draft().recipe().unwrap();
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
    let copy =
        EditorSession::new(Draft::from_snapshot(&crate::services::catalog().unwrap()[0]).unwrap())
            .unwrap();
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
    let mut d = Draft::new().unwrap();
    let a = d.add(CarrierKind::Tone).unwrap();
    let b = d.add(CarrierKind::Clicks).unwrap();
    assert!(d.remove(a).is_some());
    let c = d.add(CarrierKind::Tone).unwrap();
    assert_ne!(a, c);
    assert!(d.replace(b, {
        let mut form = controls::default_component(CarrierKind::Tone);
        form.set(auditorylab_core::ParameterId::PeakLevel, -50.0);
        form
    }));
    assert!(!d.replace(a, controls::default_component(CarrierKind::Clicks)));
    while d.components().len() < Sound::MAX_COMPONENTS {
        d.add(CarrierKind::Tone).unwrap();
    }
    assert_eq!(
        d.add(CarrierKind::Clicks),
        Err(EditRefusal::BudgetExceeded {
            components: 16,
            voices: 16,
            additional_voices: 1
        })
    );
    let mut exhausted = Draft::new().unwrap();
    exhausted.next_id = None;
    assert_eq!(
        exhausted.add(CarrierKind::Tone),
        Err(EditRefusal::IdentitiesExhausted)
    );
}

#[test]
fn copies_keep_source_attribution_without_research_claims_and_match_recipe_ids() {
    let snapshot = crate::services::catalog().unwrap().remove(0);
    let d = Draft::from_snapshot(&snapshot).unwrap();
    assert_eq!(d.title(), format!("{} (edited)", snapshot.title()));
    assert_eq!(d.based_on().unwrap().id(), snapshot.id());
    let ids: Vec<_> = d.components().iter().map(|c| c.id).collect();
    let target = snapshot.sound().target(SampleRate::DEFAULT).unwrap();
    let recipe_ids: Vec<_> = target.voices().map(|(id, _)| id).collect();
    assert_eq!(
        ids,
        snapshot
            .sound()
            .components
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        d.check(SampleRate::DEFAULT)
            .target()
            .unwrap()
            .voices()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        recipe_ids
    );
    let about = crate::presentation::recipe_text::about(&d.recipe().unwrap()).unwrap();
    assert_eq!(about.summary, snapshot.about().unwrap().summary);
    assert!(about.research.is_none());
}

#[test]
fn save_round_trips_and_keeps_existing_destinations() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("recipe.toml");
    let mut d = Draft::new().unwrap();
    d.add(CarrierKind::Clicks).unwrap();
    d.set_title("Quotes \" 🎧".into());
    save_content(&d.recipe().unwrap(), &path, crate::services::Replace::Never).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let loaded = crate::storage::load::load_file(&path).unwrap();
    assert_eq!(loaded.sound(), &d.sound());
    assert_eq!(
        save_content(&d.recipe().unwrap(), &path, crate::services::Replace::Never)
            .unwrap_err()
            .outcome(),
        crate::error::Outcome::DestinationExists
    );
    d.set_title("🎧".repeat(Recipe::MAX_BYTES));
    assert_eq!(
        save_content(
            &d.recipe().unwrap(),
            &path,
            crate::services::Replace::Allowed
        )
        .unwrap_err()
        .outcome(),
        crate::error::Outcome::InvalidRecipe
    );
    d.set_title("Small".into());
    d.set_duration_seconds(f64::NAN);
    assert!(
        d.recipe()
            .map_err(crate::error::OperationError::from)
            .and_then(
                |recipe| save_content(&recipe, &path, crate::services::Replace::Allowed)
                    .map_err(Into::into)
            )
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn derivatives_reopen_with_full_scoped_graph_and_bounded_flat_ancestry() {
    let source = crate::services::catalog().unwrap().remove(0);
    let original = source.recipe().clone();
    let mut draft = Draft::from_snapshot(&source).unwrap();
    draft.set_summary("Rīga \"quoted\" 🎧\nsecond line".into());
    draft.set_duration_seconds(2.0);
    let mut bytes = draft.to_toml().unwrap();
    for expected in 1..=16 {
        let reopened =
            crate::storage::load::load_reader(bytes.as_bytes(), "reopened elsewhere").unwrap();
        assert!(reopened.about().unwrap().research.is_none());
        assert_eq!(reopened.recipe().sources(), original.sources());
        assert_eq!(reopened.recipe().lineage().len(), expected);
        assert!(
            reopened
                .recipe()
                .evidence()
                .iter()
                .all(|e| e.scope == auditorylab_core::EvidenceScope::Ancestor)
        );
        assert!(!bytes.contains("reopened elsewhere"));
        if expected == 16 {
            assert!(Draft::from_snapshot(&reopened).is_err());
            break;
        }
        draft = Draft::from_snapshot(&reopened).unwrap();
        draft.set_title("Derivative".into());
        bytes = draft.to_toml().unwrap();
    }
}

#[test]
fn the_entire_document_including_unicode_anchor_has_exact_byte_neighbours() {
    let source = crate::services::catalog().unwrap().remove(0);
    let mut draft = Draft::from_snapshot(&source).unwrap();
    draft.set_summary("é🎧".into());
    let remaining = Recipe::MAX_BYTES - draft.to_toml().unwrap().len();
    draft.set_summary(format!("é🎧{}", "a".repeat(remaining)));
    let text = draft.to_toml().unwrap();
    assert_eq!(text.len(), Recipe::MAX_BYTES);
    Recipe::parse(&text).unwrap();
    draft.set_summary(format!("{}a", draft.summary()));
    assert!(draft.to_toml().is_err());
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("existing.toml");
    std::fs::write(&path, b"unchanged").unwrap();
    assert!(
        save_content(
            &draft.recipe().unwrap(),
            &path,
            crate::services::Replace::Allowed
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
    let absent = dir.path().join("absent.toml");
    assert!(
        save_content(
            &draft.recipe().unwrap(),
            &absent,
            crate::services::Replace::Never
        )
        .is_err()
    );
    assert!(!absent.exists());
}

#[test]
fn finite_invalid_current_content_saves_and_nonfinite_content_never_touches_a_destination() {
    let mut builder = Draft::new().unwrap().recipe().unwrap().into_builder();
    builder.sound.duration_seconds = -1.0;
    builder.sound.fade_seconds = 9.0;
    let recipe = builder.clone().build().unwrap();
    let text = serialize(&recipe).unwrap();
    assert_eq!(Recipe::parse(&text).unwrap(), recipe);
    builder.sound.duration_seconds = f64::NAN;
    assert!(builder.build().is_err());
    let mut editor = EditorSession::new(Draft::from_recipe(Recipe::parse(&text).unwrap())).unwrap();
    assert!(editor.save_readiness().is_ok());
    let document = editor.document();
    let saved = editor.draft().recipe().unwrap();
    assert!(editor.record_saved(document, &saved));
    assert!(!editor.dirty());
}

#[test]
fn descriptions_and_equivalent_text_do_not_recompile_sound_inputs() {
    let mut editor = editor_with(&[tone(440.0)]);
    let rate = SampleRate::DEFAULT;
    let first = editor.committed_analysis(rate);
    let evaluations = editor.evaluations();
    let notes = editor
        .handle(EditingField::Metadata(MetadataField::Summary))
        .unwrap();
    editor.stage(notes, "Own notes 🎧").unwrap();
    assert!(Arc::ptr_eq(&first, &editor.candidate_analysis(rate)));
    editor.commit().unwrap();
    assert!(Arc::ptr_eq(&first, &editor.committed_analysis(rate)));
    let frequency = field(&editor, 0, ParameterId::ToneFrequency);
    editor.stage(frequency, "440.000").unwrap();
    assert!(Arc::ptr_eq(&first, &editor.candidate_analysis(rate)));
    assert_eq!(editor.evaluations(), evaluations);
    editor.stage(frequency, "880").unwrap();
    assert!(!Arc::ptr_eq(&first, &editor.candidate_analysis(rate)));
    assert_eq!(editor.evaluations(), evaluations + 1);
}

#[test]
fn nominal_cache_and_saved_memberships_follow_distinct_committed_snapshots() {
    let mut c = tone(240.0);
    c = auditorylab_core::ComponentForm::Clicks {
        ears: c.route().unwrap(),
        peak_dbfs: c.get(ParameterId::PeakLevel).unwrap(),
        modulation: c.modulation(),
        rate_hz: 2.0,
        width_seconds: 0.001,
    };
    let mut editor = editor_with(&[c]);
    let first = editor.nominal();
    let saved = editor.draft().recipe().unwrap();
    let rate = field(&editor, 0, ParameterId::ClickRate);
    editor.stage(rate, "bad").unwrap();
    assert!(Arc::ptr_eq(&first, &editor.nominal()));
    editor.commit_field(rate, "6").unwrap().unwrap();
    assert_eq!(editor.nominal().bands, [crate::core::rhythm::Band::Theta]);
    assert!(editor.record_saved(editor.document(), &saved));
    assert_eq!(
        editor.saved_nominal().unwrap().bands,
        [crate::core::rhythm::Band::Delta]
    );
    assert_eq!(editor.nominal().bands, [crate::core::rhythm::Band::Theta]);
    assert!(editor.dirty());
    let stale = editor.document();
    let mut replacement = editor_with(&[tone(440.0)]);
    assert!(!replacement.record_saved(stale, &saved));
    assert!(replacement.saved_nominal().is_none());
}

#[test]
fn committed_sound_edits_never_repromote_evidence_by_merely_retuning_back() {
    let source = crate::services::builtin("ioannou-2015-binaural-2-hz").unwrap();
    let mut editor = EditorSession::new(Draft::from_recipe(source.recipe().clone())).unwrap();
    editor.record_saved(editor.document(), source.recipe());
    let title = metadata(&editor, MetadataField::Title);
    editor.commit_field(title, "My title").unwrap().unwrap();
    assert_eq!(
        editor.draft().recipe().unwrap().evidence()[0].scope,
        crate::core::EvidenceScope::ReferenceCondition
    );
    let offset = field(&editor, 0, ParameterId::PairOffset);
    editor.commit_field(offset, "6").unwrap().unwrap();
    assert_eq!(
        editor.draft().recipe().unwrap().evidence()[0].scope,
        crate::core::EvidenceScope::Ancestor
    );
    editor.commit_field(offset, "2").unwrap().unwrap();
    assert_eq!(
        editor.draft().recipe().unwrap().evidence()[0].scope,
        crate::core::EvidenceScope::Ancestor
    );
}
#[test]
fn complete_remove_undo_restores_full_metadata_but_partial_or_published_undo_cannot() {
    let source = crate::services::builtin("ioannou-2015-binaural-2-hz").unwrap();
    for intervening in [0, 1, 2] {
        let mut editor = EditorSession::new(Draft::from_recipe(source.recipe().clone())).unwrap();
        let doc = editor.document();
        editor.record_saved(doc, source.recipe());
        let id = editor.draft().components()[0].id;
        editor
            .remove_component(editor.component_key(id).unwrap())
            .unwrap();
        assert_eq!(
            editor.draft().recipe().unwrap().evidence()[0].scope,
            crate::core::EvidenceScope::Ancestor
        );
        if intervening == 1 {
            let notes = metadata(&editor, MetadataField::Summary);
            editor.commit_field(notes, "later note").unwrap().unwrap();
        }
        if intervening == 2 {
            let published = editor.draft().recipe().unwrap();
            editor.record_saved(doc, &published);
        }
        editor.restore_removed(editor.document_key()).unwrap();
        assert_eq!(
            editor.draft().recipe().unwrap().evidence()[0].scope,
            if intervening == 0 {
                crate::core::EvidenceScope::ReferenceCondition
            } else {
                crate::core::EvidenceScope::Ancestor
            }
        );
        if intervening == 0 {
            assert!(!editor.dirty());
        }
        if intervening == 1 {
            assert_eq!(editor.draft().summary(), "later note");
        }
    }
}
#[test]
fn publication_history_updates_the_same_draft_without_reusing_a_stale_candidate_cache() {
    let mut editor = EditorSession::new(Draft::new().unwrap()).unwrap();
    let before = editor.candidate();
    let mut published = before.draft().recipe().unwrap().into_builder();
    published.document.created_at = Some("2026-10-24T09:00:00Z".parse().unwrap());
    published.document.modified_at = published.document.created_at;
    let title = metadata(&editor, MetadataField::Title);
    editor.commit_field(title, "Later title").unwrap().unwrap();
    let published = published.build().unwrap();
    assert!(editor.record_saved(editor.document(), &published));
    assert!(editor.dirty());
    let after = editor.candidate();
    assert_eq!(
        after.draft().recipe().unwrap().document(),
        published.document()
    );
    assert_eq!(after.draft().title(), "Later title");
    assert_ne!(after.draft().title(), before.draft().title());
}

#[test]
fn row_keys_round_trip_without_losing_any_identity_or_generation_bits() {
    for generation in [0, 1, u64::MAX] {
        let key = RowKey::for_component(
            DocumentId(u64::MAX),
            ComponentId::new(u32::MAX).unwrap(),
            generation,
        );
        assert_eq!(key.to_string().parse::<RowKey>(), Ok(key));
        assert!(key.to_string().len() <= 42);
    }
    let document = RowKey::for_document(DocumentId(1));
    assert_eq!(document.to_string().parse::<RowKey>(), Ok(document));
    assert!(document.parameter(ParameterId::PeakLevel).is_none());
    let component = RowKey::for_component(DocumentId(1), ComponentId::new(1).unwrap(), 0);
    assert!(component.metadata(MetadataField::Title).is_none());
    for text in [
        "",
        "0:0:0",
        "01:0:0",
        "1:01:0",
        "1:1:00",
        "1:0:1",
        "1:100000000:0",
        "1:1:10000000000000000",
        "1:1:A",
        "1:1:0:",
        "1:1",
        " 1:1:0",
        "+1:1:0",
        "1:1:-1",
    ] {
        assert!(text.parse::<RowKey>().is_err(), "{text}");
    }
}

#[test]
fn the_application_rejects_retired_field_and_structural_callbacks_after_undo() {
    let mut editor = editor_with(&[tone(440.0), tone(550.0)]);
    let document = editor.document_key();
    let id = editor.draft().components()[0].id;
    let current = editor.component_key(id).unwrap();
    let field = current.parameter(ParameterId::ToneFrequency).unwrap();
    let unrelated = editor
        .component_key(editor.draft().components()[1].id)
        .unwrap();
    editor.stage(field, "bad").unwrap();
    editor.remove_component(current).unwrap();
    assert_eq!(
        editor.removed.as_ref().unwrap().generation,
        current.generation + 1
    );
    assert!(editor.component_key(id).is_none());
    assert_eq!(
        editor.stage(field, "999"),
        Err(EditRefusal::UnknownComponent)
    );
    editor.restore_removed(document).unwrap();
    let restored = editor.component_key(id).unwrap();
    assert_eq!(restored.generation, current.generation + 2);
    assert_ne!(restored, current);
    assert_eq!(
        editor.component_key(editor.draft().components()[1].id),
        Some(unrelated)
    );
    assert_eq!(editor.stage(field, "999"), Err(EditRefusal::StaleRow));
    assert_eq!(
        editor.commit_field(field, "999"),
        Err(EditRefusal::StaleRow)
    );
    assert_eq!(editor.restore(field), Err(EditRefusal::StaleRow));
    assert_eq!(
        editor.set_ears(current, Ears::Left),
        Err(EditRefusal::StaleRow)
    );
    assert_eq!(
        editor.set_carrier(current, CarrierKind::Clicks),
        Err(EditRefusal::StaleRow)
    );
    assert_eq!(
        editor.set_modulation(current, Some(ModulationKind::Am), SampleRate::DEFAULT),
        Err(EditRefusal::StaleRow)
    );
    assert_eq!(editor.remove_component(current), Err(EditRefusal::StaleRow));
    assert_eq!(editor.staged(field.field), Some("bad"));
    assert_eq!(editor.draft().component(id), Some(&tone(440.0)));
    editor
        .commit_field(
            restored.parameter(ParameterId::ToneFrequency).unwrap(),
            "660",
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        editor
            .draft()
            .component(id)
            .unwrap()
            .get(ParameterId::ToneFrequency),
        Some(660.0)
    );
}

#[test]
fn unknown_or_cross_row_field_requests_cannot_acquire_another_lifetime() {
    let mut editor = EditorSession::new(Draft::new().unwrap()).unwrap();
    assert_eq!(
        editor.handle(EditingField::Parameter(
            ComponentId::new(1).unwrap(),
            ParameterId::ToneFrequency
        )),
        Err(EditRefusal::UnknownComponent)
    );
    let document = editor.document_key();
    let first = editor.add_component(document, CarrierKind::Tone).unwrap();
    let second = editor.add_component(document, CarrierKind::Tone).unwrap();
    let first_key = editor.component_key(first).unwrap();
    let crossed = FieldHandle {
        row: first_key,
        field: EditingField::Parameter(second, ParameterId::ToneFrequency),
    };
    assert_eq!(editor.stage(crossed, "999"), Err(EditRefusal::Inapplicable));
    let metadata = FieldHandle {
        row: first_key,
        field: EditingField::Metadata(MetadataField::Title),
    };
    assert_eq!(
        editor.stage(metadata, "wrong title"),
        Err(EditRefusal::Inapplicable)
    );
    assert_eq!(
        editor.add_component(first_key, CarrierKind::Tone),
        Err(EditRefusal::Inapplicable)
    );
    assert_eq!(
        editor.remove_component(document),
        Err(EditRefusal::Inapplicable)
    );
    assert!(!editor.has_staged());
}

#[test]
fn row_generation_storage_is_bounded_and_exhaustion_never_wraps_or_drops_undo() {
    let mut editor = editor_with(&[tone(440.0)]);
    let id = editor.draft().components()[0].id;
    let document = editor.document_key();
    for _ in 0..100 {
        editor
            .remove_component(editor.component_key(id).unwrap())
            .unwrap();
        assert!(editor.row_generations.is_empty());
        editor.restore_removed(document).unwrap();
        assert_eq!(editor.row_generations.len(), 1);
    }
    editor.row_generations.insert(id, u64::MAX);
    let key = editor.component_key(id).unwrap();
    let before = editor.draft().recipe().unwrap();
    assert_eq!(
        editor.remove_component(key),
        Err(EditRefusal::IdentitiesExhausted)
    );
    assert!(
        editor
            .draft()
            .recipe()
            .unwrap()
            .same_editable_content(&before)
    );
    assert!(editor.removed().is_none());
    editor.row_generations.insert(id, u64::MAX - 1);
    editor
        .remove_component(editor.component_key(id).unwrap())
        .unwrap();
    assert_eq!(
        editor.restore_removed(document),
        Err(EditRefusal::IdentitiesExhausted)
    );
    assert!(editor.removed().is_some());
    assert!(editor.draft().components().is_empty());
    assert!(editor.row_generations.is_empty());
}

#[test]
fn editor_row_identity_producers_refuse_invalid_owner_shapes_without_rejecting_acoustics() {
    let mut draft = Draft::new().unwrap();
    draft.add(CarrierKind::Tone).unwrap();
    let recipe = draft.recipe().unwrap();
    assert!(ComponentId::new(0).is_none());
    let mut duplicate = recipe.clone().into_builder();
    duplicate
        .sound
        .components
        .push(duplicate.sound.components[0]);
    assert!(duplicate.build().is_err());
    let mut acoustic = recipe.into_builder();
    acoustic.sound.components[0]
        .form
        .set(ParameterId::ToneFrequency, -1.0);
    let editor = EditorSession::new(Draft::from_recipe(acoustic.build().unwrap())).unwrap();
    assert!(editor.component_key(ComponentId::new(1).unwrap()).is_some());
    assert!(
        editor
            .handle(EditingField::Parameter(
                ComponentId::new(1).unwrap(),
                ParameterId::ToneFrequency
            ))
            .is_ok()
    );
}

#[test]
fn authored_and_expanded_addition_boundaries_report_current_counts() {
    use auditorylab_core::PairKind;
    let primitive = tone(440.0);
    let pair = controls::default_pair(PairKind::Binaural);
    for (primitives, pairs, requested, expected) in [
        (
            0,
            8,
            primitive,
            EditRefusal::BudgetExceeded {
                components: 8,
                voices: 16,
                additional_voices: 1,
            },
        ),
        (
            0,
            8,
            pair,
            EditRefusal::BudgetExceeded {
                components: 8,
                voices: 16,
                additional_voices: 2,
            },
        ),
        (
            16,
            0,
            primitive,
            EditRefusal::BudgetExceeded {
                components: 16,
                voices: 16,
                additional_voices: 1,
            },
        ),
        (
            15,
            0,
            pair,
            EditRefusal::BudgetExceeded {
                components: 15,
                voices: 15,
                additional_voices: 2,
            },
        ),
    ] {
        let mut draft = Draft::new().unwrap();
        for _ in 0..primitives {
            draft.add_node(primitive).unwrap();
        }
        for _ in 0..pairs {
            draft.add_node(pair).unwrap();
        }
        let before = draft.components().to_vec();
        assert_eq!(draft.check_addition(requested), Err(expected));
        assert_eq!(draft.add_node(requested), Err(expected));
        assert_eq!(
            draft.restore(
                0,
                Component {
                    id: ComponentId::new(100).unwrap(),
                    form: requested
                }
            ),
            Err(expected)
        );
        assert_eq!(draft.components(), before);
    }
    let mut draft = Draft::new().unwrap();
    for _ in 0..14 {
        draft.add_node(primitive).unwrap();
    }
    draft.add_node(pair).unwrap();
    assert_eq!(draft.components().len(), 15);
    let mut draft = Draft::new().unwrap();
    for _ in 0..15 {
        draft.add_node(primitive).unwrap();
    }
    draft.add_node(primitive).unwrap();
    assert_eq!(draft.components().len(), 16);
}
