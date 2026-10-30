//! Plain-language lessons and optional technical context; core owns sound facts.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
use auditorylab_app::{
    core::rhythm::Band,
    learning::{Construction, Topic},
};

pub const fn heading(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => "Start here",
        Topic::Library => "Library guide",
        Topic::PitchAndRhythm => "Pitch and rhythm",
        Topic::SoundTypes => "Sound constructions",
        Topic::RhythmBands => "A spectrum of rhythms",
    }
}

pub const fn source_label(source: auditorylab_app::learning::LearningSource) -> &'static str {
    use auditorylab_app::learning::LearningSource;
    match source {
        LearningSource::EegGlossary => "IFCN EEG glossary",
        LearningSource::MemoryStudy => "Memory study (PubMed)",
        LearningSource::SleepStages => "NIH sleep stages",
    }
}

pub const fn navigation_label(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => "Overview",
        Topic::Library => "Library guide",
        Topic::PitchAndRhythm => "Pitch and rhythm",
        Topic::SoundTypes => "Sound types",
        Topic::RhythmBands => "Rhythm ranges",
    }
}

pub const fn body(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => {
            "Explore a sound in Library, build one in Workbench, or learn the ideas \
            behind them here. Nothing plays or saves automatically. Set your listening \
            volume low before choosing Play."
        }
        Topic::Library => {
            "Built-in sounds are examples adapted from research. Open a sound to read \
            what was studied and how the example differs. Its duration describes that \
            example, not a recommended listening time. Your own saved sounds live in \
            User Library."
        }
        Topic::PitchAndRhythm => {
            "Carrier frequency describes the tone; repetition rate describes its pulses. \
            These close-ups use different time windows."
        }
        Topic::SoundTypes => {
            "Choose how to make a sound, then open its example in Workbench to edit. \
            Playback and saving remain your choice."
        }
        Topic::RhythmBands => {
            "Delta, theta, alpha, beta and gamma are names for rhythm ranges. Each \
            section below shows one second at an example rate. These are \
            illustrations, not recorded brain activity; playing a matching audio rate \
            does not establish a brain state or promise an effect."
        }
    }
}

pub fn details(topic: Topic, construction: Construction) -> String {
    match topic {
        Topic::Overview => {
            "Library is for finding and inspecting sounds. Workbench edits one sound at \
            a time. This guide is optional and remembers your current lesson during \
            this session. Auditory Lab does not measure sound pressure or brain \
            activity."
                .into()
        }
        Topic::Library => "Read each Library record for its population, sources, limitations and \
            explicit adaptations. An example may represent one condition rather than \
            an entire trial sequence. Open files are temporary until explicitly added \
            to User Library. Editing a copy retains source lineage; the original \
            evidence does not automatically apply to your changes."
            .into(),
        Topic::PitchAndRhythm => format!(
            "{}\n\nThe carrier phase keeps advancing while the amplitude envelope \
            repeats. Changing repetition leaves the carrier-frequency setting \
            unchanged, but perceived pitch and timbre can still change. The \
            illustrated envelope comes from the sound model, including its shaped \
            edges.",
            construction_details(Construction::PulsedTone)
        ),
        Topic::SoundTypes => construction_details(construction),
        Topic::RhythmBands => "The curves are one-second, zero-phase sine schematics with equal \
            normalized amplitude. They are not audio waveforms or pulse envelopes. \
            EEG/MEG band conventions vary; the ranges here follow the app’s IFCN \
            reference. Associations overlap and depend on region, task and \
            measurement. This is not an alertness, intelligence or therapy \
            ranking.\n\nSources: IFCN glossary (2017, corrected 2019); Osipova et al., \
            human memory MEG observations (2006); NIH sleep-stage overview. The memory \
            study used 4.5–8.5 and 60–90 Hz, not these exact app intervals. See the \
            source buttons below for the original references."
            .into(),
    }
}

