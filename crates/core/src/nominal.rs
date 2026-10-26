// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Sample-rate-independent structural rhythm classification.
//!
//! These reference bands describe authored audio schedules, never measured EEG
//! or a promised percept. Carrier frequencies and endpoint fades are excluded.
use crate::{Carrier, ComponentId, Ears, Modulation, Voice};
use alloc::{string::String, vec::Vec};
use core::fmt::{self, Display, Formatter};
use serde::Serialize;

/// The disclosed band convention, independent of a study's own terminology.
pub const CONVENTION: &str = "ifcn-2017-corrected-2019";
/// Revision of the nominal interpretation rules.
pub const CLASSIFIER_REVISION: u32 = 1;

/// One reference interval, with intentionally nonuniform endpoint inclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    /// 0.1 Hz through strictly below 4 Hz.
    Delta,
    /// 4 Hz through strictly below 8 Hz.
    Theta,
    /// 8 Hz through 13 Hz inclusive.
    Alpha,
    /// Strictly above 13 Hz through 30 Hz inclusive.
    Beta,
    /// Strictly above 30 Hz through 80 Hz inclusive.
    Gamma,
}
impl Band {
    /// Reference bands in ascending frequency order.
    pub const ALL: [Self; 5] = [
        Self::Delta,
        Self::Theta,
        Self::Alpha,
        Self::Beta,
        Self::Gamma,
    ];
    /// Classify a positive finite nominal rate; outside rates have no band.
    #[must_use]
    pub fn of(hz: f64) -> Option<Self> {
        if !hz.is_finite() || hz < 0.1 {
            None
        } else if hz < 4.0 {
            Some(Self::Delta)
        } else if hz < 8.0 {
            Some(Self::Theta)
        } else if hz <= 13.0 {
            Some(Self::Alpha)
        } else if hz <= 30.0 {
            Some(Self::Beta)
        } else if hz <= 80.0 {
            Some(Self::Gamma)
        } else {
            None
        }
    }
    /// Human-readable interval in the shared reference convention.
    #[must_use]
    pub const fn interval(self) -> &'static str {
        match self {
            Self::Delta => "0.1–<4 Hz",
            Self::Theta => "4–<8 Hz",
            Self::Alpha => "8–13 Hz",
            Self::Beta => ">13–30 Hz",
            Self::Gamma => ">30–80 Hz",
        }
    }
    /// Stable lower-case query spelling.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::Theta => "theta",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Gamma => "gamma",
        }
    }
}
impl Display for Band {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Delta => "Delta",
            Self::Theta => "Theta",
            Self::Alpha => "Alpha",
            Self::Beta => "Beta",
            Self::Gamma => "Gamma",
        })
    }
}
impl core::str::FromStr for Band {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|b| b.key() == text)
            .ok_or("expected delta, theta, alpha, beta or gamma")
    }
}

/// An attributable structural method; steady tones have no rhythmic descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Separate-ear two-tone construction.
    Binaural,
    /// Same-route two-tone interference, including unequal levels.
    Monaural,
    /// Intrinsic rectangular click schedule.
    Clicks,
    /// Periodic amplitude gating of a carrier.
    Gate,
    /// Positive-depth sinusoidal amplitude modulation.
    Am,
    /// Effectively unmodulated tone, with no rhythm inferred from its carrier.
    SteadyTone,
}
impl Method {
    /// A short display name for the structural process.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Binaural => "interaural pair",
            Self::Monaural => "monaural pair",
            Self::Clicks => "clicks",
            Self::Gate => "gate",
            Self::Am => "AM",
            Self::SteadyTone => "steady tone",
        }
    }
    pub(crate) const fn of_pair(kind: crate::PairKind) -> Self {
        match kind {
            crate::PairKind::Binaural => Self::Binaural,
            crate::PairKind::Monaural => Self::Monaural,
        }
    }
    pub(crate) fn pair_routes(first: [bool; 2], second: [bool; 2]) -> Option<Self> {
        match (first, second) {
            ([true, false], [false, true]) | ([false, true], [true, false]) => Some(Self::Binaural),
            (a, b) if a == b && a != [false, false] => Some(Self::Monaural),
            _ => None,
        }
    }
    /// Supported query vocabulary.
    pub const ALL: [Self; 6] = [
        Self::Binaural,
        Self::Monaural,
        Self::Clicks,
        Self::Gate,
        Self::Am,
        Self::SteadyTone,
    ];
    /// Stable command-line spelling.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Binaural => "binaural",
            Self::Monaural => "monaural",
            Self::Clicks => "clicks",
            Self::Gate => "gate",
            Self::Am => "am",
            Self::SteadyTone => "steady-tone",
        }
    }
}
impl core::str::FromStr for Method {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|m| m.key() == text)
            .ok_or("unknown structural method")
    }
}

