//! Shared orientation wording; factual ranges remain owned by core.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
use auditorylab_app::{
    core::rhythm::Band,
    learning::{Construction, Topic},
};

pub const fn heading(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => "Start here",
        Topic::Library => "Meet the Library",
        Topic::PitchAndRhythm => "Pitch and rhythm",
        Topic::SoundTypes => "Six ways to construct sound",
        Topic::RhythmBands => "A spectrum of rhythms",
    }
}

pub const fn body(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => {
            "Explore sounds at your own pace. Browse Library to inspect real \
            recipes and their research context, or create a pulsed tone in \
            Workbench. This optional guide is always available from Start here. \
            Nothing plays or saves automatically.\n\nSet your listening volume \
            low before choosing Play. Auditory Lab does not measure sound \
            pressure or your brain state; these examples do not promise a \
            listening outcome."
        }
        Topic::Library => {
            "Built-ins adapt specific research conditions. Read each real Library \
            record for its population, sources, limitations and explicit \
            modifications. A represented condition may omit the rest of a trial \
            sequence. Its duration describes that source condition; it is not a \
            prescribed listening time.\n\nUser Library contains your saved \
            recipes. Open files are temporary until you explicitly add them. \
            Browse Library preserves your query and selection; choose Built-in \
            using the normal collection controls. Editing a study copy creates a \
            fresh identity and retains its lineage without claiming the original \
            evidence for your edits."
        }
        Topic::PitchAndRhythm => {
            "A carrier frequency and a repetition rate describe different things. \
            A pulsed tone keeps its carrier phase advancing while its amplitude \
            envelope repeats. The carrier frequency setting remains the same \
            when repetition changes; perceived pitch and timbre can still \
            change.\n\nIn Workbench, the real compiled preview describes the \
            actual sound and modulation envelope. A rectangular pulse is \
            rectangular; shaped pulse edges and sinusoidal amplitude modulation \
            have their own meanings. The band illustrations in this guide are \
            normalized sine schematics, not audio waveforms or measured EEG."
        }
        Topic::SoundTypes => {
            "Steady tone: an unmodulated carrier.\n\nPulsed tone (isochronic): a \
            carrier gated by a repeating pulse envelope, with rectangular or \
            shaped edges.\n\nSinusoidal amplitude modulation: a smooth periodic \
            change in carrier amplitude.\n\nBinaural pair: two tone frequencies \
            routed separately to left and right channels. Routing does not \
            verify separate-ear delivery.\n\nMonaural pair: two tones combined \
            on the same route; their interaction can create beating.\n\nPulse \
            train: repeated source pulses, distinct from gating a tone \
            carrier.\n\nNew sound starts empty and silent. Create a pulsed tone \
            opens a fresh example using the actual constructor settings in \
            the compiled comparison. Live Play runs until Stop; finite playback/export timing \
            remains editable in Workbench."
        }
        Topic::RhythmBands => {
            "These five app reference bands describe nominal rhythm rates. \
            Empirical EEG/MEG conventions vary. The illustrations show the same \
            one-second, zero-phase, normalized sine examples in the overview and \
            individual views. Select each named band for its cycles/s and context. \
            They are illustrations, not recorded EEG, rendered audio \
            or pulse envelopes.\n\nSelected EEG/MEG associations overlap and \
            depend on region, task and measurement. Delta occurs in slow-wave \
            sleep; theta and gamma have been observed in memory tasks; alpha and \
            beta appear in many resting and active contexts. This is no \
            alertness, intelligence or therapy ranking. Playing an audio rate \
            does not establish or predict a listener's brain state.\n\nSources: \
            IFCN glossary (2017, corrected 2019); Osipova et al., human memory \
            MEG observations (2006); NIH sleep-stage overview. The memory study \
            used 4.5–8.5 and 60–90 Hz, not these exact app intervals."
        }
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
    let visual = construction.visual();
    let facts = visual
        .facts
        .iter()
        .map(crate::voice_details)
        .collect::<Vec<_>>()
        .join("\n");
    let meaning = match construction {
        Construction::SteadyTone => "Continuous carrier; constant amplitude envelope.",
        Construction::PulsedTone => "The amplitude envelope repeats while carrier phase continues.",
        Construction::SinusoidalAm => "A smooth amplitude envelope scales the continuous carrier.",
        Construction::BinauralPair => "Separate left/right carriers; routing does not verify ears.",
        Construction::MonauralPair => {
            "Two same-route carriers interfere; inspect the combined trace."
        }
        Construction::PulseTrain => "Repeated source pulses, with no tone carrier substituted.",
    };
    format!(
        "{meaning}\n{facts}\nStatic compiled example at {} Hz. Stereo waveform: {} ms; \
        carrier/source comparison: {} ms. No device or playback. Envelope columns use \
        the actual compiled model, not normalized rhythm-band sine illustrations.",
        visual.timing.rate.hz(),
        crate::compact_number(visual.waveform_seconds * 1000.0),
        crate::compact_number(visual.carrier_seconds * 1000.0)
    )
}