pub const fn construction_heading(construction: Construction) -> &'static str {
    match construction {
        Construction::SteadyTone => "Steady tone",
        Construction::PulsedTone => "Pulsed tone (isochronic)",
        Construction::SinusoidalAm => "Sinusoidal amplitude modulation",
        Construction::BinauralPair => "Binaural pair",
        Construction::MonauralPair => "Monaural pair",
        Construction::PulseTrain => "Pulse train",
    }
}

pub fn construction_description(construction: Construction) -> String {
    match construction {
        Construction::SteadyTone => "A continuous tone with a constant amplitude envelope.",
        Construction::PulsedTone => {
            "A continuous tone made into regular pulses by a repeating amplitude \
            envelope."
        }
        Construction::SinusoidalAm => {
            "A tone whose signal level rises and falls smoothly in a repeating cycle."
        }
        Construction::BinauralPair => {
            "Two different tones sent separately to the left and right channels. \
            Software cannot verify that they reach separate ears."
        }
        Construction::MonauralPair => {
            "Two tones combined on the same route. Their interference can create a \
            beating pattern."
        }
        Construction::PulseTrain => {
            "A series of source pulses, rather than a continuous tone switched on and \
            off."
        }
    }
    .into()
}

pub fn construction_details(construction: Construction) -> String {
    let visual = construction.visual();
    let facts = visual
        .facts
        .iter()
        .map(crate::voice_details)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{facts}\nSample rate: {} kHz. Sound close-up: {} ms. These static plots use \
            the compiled construction; no playback is needed. Waveform height uses \
            one fitted signed scale for both channels: zero is the center line, \
            and height does not show digital full scale or listening volume. \
            Source/modulation gain is nonnegative: zero is off; one retains the \
            original source level.",
        crate::compact_number(f64::from(visual.timing.rate.hz()) / 1000.0),
        crate::compact_number(visual.waveform_seconds * 1000.0)
    )
}

pub const fn band_context(band: Band) -> &'static str {
    match band {
        Band::Delta => "Deep NREM sleep; slow-wave activity.",
        Band::Theta => "Drowsiness; also waking memory tasks.",
        Band::Alpha => "Quiet wakefulness, especially eyes closed.",
        Band::Beta => "Waking activity; fronto-central EEG activity.",
        Band::Gamma => "Local processing; engaged networks.",
    }
}

pub fn band_description(band: Band) -> String {
    let example = auditorylab_app::learning::band_examples()
        .into_iter()
        .find(|example| example.band == band)
        .unwrap_or_else(|| unreachable!("all core bands have a teaching example"));
    format!(
        "{} · {}\nExample: {} cycles per second. Observed EEG/MEG context: {}",
        band,
        band.interval(),
        example.example_hz,
        band_context(band)
    )
}

/// Primary example quantities come from the compiled constructor facts.
pub fn example_summary(construction: Construction) -> String {
    use auditorylab_app::core::{CarrierFacts, ModulationFacts};
    let visual = construction.visual();
    let facts = visual
        .facts
        .first()
        .unwrap_or_else(|| unreachable!("examples have a voice"));
    if let Some(pair) = facts.pair {
        return format!(
            "Example settings: {} Hz and {} Hz tones · {} Hz frequency difference",
            crate::compact_number(pair.first_hz),
            crate::compact_number(pair.second_hz),
            crate::compact_number(pair.offset_hz.abs())
        );
    }
    let source = match facts.carrier {
        CarrierFacts::Tone(rate) => format!(
            "{} Hz tone carrier",
            crate::compact_number(rate.requested_hz)
        ),
        CarrierFacts::PulseTrain(pulse) => {
            format!("{} source pulses/s", crate::compact_number(pulse.rate_hz))
        }
    };
    let modulation = match facts.modulation {
        Some(ModulationFacts::Pulsed(pulse)) => {
            format!(" · {} pulses/s", crate::compact_number(pulse.rate_hz))
        }
        Some(ModulationFacts::Sinusoidal { rate, .. }) => format!(
            " · amplitude modulation {} cycles/s",
            crate::compact_number(rate.requested_hz)
        ),
        None => String::new(),
    };
    format!("Example settings: {source}{modulation}")
}

