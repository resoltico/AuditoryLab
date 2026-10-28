use super::*;
use crate::RecipeSnapshotText;
use crate::recipe_text::SnapshotText;
use auditorylab_core::catalog;
use std::collections::BTreeMap;

#[test]
fn display_about_uses_population_and_condition_labels_for_every_declaration() {
    let source = auditorylab_core::catalog::ENTRIES
        .iter()
        .find_map(|entry| {
            let recipe = Recipe::parse(&entry.text()).unwrap();
            recipe
                .evidence()
                .iter()
                .any(|e| e.scope == auditorylab_core::provenance::EvidenceScope::ReferenceCondition)
                .then_some(recipe)
        })
        .unwrap();
    for population in auditorylab_core::provenance::Population::ALL {
        for role in auditorylab_core::provenance::ConditionRole::ALL {
            let mut builder = source.clone().into_builder();
            let evidence = builder
                .content
                .evidence
                .iter_mut()
                .find(|e| {
                    e.scope == auditorylab_core::provenance::EvidenceScope::ReferenceCondition
                })
                .unwrap();
            evidence.population = population;
            evidence.population_detail = None;
            evidence.condition_role = role;
            let snapshot = auditorylab_app::document::recipe::RecipeSnapshot::new(
                auditorylab_app::document::recipe::Origin::Stream("test".into()),
                builder.build().unwrap(),
            );
            let about = snapshot.about().unwrap();
            let text = about
                .research
                .as_ref()
                .unwrap()
                .studied_for
                .as_ref()
                .unwrap();
            assert_eq!(
                *text,
                format!("{} study · {}", population.label(), role.label())
            );
            assert!(!text.contains("OtherAnimal") && !text.contains("NotApplicable"));
        }
    }
}

fn printable_about_value(value: &str) -> bool {
    value
        .chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}

#[test]
fn descriptive_text_supports_unicode_quotes_and_multiline() {
    assert!(printable_about_value("Rīga \"quoted\"\nline\ttab"));
    assert!(!printable_about_value("escape\u{1b}"));
}

#[test]
fn every_entry_states_all_about_text_and_one_source_has_one_citation() {
    let mut citations = BTreeMap::new();
    for entry in catalog::ENTRIES {
        let about = about(&Recipe::parse(&entry.text()).unwrap())
            .unwrap_or_else(|| panic!("{} has no about table", entry.id));
        let field = |name: &str, value: &Option<String>| {
            value
                .clone()
                .unwrap_or_else(|| panic!("{}: no {name}", entry.id))
        };
        let research = about.research.as_ref().unwrap();
        let citation = field("citation", &research.citation);
        let source_url = field("source_url", &research.source_url);
        for (name, value) in [
            ("title", about.title.clone()),
            ("studied_for", field("studied_for", &research.studied_for)),
            ("summary", field("summary", &research.summary)),
            ("citation", citation.clone()),
            ("source_url", source_url.clone()),
            ("study", field("study", &research.study)),
            ("adaptation", field("adaptation", &research.adaptation)),
        ] {
            assert!(!value.trim().is_empty(), "{}: empty {name}", entry.id);
            // Human text permits Unicode and TOML escaping; control bytes remain invalid.
            assert!(printable_about_value(&value), "{}: {name}", entry.id);
        }
        assert!(
            source_url.starts_with("https://doi.org/10."),
            "{}",
            entry.id
        );
        let year = entry
            .id
            .split('-')
            .find(|part| part.len() == 4 && part.bytes().all(|b| b.is_ascii_digit()))
            .unwrap();
        assert!(citation.contains(&format!("({year})")), "{}", entry.id);
        let first = citations
            .entry(source_url.clone())
            .or_insert_with(|| citation.clone());
        assert_eq!(*first, citation, "{}", entry.id);
    }
}