pub fn band_description(band: Band) -> String {
    let example = auditorylab_app::learning::band_examples()
        .into_iter()
        .find(|example| example.band == band)
        .unwrap_or_else(|| unreachable!("all core bands have a teaching example"));
    let context = match band {
        Band::Delta => "Deep NREM sleep; slow-wave activity.",
        Band::Theta => "Drowsiness; also waking memory tasks.",
        Band::Alpha => "Quiet wakefulness, especially eyes closed.",
        Band::Beta => "Waking activity; fronto-central EEG activity.",
        Band::Gamma => "Local processing; engaged networks.",
    };
    format!(
        "{} · {}\nIllustrative {} rhythm · {} cycles/s · one second\nZero phase; \
        normalized signed amplitude −1 to 1. This sine diagram is not EEG, audio \
        or a pulse envelope.\n{context}",
        band,
        band.interval(),
        band,
        example.example_hz
    )
}

pub fn carrier_caption(visual: &auditorylab_app::learning::TeachingVisual) -> String {
    format!(
        "Same source without modulation · {} ms · {} Hz · common stereo amplitude fit",
        crate::compact_number(visual.carrier_seconds * 1000.0),
        visual.timing.rate.hz()
    )
}
pub fn waveform_caption(visual: &auditorylab_app::learning::TeachingVisual) -> String {
    format!(
        "Actual constructed sound · {} ms · {} Hz · common stereo amplitude fit",
        crate::compact_number(visual.waveform_seconds * 1000.0),
        visual.timing.rate.hz()
    )
}
pub fn timing_caption(visual: &auditorylab_app::learning::TeachingVisual) -> String {
    format!(
        "Actual source/modulation envelope · {} ms from phase origin · {} Hz · gain 0–1",
        crate::compact_number(visual.timing.rate.seconds_in(visual.timing.frames) * 1000.0),
        visual.timing.rate.hz()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn teaching_copy_describes_actual_compiled_example_without_regenerating_identity() {
        for topic in Topic::ALL {
            assert_eq!(Topic::ALL[topic.index()], topic);
            assert_ne!(heading(topic), "");
            assert_ne!(body(topic), "");
            assert_eq!(
                Topic::ALL
                    .iter()
                    .filter(|other| other.key() == topic.key())
                    .count(),
                1
            );
        }
        for construction in Construction::ALL {
            let visual = construction.visual();
            let text = construction_description(construction);
            assert_ne!(construction_heading(construction), "");
            for facts in &visual.facts {
                assert!(text.contains(&crate::voice_details(facts)));
            }
            assert!(std::ptr::eq(visual, construction.visual()));
            assert!(carrier_caption(visual).contains("10 ms"));
            assert!(waveform_caption(visual).contains("100 ms"));
            assert!(timing_caption(visual).contains("gain 0–1"));
        }
        let pitch = body(Topic::PitchAndRhythm);
        assert!(pitch.contains("phase advancing"));
        assert!(pitch.contains("envelope repeats"));
        assert!(!pitch.contains("carrier, repeated"));
        assert!(
            !pitch.contains("440"),
            "exact values belong to the compiled facts"
        );
    }
}