/// How a rhythm was attributed, independent of acoustic validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derivation {
    /// An intrinsic authored repetition/modulation schedule.
    Explicit,
    /// The whole recipe has exactly two eligible tones with unambiguous routing.
    NarrowInference,
}
/// Interpretation coverage; this is not exhaustive spectral analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    /// Every supported authored process was interpreted without ambiguity.
    Complete,
    /// Known descriptors retained beside an ambiguous raw-tone relationship.
    Partial,
    /// Nonfinite or invalid intrinsic settings require correction.
    Invalid,
}
/// One independent positive rhythm; equal rates remain separate descriptors.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    /// First owner in authored order.
    pub owner: ComponentId,
    /// Second owner for a narrow raw-tone relationship.
    pub related: Option<ComponentId>,
    /// Structural mechanism.
    pub method: Method,
    /// Positive finite authored rate in hertz, without display rounding.
    pub hz: f64,
    /// Membership under [`CONVENTION`], or an outside rate.
    pub band: Option<Band>,
    /// Attribution rule.
    pub derivation: Derivation,
}
/// Nominal classification that can be computed without compilation or rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Independently attributable processes in authored order.
    pub rhythms: Vec<Descriptor>,
    /// Deduplicated methods, including nonrhythmic steady tones.
    pub methods: Vec<Method>,
    /// Deduplicated bands in reference order.
    pub bands: Vec<Band>,
    /// Coverage of this structural interpretation.
    pub completeness: Completeness,
    /// Problems in authored settings, never target-sample-rate errors.
    pub diagnostics: Vec<String>,
}
impl Analysis {
    /// At least one recognized rate outside the five reference bands.
    #[must_use]
    pub fn outside_bands(&self) -> bool {
        self.rhythms.iter().any(|r| r.band.is_none())
    }
    /// No rhythm is asserted only for complete, unambiguous analysis.
    #[must_use]
    pub fn no_rhythm(&self) -> bool {
        self.completeness == Completeness::Complete && self.rhythms.is_empty()
    }
    /// More than one process, even if their bands or rates coincide.
    #[must_use]
    pub fn multiple(&self) -> bool {
        self.rhythms.len() > 1
    }
    /// Correlated facets: one descriptor must satisfy every active restriction.
    /// Values within a facet are alternatives. A steady tone cannot borrow a band.
    #[must_use]
    pub fn matches(&self, bands: &[Band], methods: &[Method]) -> bool {
        if bands.is_empty() && methods.is_empty() {
            return true;
        }
        if bands.is_empty() {
            return self.methods.iter().any(|m| methods.contains(m));
        }
        self.rhythms.iter().any(|r| {
            r.band.is_some_and(|b| bands.contains(&b))
                && (methods.is_empty() || methods.contains(&r.method))
        })
    }
    /// Smallest descriptor satisfying the active correlated restrictions.
    #[must_use]
    pub fn matching_rate(&self, bands: &[Band], methods: &[Method]) -> Option<f64> {
        self.rhythms
            .iter()
            .filter(|r| {
                (bands.is_empty() || r.band.is_some_and(|b| bands.contains(&b)))
                    && (methods.is_empty() || methods.contains(&r.method))
            })
            .map(|r| r.hz)
            .reduce(f64::min)
    }
    fn add(
        &mut self,
        owner: ComponentId,
        related: Option<ComponentId>,
        method: Method,
        hz: f64,
        derivation: Derivation,
    ) {
        if !hz.is_finite() || hz <= 0.0 {
            self.invalid("rhythmic rates must be finite and positive");
            return;
        }
        self.methods.push(method);
        self.rhythms.push(Descriptor {
            owner,
            related,
            method,
            hz,
            band: Band::of(hz),
            derivation,
        });
    }
    fn invalid(&mut self, diagnostic: &str) {
        self.completeness = Completeness::Invalid;
        self.diagnostics.push(diagnostic.into());
    }
}