pub fn carrier_caption(visual: &auditorylab_app::learning::TeachingVisual) -> String {
    format!(
        "The tone before pulsing · first {} ms",
        crate::compact_number(visual.carrier_seconds * 1000.0)
    )
}
pub fn waveform_caption(visual: &auditorylab_app::learning::TeachingVisual) -> String {
    format!(
        "What you hear · first {} ms",
        crate::compact_number(visual.waveform_seconds * 1000.0)
    )
}
/// Keep the producer's physical model meaning when selecting a plot caption.
pub fn timing_caption(
    visual: &auditorylab_app::learning::TeachingVisual,
    kind: auditorylab_app::core::timing::ModelKind,
) -> String {
    use auditorylab_app::core::timing::ModelKind;
    let meaning = match kind {
        ModelKind::Steady => "Tone level",
        ModelKind::PulseTrain => "When each source pulse is on",
        ModelKind::Pulsed => "How the pulses switch the tone on and off",
        ModelKind::Envelope => "How the tone's level rises and falls",
    };
    let scale = if kind == ModelKind::PulseTrain {
        "bottom off, top on"
    } else {
        "bottom silent, top the tone's full level"
    };
    format!(
        "{meaning} · {} ms · {scale}",
        crate::compact_number(visual.timing.rate.seconds_in(visual.timing.frames) * 1000.0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_facts_and_timing_captions_preserve_constructor_meaning() {
        use auditorylab_app::core::timing::ModelKind;
        let pulsed = Construction::PulsedTone.visual();
        assert!(
            example_summary(Construction::PulsedTone).contains("440 Hz tone carrier · 10 pulses/s")
        );
        assert!(example_summary(Construction::PulseTrain).contains("40 source pulses/s"));
        assert!(example_summary(Construction::MonauralPair).contains("200 Hz and 206 Hz tones"));
        assert!(
            timing_caption(pulsed, ModelKind::Pulsed)
                .contains("How the pulses switch the tone on and off")
        );
        assert!(
            timing_caption(Construction::PulseTrain.visual(), ModelKind::PulseTrain)
                .contains("When each source pulse is on")
        );
        assert!(
            timing_caption(Construction::SinusoidalAm.visual(), ModelKind::Envelope)
                .contains("How the tone's level rises and falls")
        );
        assert!(waveform_caption(pulsed).starts_with("What you hear"));
        assert!(!construction_details(Construction::PulseTrain).contains("carrier close-up"));
        assert_eq!(navigation_label(Topic::Library), "Library guide");
        for source in auditorylab_app::learning::LearningSource::ALL {
            assert_ne!(source_label(source), "");
            assert!(source.url().starts_with("https://"));
        }
    }

    #[test]
    fn teaching_keeps_technical_quantities_labelled_and_out_of_primary_copy() {
        for topic in Topic::ALL {
            assert_ne!(heading(topic), "");
            assert_ne!(body(topic), "");
            assert!(!body(topic).contains("48000"));
        }
        for construction in Construction::ALL {
            let visual = construction.visual();
            let text = construction_details(construction);
            assert!(text.contains("Sample rate: 48 kHz"));
            for facts in &visual.facts {
                assert!(text.contains(&crate::voice_details(facts)));
            }
            assert!(!construction_description(construction).contains("sample rate"));
            assert!(carrier_caption(visual).contains("10 ms"));
            assert!(waveform_caption(visual).contains("100 ms"));
        }
        assert!(
            details(Topic::PitchAndRhythm, Construction::SteadyTone)
                .contains("phase keeps advancing")
        );
        assert!(details(Topic::RhythmBands, Construction::SteadyTone).contains("not an alertness"));
    }
}
