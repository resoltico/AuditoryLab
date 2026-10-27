//! Shared structural rhythm rules for authored content and realized generators.
use crate::{
    Carrier, CarrierFacts, Lane, Modulation, ModulationFacts, Observation, PairError, PairKind,
    ParameterId, SlotRole, Target, VoiceId, VoiceObservation,
    validity::{self, ValueIssue},
};
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

/// The disclosed band convention, independent of a study's own terminology.
pub const CONVENTION: &str = "ifcn-2017-corrected-2019";
/// Revision of the nominal interpretation rules.
pub const CLASSIFIER_REVISION: u32 = 2;

/// One reference interval, with intentionally nonuniform endpoint inclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
}

impl Display for Band {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// An attributable structural method; steady tones have no rhythmic descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// One heading for a form; rate facts may still contain several methods.
    #[must_use]
    pub fn primary_of(form: crate::ComponentForm) -> Self {
        let unmodulated = match form {
            crate::ComponentForm::BinauralPair { .. } => return Self::Binaural,
            crate::ComponentForm::MonauralPair { .. } => return Self::Monaural,
            crate::ComponentForm::Tone { .. } => Self::SteadyTone,
            crate::ComponentForm::Clicks { .. } => Self::Clicks,
        };
        match form.modulation() {
            Some(Modulation::Gate { .. }) => Self::Gate,
            Some(Modulation::Am { depth, .. }) if depth.get() > 0.0 => Self::Am,
            _ => unmodulated,
        }
    }

    pub(crate) const fn of_pair(kind: crate::PairKind) -> Self {
        match kind {
            crate::PairKind::Binaural => Self::Binaural,
            crate::PairKind::Monaural => Self::Monaural,
        }
    }
}

/// Whether intrinsic settings satisfy the structural analysis contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    /// Every authored process has valid intrinsic settings.
    Complete,
    /// Intrinsic settings or authored structure need correction.
    Invalid,
}
/// One positive rhythmic process, with stable generator attribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Descriptor {
    /// Generator identity within its authored owner.
    pub owner: VoiceId,
    /// Continuing, incoming or outgoing generator role.
    pub role: SlotRole,
    /// Structural mechanism.
    pub method: Method,
    /// Positive finite rhythmic rate in hertz.
    pub hz: f64,
    /// Membership under the disclosed convention.
    pub band: Option<Band>,
}
/// An intrinsic quantity or authored-structure problem.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// A rate-independent quantity violates its domain rule.
    Value {
        /// The generator owning this quantity.
        owner: VoiceId,
        /// The typed quantity and its failed rule.
        issue: ValueIssue,
    },
    /// Coupled pair arithmetic cannot retain its authored intent.
    Pair(PairError),
}
/// Rate-independent analysis, including invalid but inspectable authored sounds.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Independent rhythmic processes in authored order.
    pub rhythms: Vec<Descriptor>,
    /// Deduplicated structural methods, including steady tones.
    pub methods: Vec<Method>,
    /// Deduplicated bands in reference order.
    pub bands: Vec<Band>,
    /// Intrinsic validity of the analysis.
    pub completeness: Completeness,
    /// Typed problems, independent of frontend wording.
    pub diagnostics: Vec<Issue>,
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
}

