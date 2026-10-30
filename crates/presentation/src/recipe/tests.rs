use super::*;

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
            let about = crate::recipe::snapshot_about(&(snapshot)).unwrap();
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
    use auditorylab_core::SampleRate;

    for (form, expected) in authored_description_cases() {
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
        candidate.content.sound = {
            let fixture = auditorylab_app::core::test_support::FiniteFixture::new(
                finite(2.0),
                finite(0.1),
                vec![c],
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
            candidate.content.timing = fixture.timing;
            fixture.sound
        };
        let recipe = candidate.build().unwrap();
        let snapshot = RecipeSnapshot::new(Origin::Stream("test".into()), recipe);
        assert!(crate::snapshot_details_at(&(snapshot), SampleRate::DEFAULT).contains(expected));
        if snapshot.at(SampleRate::DEFAULT).is_err() {
            assert!(crate::snapshot_description(&(snapshot)).contains(expected));
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
            "Attributed copy; source findings concern the ancestor, not this \
                sound. Ancestor notes: Parent notes.",
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

#[expect(
    clippy::too_many_lines,
    reason = "Literal oracle specifies all seven authored source/modulation descriptions"
)]
fn authored_description_cases() -> [(ComponentForm, &'static str); 7] {
    use auditorylab_core::{ChannelRouting, Pair};
    let plain = ComponentForm {
        source: auditorylab_app::core::SoundSource::Tone {
            frequency_hz: finite(437.0),
            route: ChannelRouting::Left,
            peak_dbfs: finite(-24.0),
        },
        modulation: None,
    };
    let pair = Pair {
        first_hz: finite(200.0),
        offset_hz: finite(-6.0),
        first_peak_dbfs: finite(-20.0),
        second_peak_dbfs: finite(-30.0),
    };
    [
        (plain, "tone 437 Hz, peak -24 dBFS, left channel"),
        (
            {
                let mut form = plain;
                form.set_modulation(Some(Modulation::Sinusoidal {
                    rate_hz: finite(7.0),
                    depth: finite(0.5),
                }));
                form
            },
            "tone 437 Hz, peak -24 dBFS, left channel, sinusoidal AM 7 Hz, depth 50%",
        ),
        (
            {
                let mut form = plain;
                form.set_modulation(Some(Modulation::Pulsed {
                    pulse: auditorylab_app::core::PulseSpec {
                        rate_hz: finite(7.0),
                        width: auditorylab_app::core::PulseWidth::Duration {
                            seconds: finite(0.01),
                        },
                        shape: auditorylab_app::core::PulseShape::Rectangular {},
                    },
                }));
                form
            },
            "tone 437 Hz, peak -24 dBFS, left channel, shared pulsed modulation 7 pulses/s · \
                requested 10 ms fixed window · rectangular",
        ),
        (
            {
                let base = plain;
                ComponentForm {
                    source: auditorylab_app::core::SoundSource::PulseTrain {
                        route: base.route().unwrap(),
                        peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                        pulse: auditorylab_app::core::PulseSpec {
                            rate_hz: finite(7.0),
                            width: auditorylab_app::core::PulseWidth::Duration {
                                seconds: finite(0.01),
                            },
                            shape: auditorylab_app::core::PulseShape::Rectangular {},
                        },
                    },
                    modulation: base.modulation(),
                }
            },
            "pulse train 7 pulses/s · requested 10 ms fixed window · rectangular, peak -24 dBFS, \
                left channel",
        ),
        (
            {
                ComponentForm {
                    source: auditorylab_app::core::SoundSource::BinauralPair {
                        first_hz: pair.first_hz,
                        offset_hz: pair.offset_hz,
                        first_peak_dbfs: pair.first_peak_dbfs,
                        second_peak_dbfs: pair.second_peak_dbfs,
                    },
                    modulation: None,
                }
            },
            "binaural pair, first 200 Hz, beat offset -6 Hz, second 194 Hz, peaks -20 / -30 dBFS",
        ),
        (
            {
                let pair = Pair {
                    offset_hz: finite(6.0),
                    ..pair
                };
                ComponentForm {
                    source: auditorylab_app::core::SoundSource::MonauralPair {
                        route: ChannelRouting::Right,
                        first_hz: pair.first_hz,
                        offset_hz: pair.offset_hz,
                        first_peak_dbfs: pair.first_peak_dbfs,
                        second_peak_dbfs: pair.second_peak_dbfs,
                    },
                    modulation: None,
                }
            },
            "monaural pair, first 200 Hz, beat offset 6 Hz, second 206 Hz, peaks -20 / -30 dBFS",
        ),
        (
            {
                let base = plain;
                ComponentForm {
                    source: auditorylab_app::core::SoundSource::Tone {
                        frequency_hz: finite(-437.0),
                        route: base.route().unwrap(),
                        peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                    },
                    modulation: base.modulation(),
                }
            },
            "tone -437 Hz, peak -24 dBFS, left channel",
        ),
    ]
}
