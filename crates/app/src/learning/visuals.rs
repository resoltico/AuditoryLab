//! Pure, cached teaching projections from actual constructors and compiled synthesis.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
use crate::core::{self, ComponentForm, VoiceFacts, waveform::Extent};
use crate::document::construction::{CreationKind, default_modulation};
use std::sync::LazyLock;

/// The six sound mechanisms taught here; this is not a second creation menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construction {
    SteadyTone,
    PulsedTone,
    SinusoidalAm,
    BinauralPair,
    MonauralPair,
    PulseTrain,
}
impl Construction {
    pub const ALL: [Self; 6] = [
        Self::SteadyTone,
        Self::PulsedTone,
        Self::SinusoidalAm,
        Self::BinauralPair,
        Self::MonauralPair,
        Self::PulseTrain,
    ];
    pub const fn key(self) -> &'static str {
        match self {
            Self::SteadyTone => "steady-tone",
            Self::PulsedTone => "pulsed-tone",
            Self::SinusoidalAm => "sinusoidal-am",
            Self::BinauralPair => "binaural-pair",
            Self::MonauralPair => "monaural-pair",
            Self::PulseTrain => "pulse-train",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|construction| *construction == self)
            .unwrap_or_else(|| unreachable!("every construction has an order"))
    }
    /// Reuse authored defaults; no recipe/UUID, device, save or task is created.
    pub fn form(self) -> ComponentForm {
        match self {
            Self::SteadyTone => CreationKind::Tone.component(),
            Self::PulsedTone => CreationKind::PulsedTone.component(),
            Self::SinusoidalAm => {
                let mut form = CreationKind::Tone.component();
                form.modulation = Some(default_modulation(core::ModulationKind::Sinusoidal));
                form
            }
            Self::BinauralPair => CreationKind::BinauralPair.component(),
            Self::MonauralPair => CreationKind::MonauralPair.component(),
            Self::PulseTrain => CreationKind::PulseTrain.component(),
        }
    }
    /// Capture a fresh complete document from exactly the illustrated construction.
    pub fn recipe(self) -> Result<core::Recipe, crate::services::OperationError> {
        let mut draft = crate::document::editing::Draft::new()?;
        draft
            .add_node(self.form())
            .unwrap_or_else(|_| unreachable!("one example fits an empty draft"));
        Ok(draft.recipe()?)
    }

    /// Bounded origin-zero projections compiled once; frontends only choose geometry.
    pub fn visual(self) -> &'static TeachingVisual {
        static VISUALS: LazyLock<Vec<TeachingVisual>> = LazyLock::new(|| {
            Construction::ALL
                .into_iter()
                .map(TeachingVisual::compile)
                .collect()
        });
        &VISUALS[self.index()]
    }
}