#[derive(Clone, Copy)]
enum Periodic {
    Am { rate_hz: f64, depth: f64 },
    Gate { rate_hz: f64 },
}
impl From<Modulation> for Periodic {
    fn from(value: Modulation) -> Self {
        match value {
            Modulation::Am { rate_hz, depth } => Self::Am {
                rate_hz: rate_hz.get(),
                depth: depth.get(),
            },
            Modulation::Gate { rate_hz, .. } => Self::Gate {
                rate_hz: rate_hz.get(),
            },
        }
    }
}
#[derive(Clone, Copy)]
struct Input {
    owner: VoiceId,
    role: SlotRole,
    carrier: Carrier,
    modulation: Option<Periodic>,
    pair: Option<(PairKind, Option<f64>)>,
}
// Both public entry points normalize to this function. Carrier frequencies
// never become rhythmic rates, and unrelated tones never become a pair.
fn describe(input: Input, rhythms: &mut Vec<Descriptor>, methods: &mut Vec<Method>) {
    let mut add = |method, rate: Option<f64>| {
        methods.push(method);
        if let Some(hz) = rate.filter(|hz| validity::positive(*hz)) {
            rhythms.push(Descriptor {
                owner: input.owner,
                role: input.role,
                method,
                hz,
                band: Band::of(hz),
            });
        }
    };
    if let Some((kind, rate)) = input.pair {
        add(Method::of_pair(kind), rate);
        return;
    }
    match input.carrier {
        Carrier::Clicks { rate_hz, .. } => add(Method::Clicks, Some(rate_hz)),
        Carrier::Tone { .. }
            if matches!(
                input.modulation,
                None | Some(Periodic::Am { depth: 0.0, .. })
            ) =>
        {
            add(Method::SteadyTone, None)
        }
        Carrier::Tone { .. } => {}
    }
    match input.modulation {
        Some(Periodic::Am { rate_hz, depth }) if depth > 0.0 => add(Method::Am, Some(rate_hz)),
        Some(Periodic::Gate { rate_hz }) => add(Method::Gate, Some(rate_hz)),
        _ => {}
    }
}
fn finish_methods(methods: &mut Vec<Method>) {
    methods.sort();
    methods.dedup();
}
/// Classify authored sound without choosing a rendering sample rate.
#[must_use]
pub fn nominal(sound: &crate::Sound) -> Analysis {
    let components = sound.components();
    let mut result = Analysis {
        rhythms: Vec::new(),
        methods: Vec::new(),
        bands: Vec::new(),
        completeness: Completeness::Complete,
        diagnostics: Vec::new(),
    };
    for component in components {
        let owner = component.id.into();
        let (carrier, modulation, pair) = match component.form.primitive() {
            Some(voice) => {
                result.diagnostics.extend(
                    component
                        .form
                        .intrinsic_issues()
                        .map(|issue| Issue::Value { owner, issue }),
                );
                (voice.carrier, voice.modulation.map(Periodic::from), None)
            }
            None => {
                let form = component.form;
                let pair = form.pair().unwrap_or_else(|| unreachable!("pair form"));
                let kind = form
                    .pair_kind()
                    .unwrap_or_else(|| unreachable!("pair form"));
                for (parameter, value, lane) in [
                    (
                        ParameterId::PairFirstLevel,
                        pair.first_peak_dbfs,
                        Lane::Primary,
                    ),
                    (
                        ParameterId::PairSecondLevel,
                        pair.second_peak_dbfs,
                        Lane::Secondary,
                    ),
                ] {
                    if let Err(issue) = validity::check_value(parameter, value) {
                        result.diagnostics.push(Issue::Value {
                            owner: component.id.with_lane(lane),
                            issue,
                        });
                    }
                }
                if let Err(error) = crate::authored::lower(component) {
                    result.diagnostics.push(Issue::Pair(error));
                }
                (
                    Carrier::Tone {
                        frequency_hz: pair.first_hz,
                    },
                    None,
                    Some((kind, Some(pair.offset_hz.abs()))),
                )
            }
        };
        describe(
            Input {
                owner,
                role: SlotRole::Continuing,
                carrier,
                modulation,
                pair,
            },
            &mut result.rhythms,
            &mut result.methods,
        );
    }
    if !result.diagnostics.is_empty() {
        result.completeness = Completeness::Invalid;
    }
    finish_methods(&mut result.methods);
    result.bands = Band::ALL
        .into_iter()
        .filter(|band| result.rhythms.iter().any(|r| r.band == Some(*band)))
        .collect();
    result
}
/// Actual carrier rates of one explicit authored pair at an observed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairRates {
    /// Explicit pair construction, never guessed from routes.
    pub kind: PairKind,
    /// First/left realized carrier rate.
    pub first: f64,
    /// Second/right realized carrier rate.
    pub second: f64,
    /// Generator transition role.
    pub role: SlotRole,
}
/// Realized structure and active voice facts, including transition roles.
#[derive(Debug, Clone, PartialEq)]
pub struct Structure {
    /// Independent realized rhythmic processes.
    pub rhythms: Vec<Descriptor>,
    /// Active voices with actual routing gains.
    pub voices: Vec<VoiceObservation>,
    /// Explicit pairs with realized carrier rates.
    pub pairs: Vec<PairRates>,
    /// Whether the generated state is transitioning.
    pub changing: bool,
    methods: Vec<Method>,
}
impl Structure {
    /// Deduplicated structural methods from the shared descriptor rules.
    #[must_use]
    pub fn methods(&self) -> Vec<Method> {
        self.methods.clone()
    }
    /// The realized rates of one explicit binaural pair with no extra voices.
    #[must_use]
    pub fn interaural(&self) -> Option<(f64, f64)> {
        if self.voices.len() != 2 {
            return None;
        }
        self.pairs
            .iter()
            .find(|pair| pair.kind == PairKind::Binaural)
            .map(|pair| (pair.first, pair.second))
    }
    /// The slowest active rhythmic process.
    #[must_use]
    pub fn slowest_rate(&self) -> Option<f64> {
        self.rhythms.iter().map(|r| r.hz).reduce(f64::min)
    }
}
/// Either supported source of realized generator facts.
pub enum RealizedInput<'a> {
    /// One generated observation.
    Observation(&'a Observation),
    /// A validated target at model origin.
    Target(&'a Target),
}
impl<'a> From<&'a Observation> for RealizedInput<'a> {
    fn from(value: &'a Observation) -> Self {
        Self::Observation(value)
    }
}
impl<'a> From<&'a Target> for RealizedInput<'a> {
    fn from(value: &'a Target) -> Self {
        Self::Target(value)
    }
}
/// Classify actual generator rates and transition roles through the shared rules.
#[must_use]
pub fn realized<'a>(source: impl Into<RealizedInput<'a>>) -> Structure {
    match source.into() {
        RealizedInput::Observation(observation) => classify(
            observation.voices.iter().flatten().copied(),
            observation.transitioning,
        ),
        RealizedInput::Target(target) => classify(target.observations(), false),
    }
}
pub(crate) fn classify(
    voices: impl Iterator<Item = VoiceObservation>,
    changing: bool,
) -> Structure {
    let voices: Vec<_> = voices
        .filter(|v| v.gains.iter().any(|gain| *gain > 0.0))
        .collect();
    let mut result = Structure {
        rhythms: Vec::new(),
        voices,
        pairs: Vec::new(),
        changing,
        methods: Vec::new(),
    };
    for value in &result.voices {
        let carrier = match value.facts.carrier {
            CarrierFacts::Tone(rate) => Carrier::Tone {
                frequency_hz: rate.realized_hz,
            },
            CarrierFacts::Clicks(pulse) => Carrier::Clicks {
                rate_hz: pulse.rate_hz,
                width_seconds: pulse.realized_width_seconds(),
            },
        };
        let modulation = value.facts.modulation.map(|modulation| match modulation {
            ModulationFacts::Am { rate, depth } => Periodic::Am {
                rate_hz: rate.realized_hz,
                depth,
            },
            ModulationFacts::Gate(pulse) => Periodic::Gate {
                rate_hz: pulse.rate_hz,
            },
        });
        let pair = value.facts.pair.map(|pair| {
            let mut rate = None;
            if value.id.lane == Lane::Primary
                && let Some(other) = result.voices.iter().find(|other| {
                    other.id == value.id.with_lane(Lane::Secondary)
                        && other.role == value.role
                        && other.facts.pair == Some(pair)
                })
                && let (CarrierFacts::Tone(first), CarrierFacts::Tone(second)) =
                    (value.facts.carrier, other.facts.carrier)
            {
                rate = Some((second.realized_hz - first.realized_hz).abs());
                result.pairs.push(PairRates {
                    kind: pair.kind,
                    first: first.realized_hz,
                    second: second.realized_hz,
                    role: value.role,
                });
            }
            (pair.kind, rate)
        });
        describe(
            Input {
                owner: value.id,
                role: value.role,
                carrier,
                modulation,
                pair,
            },
            &mut result.rhythms,
            &mut result.methods,
        );
    }
    finish_methods(&mut result.methods);
    result
}