fn unmodulated(component: &Voice) -> bool {
    matches!(
        component.modulation,
        None | Some(Modulation::Am { depth: 0.0, .. })
    )
}
/// Analyze authored nodes, retaining signed-offset intent in arbitrary mixtures.
/// The explicit offset is the rate authority even when carrier addition cannot
/// represent it. Such a document is classified and marked for review, not played.
#[must_use]
pub fn analyze(components: &[crate::Component]) -> Analysis {
    use crate::{ComponentForm, authored};
    let mut analysis = Analysis {
        rhythms: Vec::new(),
        methods: Vec::new(),
        bands: Vec::new(),
        completeness: Completeness::Complete,
        diagnostics: Vec::new(),
    };
    let mut tones = Vec::new();
    for component in components {
        match component.form {
            ComponentForm::Voice(value) => {
                let id = component.id;
                let value = &value;
                if !value.peak_dbfs.is_finite() || value.peak_dbfs > 0.0 {
                    analysis.invalid("component peak must be finite and at most 0 dBFS");
                }
                match value.carrier {
                    Carrier::Clicks {
                        rate_hz,
                        width_seconds,
                    } => {
                        analysis.add(id, None, Method::Clicks, rate_hz, Derivation::Explicit);
                        if !width_seconds.is_finite() || width_seconds <= 0.0 {
                            analysis.invalid("click width must be finite and positive");
                        }
                    }
                    Carrier::Tone { frequency_hz } => {
                        if !frequency_hz.is_finite() || frequency_hz <= 0.0 {
                            analysis.invalid("tone carrier must be finite and positive");
                        }
                        if unmodulated(value) {
                            analysis.methods.push(Method::SteadyTone);
                            tones.push((id, *value, frequency_hz));
                        }
                    }
                }
                match value.modulation {
                    Some(Modulation::Am { rate_hz, depth }) => {
                        if !rate_hz.is_finite()
                            || rate_hz <= 0.0
                            || !depth.is_finite()
                            || !(0.0..=1.0).contains(&depth)
                        {
                            analysis.invalid(
                                "AM requires a finite positive rate and depth from 0 to 1",
                            );
                        }
                        if depth > 0.0 {
                            analysis.add(id, None, Method::Am, rate_hz, Derivation::Explicit);
                        }
                    }
                    Some(Modulation::Gate {
                        rate_hz,
                        width_seconds,
                    }) => {
                        analysis.add(id, None, Method::Gate, rate_hz, Derivation::Explicit);
                        if !width_seconds.is_finite() || width_seconds <= 0.0 {
                            analysis.invalid("gate width must be finite and positive");
                        }
                    }
                    None => {}
                }
            }
            form => {
                let (pair, method) = match form {
                    ComponentForm::BinauralPair(pair) => {
                        (pair, Method::of_pair(crate::PairKind::Binaural))
                    }
                    ComponentForm::MonauralPair { pair, .. } => {
                        (pair, Method::of_pair(crate::PairKind::Monaural))
                    }
                    ComponentForm::Voice(_) => unreachable!("raw form handled above"),
                };
                analysis.methods.push(method);
                if pair.offset_hz != 0.0 {
                    analysis.add(
                        component.id,
                        None,
                        method,
                        pair.offset_hz.abs(),
                        Derivation::Explicit,
                    );
                }
                if !pair.first_peak_dbfs.is_finite()
                    || !pair.second_peak_dbfs.is_finite()
                    || pair.first_peak_dbfs > 0.0
                    || pair.second_peak_dbfs > 0.0
                {
                    analysis.invalid("pair levels must be finite and at most 0 dBFS");
                }
                if let Err(error) = authored::lower(core::slice::from_ref(component)) {
                    analysis.invalid(&alloc::format!("{error}"));
                }
            }
        }
    }
    let relationship = if components.len() == 2 && tones.len() == 2 {
        let a = tones[0];
        let b = tones[1];
        let routes = |ears| match ears {
            Ears::Left => [true, false],
            Ears::Right => [false, true],
            Ears::Both => [true, true],
        };
        let method = Method::pair_routes(routes(a.1.ears), routes(b.1.ears));
        if let Some(method) = method {
            analysis.methods.retain(|m| *m != Method::SteadyTone);
            analysis.methods.push(method);
            if a.2 != b.2 {
                analysis.add(
                    a.0,
                    Some(b.0),
                    method,
                    (a.2 - b.2).abs(),
                    Derivation::NarrowInference,
                );
            }
        }
        method.is_some()
    } else {
        false
    };
    if tones.len() > 1 && !relationship && analysis.completeness != Completeness::Invalid {
        analysis.completeness = Completeness::Partial;
        analysis
            .diagnostics
            .push("ambiguous raw-tone mixture; no arbitrary pairwise differences inferred".into());
    }
    if let Err(error) = authored::lower(components)
        && !analysis
            .diagnostics
            .iter()
            .any(|d| *d == alloc::format!("{error}"))
    {
        analysis.invalid(&alloc::format!("{error}"));
    }
    analysis.methods.sort();
    analysis.methods.dedup();
    analysis.bands = Band::ALL
        .into_iter()
        .filter(|band| analysis.rhythms.iter().any(|r| r.band == Some(*band)))
        .collect();
    analysis
}