/// All-sample stereo waveform extrema and real compiled envelope columns.
#[derive(Debug)]
pub struct TeachingVisual {
    pub form: ComponentForm,
    /// Same source with modulation removed, over ten milliseconds.
    pub carrier: Vec<[Extent; 2]>,
    /// Actual construction with modulation, over one hundred milliseconds.
    pub waveform: Vec<[Extent; 2]>,
    pub carrier_seconds: f64,
    pub waveform_seconds: f64,
    /// Core's frame-quantized source/modulation-envelope model, anchored at zero.
    pub timing: core::timing::TimingModel,
    pub facts: Vec<VoiceFacts>,
}
impl TeachingVisual {
    fn compile(construction: Construction) -> Self {
        let form = construction.form();
        let target = compile(form);
        let mut unmodulated = form;
        unmodulated.modulation = None;
        let carrier_target = compile(unmodulated);
        let carrier_signal = carrier_target
            .signal(1.0, 0.0)
            .unwrap_or_else(|_| unreachable!("one-second zero-fade visual is valid"));
        let signal = target
            .signal(1.0, 0.0)
            .unwrap_or_else(|_| unreachable!("one-second zero-fade visual is valid"));
        let mut carrier = Vec::new();
        let mut waveform = Vec::new();
        let rate = core::SampleRate::DEFAULT;
        let carrier_frames = u64::from(rate.hz()) / 100;
        let waveform_frames = u64::from(rate.hz()) / 10;
        // Match the teaching plot's 1024-unit horizontal extent. Every sample
        // still contributes to an extrema bin; short spans stop at one bin/frame.
        core::waveform::trace(&carrier_signal, 0, carrier_frames, 1024, &mut carrier);
        core::waveform::trace(&signal, 0, waveform_frames, 1024, &mut waveform);
        Self {
            form,
            carrier,
            waveform,
            carrier_seconds: rate.seconds_in(carrier_frames),
            waveform_seconds: rate.seconds_in(waveform_frames),
            timing: target.timing_model(256),
            facts: target.voices().map(|(_, facts)| *facts).collect(),
        }
    }
}
fn compile(form: ComponentForm) -> core::Target {
    let sound = core::Sound::new(vec![core::Component {
        id: core::ComponentId::new(1).unwrap_or_else(|| unreachable!("positive owner identity")),
        form,
    }])
    .unwrap_or_else(|_| unreachable!("one constructed component fits a sound"));
    sound
        .check(core::SampleRate::DEFAULT)
        .into_result()
        .unwrap_or_else(|_| unreachable!("teaching constructors compile at 48 kHz"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CarrierFacts, ChannelRouting, ModulationFacts, timing::ModelKind};
    #[test]
    fn six_visuals_are_cached_actual_constructions_without_recipe_identity() {
        for construction in Construction::ALL {
            let visual = construction.visual();
            assert!(std::ptr::eq(visual, construction.visual()));
            assert_eq!(visual.form, construction.form());
            assert_eq!(visual.carrier.len(), 480);
            assert_eq!(visual.waveform.len(), 1024);
            assert_eq!(visual.carrier_seconds, 0.01);
            assert_eq!(visual.waveform_seconds, 0.1);
            assert_eq!(visual.timing.rate.hz(), 48_000);
            assert!(
                visual
                    .carrier
                    .iter()
                    .chain(&visual.waveform)
                    .flatten()
                    .all(|bin| bin.min.is_finite() && bin.max.is_finite() && bin.min <= bin.max)
            );
        }
        let tone = Construction::SteadyTone.visual();
        assert_eq!(tone.timing.lanes.len(), 1);
        assert!(
            tone.timing.lanes[0]
                .columns
                .iter()
                .all(|bin| *bin == Extent { min: 1.0, max: 1.0 })
        );
        let pulse = Construction::PulsedTone.visual();
        assert_eq!(pulse.timing.lanes[0].kind, ModelKind::Pulsed);
        assert!(
            pulse.timing.lanes[0]
                .columns
                .iter()
                .any(|bin| bin.max == 0.0)
        );
        assert!(
            pulse.timing.lanes[0]
                .columns
                .iter()
                .any(|bin| bin.min == 1.0)
        );
        assert!(
            pulse.timing.lanes[0]
                .columns
                .iter()
                .any(|bin| bin.max > 0.0 && bin.max < 1.0)
        );
        assert_eq!(
            pulse.carrier, tone.carrier,
            "phase-continuous tone carrier comparison"
        );
        assert_ne!(
            pulse.waveform, tone.waveform,
            "real gating must remain visible"
        );
        assert!(matches!(pulse.facts[0].carrier,CarrierFacts::Tone(rate)
            if rate.requested_hz == 440.0));
        assert!(
            matches!(pulse.facts[0].modulation,Some(ModulationFacts::Pulsed(facts))
            if facts.rate_hz == 10.0)
        );
        let am = Construction::SinusoidalAm.visual();
        assert_eq!(am.timing.lanes[0].kind, ModelKind::Envelope);
        assert_ne!(am.timing.lanes[0].columns, pulse.timing.lanes[0].columns);
        let train = Construction::PulseTrain.visual();
        assert_eq!(train.timing.lanes[0].kind, ModelKind::PulseTrain);
        assert!(
            train.timing.lanes[0]
                .columns
                .iter()
                .all(|bin| [0.0, 1.0].contains(&bin.min) && [0.0, 1.0].contains(&bin.max)),
            "rectangular source retains actual rectangular envelope bounds"
        );
        let binaural = Construction::BinauralPair.visual();
        assert_eq!(binaural.facts.len(), 2);
        assert_eq!(binaural.facts[0].route, ChannelRouting::Left);
        assert_eq!(binaural.facts[1].route, ChannelRouting::Right);
        assert!(binaural.waveform.iter().any(|bin| bin[0] != bin[1]));
        let monaural = Construction::MonauralPair.visual();
        assert_eq!(monaural.facts.len(), 2);
        assert!(monaural.waveform.iter().all(|bin| bin[0] == bin[1]));
    }
}