crate::vocabulary::vocabulary! { Band,"band" {
    Delta => ("delta","Delta","Delta"),
    Theta => ("theta","Theta","Theta"),
    Alpha => ("alpha","Alpha","Alpha"),
    Beta => ("beta","Beta","Beta"),
    Gamma => ("gamma","Gamma","Gamma"),
}}

crate::vocabulary::vocabulary! { Method,"method" {
    Binaural => ("binaural","Binaural","binaural"),
    Monaural => ("monaural","Monaural","monaural"),
    Clicks => ("clicks","Clicks","clicks"),
    Gate => ("gate","Gate","gate"),
    Am => ("am","AM","AM"),
    SteadyTone => ("steady_tone","Steady tone","steady tone"),
}}

crate::vocabulary::vocabulary! { Completeness,"completeness" {
    Complete => ("complete","Complete","complete"),
    Invalid => ("invalid","Invalid","invalid"),
}}

impl core::fmt::Display for Issue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}

impl core::error::Error for Issue {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(match self {
            Self::Value { issue, .. } => issue,
            Self::Pair(cause) => cause,
        })
    }
}

#[cfg(test)]
mod primary_tests {
    use super::*;
    use crate::{ComponentForm, Ears, test_support::finite};
    #[test]
    fn primary_method_keeps_modulation_precedence_and_zero_depth_carriers() {
        for (modulation, tone, clicks) in [
            (None, Method::SteadyTone, Method::Clicks),
            (
                Some(Modulation::Gate {
                    rate_hz: finite(0.0),
                    width_seconds: finite(0.001),
                }),
                Method::Gate,
                Method::Gate,
            ),
            (
                Some(Modulation::Am {
                    rate_hz: finite(0.0),
                    depth: finite(0.5),
                }),
                Method::Am,
                Method::Am,
            ),
            (
                Some(Modulation::Am {
                    rate_hz: finite(7.0),
                    depth: finite(0.0),
                }),
                Method::SteadyTone,
                Method::Clicks,
            ),
        ] {
            let form = ComponentForm::Tone {
                ears: Ears::Both,
                frequency_hz: finite(440.0),
                peak_dbfs: finite(-30.0),
                modulation,
            };
            assert_eq!(Method::primary_of(form), tone);
            let form = ComponentForm::Clicks {
                ears: Ears::Both,
                rate_hz: finite(40.0),
                width_seconds: finite(0.001),
                peak_dbfs: finite(-30.0),
                modulation,
            };
            assert_eq!(Method::primary_of(form), clicks);
        }
        assert_eq!(
            Method::primary_of(ComponentForm::BinauralPair {
                left_hz: finite(440.0),
                offset_hz: finite(7.0),
                left_peak_dbfs: finite(-30.0),
                right_peak_dbfs: finite(-30.0)
            }),
            Method::Binaural
        );
        assert_eq!(
            Method::primary_of(ComponentForm::MonauralPair {
                ears: Ears::Both,
                first_hz: finite(440.0),
                offset_hz: finite(7.0),
                first_peak_dbfs: finite(-30.0),
                second_peak_dbfs: finite(-30.0)
            }),
            Method::Monaural
        );
    }
}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    Issue {
        codes {}
        delegates {
            Self::Value { issue, .. } => issue,
            Self::Pair(error) => error,
        }
        params(this) {
            match this {
                Self::Value { owner, .. } => {
                    crate::diagnostic_params! {"owner"=>alloc::string::ToString::to_string(owner)}
                }
                Self::Pair(_) => crate::Params::new(),
            }
        }
    }
}