#[test]
fn authored_descriptions_are_used_by_snapshot_details_and_invalid_reports() {
    use auditorylab_app::document::recipe::Origin;
    use auditorylab_app::document::recipe::RecipeSnapshot;
    use auditorylab_core::ComponentId;
    use auditorylab_core::DocumentIdentity;
    use auditorylab_core::Ears;
    use auditorylab_core::Pair;
    use auditorylab_core::SampleRate;
    use auditorylab_core::Sound;
    let plain = ComponentForm::Tone {
        frequency_hz: finite(437.0),
        ears: Ears::Left,
        peak_dbfs: finite(-24.0),
        modulation: None,
    };
    let pair = Pair {
        first_hz: finite(200.0),
        offset_hz: finite(-6.0),
        first_peak_dbfs: finite(-20.0),
        second_peak_dbfs: finite(-30.0),
    };
    for (form, expected) in [
        (plain, "tone 437 Hz, peak -24 dBFS, left ear"),
        (
            {
                let mut form = plain;
                form.set_modulation(Some(Modulation::Am {
                    rate_hz: finite(7.0),
                    depth: finite(0.5),
                }));
                form
            },
            "tone 437 Hz, peak -24 dBFS, left ear, AM 7 Hz, depth 0.5",
        ),
        (
            {
                let mut form = plain;
                form.set_modulation(Some(Modulation::Gate {
                    rate_hz: finite(7.0),
                    width_seconds: finite(0.01),
                }));
                form
            },
            "tone 437 Hz, peak -24 dBFS, left ear, gate 7 Hz, 0.01 s open",
        ),
        (
            {
                let base = plain;
                ComponentForm::Clicks {
                    rate_hz: finite(7.0),
                    width_seconds: finite(0.01),
                    ears: base.route().unwrap(),
                    peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                    modulation: base.modulation(),
                }
            },
            "clicks 7 Hz, 0.01 s wide, peak -24 dBFS, left ear",
        ),
        (
            {
                ComponentForm::BinauralPair {
                    left_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    left_peak_dbfs: pair.first_peak_dbfs,
                    right_peak_dbfs: pair.second_peak_dbfs,
                }
            },
            "binaural pair, first 200 Hz, signed offset -6 Hz, second 194 Hz, peaks -20 / -30 dBFS",
        ),
        (
            {
                let pair = Pair {
                    offset_hz: finite(6.0),
                    ..pair
                };
                ComponentForm::MonauralPair {
                    ears: Ears::Right,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    first_peak_dbfs: pair.first_peak_dbfs,
                    second_peak_dbfs: pair.second_peak_dbfs,
                }
            },
            "monaural pair, first 200 Hz, signed offset 6 Hz, second 206 Hz, peaks -20 / -30 dBFS",
        ),
        (
            {
                let base = plain;
                ComponentForm::Tone {
                    frequency_hz: finite(-437.0),
                    ears: base.route().unwrap(),
                    peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                    modulation: base.modulation(),
                }
            },
            "tone -437 Hz, peak -24 dBFS, left ear",
        ),
    ] {
        let c = Component {
            id: ComponentId::new(7).unwrap(),
            form,
        };
        assert_eq!(component(&c), expected);
        let mut candidate = Recipe::builder(DocumentIdentity {
            id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
            created_at: None,
            modified_at: None,
        });
        candidate.content.sound = Sound::new(finite(2.0), finite(0.1), vec![c])
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        let recipe = candidate.build().unwrap();
        let snapshot = RecipeSnapshot::new(Origin::Stream("test".into()), recipe);
        assert!(snapshot.details_at(SampleRate::DEFAULT).contains(expected));
        if snapshot.at(SampleRate::DEFAULT).is_err() {
            assert!(snapshot.description().contains(expected));
        }
    }
}

#[test]
fn independent_copies_store_the_attribution_and_ancestor_notes_verbatim() {
    use auditorylab_app::document::editing::Draft;
    use auditorylab_app::document::recipe::Origin;
    use auditorylab_app::document::recipe::RecipeSnapshot;
    use auditorylab_core::DocumentIdentity;
    use auditorylab_core::provenance::Description;
    for (notes, expected) in [
        (
            None,
            "Attributed copy; source findings concern the ancestor, not this sound.",
        ),
        (
            Some(""),
            "Attributed copy; source findings concern the ancestor, not this sound.",
        ),
        (
            Some("Parent notes."),
            "Attributed copy; source findings concern the ancestor, not this sound. Ancestor notes: Parent notes.",
        ),
    ] {
        let mut candidate = Recipe::builder(DocumentIdentity {
            id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
            created_at: None,
            modified_at: None,
        });
        candidate.content.description = Some(Description {
            title: "Parent".into(),
            notes: notes.map(str::to_owned),
        });
        let recipe = candidate.build().unwrap();
        let snapshot = RecipeSnapshot::new(Origin::Stream("test".into()), recipe);
        let copy = Draft::from_snapshot(&snapshot).unwrap().recipe().unwrap();
        assert_eq!(copy.lineage().last().unwrap().note, expected);
        assert_eq!(
            copy.lineage().last().unwrap().document_id,
            Some(snapshot.recipe().document().id)
        );
    }
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
