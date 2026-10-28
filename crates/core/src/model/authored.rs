// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Ordered authored components and lossless lowering into primitive generator lanes.
use crate::{Carrier, ComponentId, Ears, Finite, Lane, Modulation, Voice, VoiceId};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A signed-offset two-tone construction. The second carrier is first + offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// First carrier; left for a binaural pair.
    pub first_hz: Finite,
    /// Authored signed nominal difference; never recovered from generated carriers.
    pub offset_hz: Finite,
    /// First/left tone's peak bound.
    pub first_peak_dbfs: Finite,
    /// Second/right tone's peak bound.
    pub second_peak_dbfs: Finite,
}
// These helpers select only capabilities explicitly present in a kind row.
macro_rules! optional_route {
    () => {
        None
    };
    ($field:ident) => {
        Some($field)
    };
}
macro_rules! optional_modulation {
    () => {
        None
    };
    ($field:ident) => {
        $field
    };
}
macro_rules! replace_modulation {
    ($value:expr;) => {
        false
    };
    ($value:expr;$field:ident) => {{
        *$field = $value;
        true
    }};
}
macro_rules! set_modulation_quantity {
    ($parameter:expr,$value:expr;) => {
        false
    };
    ($parameter:expr,$value:expr;$field:ident) => {
        $field
            .as_mut()
            .is_some_and(|modulation| modulation.set($parameter, $value))
    };
}

// The parser constructs either identified components or standalone forms.
// The associated identity keeps the required-id case out of an optional result.
trait FromComponentFields: Sized {
    type Identity;
    const IDENTIFIED: bool;
    fn identity<E: serde::de::Error>(id: Option<ComponentId>) -> Result<Self::Identity, E>;
    fn from_fields(id: Self::Identity, form: ComponentForm) -> Self;
}
impl FromComponentFields for Component {
    type Identity = ComponentId;
    const IDENTIFIED: bool = true;
    fn identity<E: serde::de::Error>(id: Option<ComponentId>) -> Result<Self::Identity, E> {
        id.ok_or_else(|| E::missing_field("id"))
    }
    fn from_fields(id: Self::Identity, form: ComponentForm) -> Self {
        Self { id, form }
    }
}
impl FromComponentFields for ComponentForm {
    type Identity = ();
    const IDENTIFIED: bool = false;
    fn identity<E: serde::de::Error>(_: Option<ComponentId>) -> Result<Self::Identity, E> {
        Ok(())
    }
    fn from_fields((): Self::Identity, form: ComponentForm) -> Self {
        form
    }
}

macro_rules! define_component_codec {
    (
        parameters {$($parameters:tt)*}
        fields {$( $wire:ident:$wire_type:ty $(=> $wire_parameter:ident)?; )*}
        kinds {$( $kind:ident $key:literal {
            $(route $route:ident;)?
            quantities [$($quantity:ident),*];
            $(level $level:ident;)?
            $(modulation $modulation:ident;)?
        })*}
        modulation_quantities {$($modulation_quantities:tt)*}
    )=>{
        macro_rules! component_field_type {$(($wire)=>{$wire_type};)*}
        macro_rules! component_parameter {$( $(($wire)=>{crate::ParameterId::$wire_parameter};)? )*}
        /// One authored primitive or explicit pair, without an identity.
        #[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
        pub enum ComponentForm {
            $(
                #[doc=concat!("Authored ",$key," quantities.")]
                $kind {
                    $(#[doc="Requested route."] $route:component_field_type!($route),)?
                    $(#[doc=concat!("Requested `",stringify!($quantity),
                        "` in model units.")] $quantity:component_field_type!($quantity),)*
                    $(#[doc="Requested peak level in dBFS."] $level:component_field_type!($level),)?
                    $(#[doc="Optional authored modulation."]
                        $modulation:component_field_type!($modulation),)?
                },
            )*
        }
        impl ComponentForm {
            fn serialize_with_id<S:Serializer>(self,id:Option<ComponentId>,
                serializer:S)->Result<S::Ok,S::Error> {
                use serde::ser::SerializeMap;
                let mut map=serializer.serialize_map(None)?;
                match self {
                    $(Self::$kind { $($route,)? $($quantity,)* $($level,)? $($modulation,)? }=>{
                        map.serialize_entry("kind",$key)?;
                        if let Some(id)=id {map.serialize_entry("id",&id)?;}
                        $(map.serialize_entry(stringify!($route),&$route)?;)?
                        $(map.serialize_entry(stringify!($quantity),&$quantity)?;)*
                        $(map.serialize_entry(stringify!($level),&$level)?;)?
                        $(if let Some(modulation)=$modulation {
                            map.serialize_entry(stringify!($modulation),&modulation)?;})?
                    },)*
                }
                map.end()
            }
            /// Shared route, absent for separate-ear pairs.
            #[must_use]
            pub const fn route(self)->Option<Ears> {match self {$(Self::$kind {$($route,)?..}
                =>optional_route!($($route)?),)*}}
            /// Optional modulation, when the kind permits it.
            #[must_use]
            pub const fn modulation(self)->Option<Modulation> {match self {$(Self::$kind {
                $($modulation,)?..}=>optional_modulation!($($modulation)?),)*}}
            /// Replace modulation only on a kind that supports it.
            pub fn set_modulation(&mut self,value:Option<Modulation>)->bool {match self {
                $(Self::$kind {$($modulation,)?..}=>replace_modulation!(value;$($modulation)?),)*}}
            /// Read an applicable authored quantity in model units.
            #[must_use]
            pub fn get(self,parameter:crate::ParameterId)->Option<Finite> {
                match (parameter,self) {
                    $($( (component_parameter!($quantity),Self::$kind {$quantity,..}
                        )=>Some($quantity),)*
                      $((component_parameter!($level),Self::$kind {$level,..})=>Some($level),)?)*
                    _=>self.modulation().and_then(|modulation|modulation.get(parameter)),
                }
            }
            /// Change one applicable authored quantity, without touching other fields.
            pub fn set(&mut self,parameter:crate::ParameterId,value:Finite)->bool {
                match self {
                    $(Self::$kind { $($quantity,)* $($level,)? $($modulation,)? ..}=>{
                        $(if parameter==component_parameter!($quantity) {*$quantity=value;
                            return true;})*
                        $(if parameter==component_parameter!($level) {*$level=value;return true;})?
                        set_modulation_quantity!(parameter,value;$($modulation)?)
                    },)*
                }
            }
        }
        impl Serialize for ComponentForm {
            fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> {
                self.serialize_with_id(None,serializer)}
        }
        impl Serialize for Component {
            fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> {
                self.form.serialize_with_id(Some(self.id),serializer)}
        }
        struct ComponentVisitor<T>(core::marker::PhantomData<T>);
        impl<'de,T:FromComponentFields> serde::de::Visitor<'de> for ComponentVisitor<T> {
            type Value=T;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("an authored \
                component")}
            fn visit_map<M:serde::de::MapAccess<'de>>(self,mut map:M)->Result<T,M::Error> {
                use serde::de::Error;
                let mut id:Option<ComponentId>=None;
                let mut kind:Option<alloc::string::String>=None;
                $(let mut $wire:Option<$wire_type>=None;)*
                while let Some(key)=map.next_key::<alloc::string::String>()? {
                    match key.as_str() {
                        "id" if T::IDENTIFIED=>{if id.is_some(){
                            return Err(M::Error::duplicate_field("id"));}id=Some(map.next_value()?);
                            },
                        "kind"=>{if kind.is_some(){return Err(M::Error::duplicate_field("kind"));}
                            kind=Some(map.next_value()?);},
                        $(stringify!($wire)=>{if $wire.is_some(){
                            return Err(M::Error::duplicate_field(stringify!($wire)));}
                            $wire=Some(map.next_value()?);},)*
                        _=>return Err(M::Error::custom(alloc::format!("unknown component field \
                            `{key}`"))),
                    }
                }
                let id=T::identity(id)?;
                let kind=kind.ok_or_else(||M::Error::missing_field("kind"))?;
                let present=[$((stringify!($wire),$wire.is_some())),*];
                let form=match kind.as_str() {
                    $($key=>{
                        let allowed:&[&str]=&[$(stringify!($route),)?$(stringify!($quantity),
                            )*$(stringify!($level),)?$(stringify!($modulation),)?];
                        for (field,present) in present {if present&&!allowed.contains(&field) {
                            return Err(M::Error::custom(alloc::format!("unknown \
                            field `{field}` for {kind}")));}}
                        ComponentForm::$kind {
                            $(
                                $route: $route.ok_or_else(
                                    || M::Error::missing_field(stringify!($route))
                                )?,
                            )?
                            $(
                                $quantity: $quantity.ok_or_else(
                                    || M::Error::missing_field(stringify!($quantity))
                                )?,
                            )*
                            $(
                                $level: $level.ok_or_else(
                                    || M::Error::missing_field(stringify!($level))
                                )?,
                            )?
                            $($modulation:$modulation.unwrap_or(None),)?
                        }
                    },)*
                    _=>return Err(M::Error::unknown_variant(&kind,&[$($key),*])),
                };
                Ok(T::from_fields(id,form))
            }
        }
        impl<'de> Deserialize<'de> for Component {
            fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
                deserializer.deserialize_map(ComponentVisitor::<Self>(core::marker::PhantomData))}
        }
        impl<'de> Deserialize<'de> for ComponentForm {
            fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
                deserializer.deserialize_map(ComponentVisitor::<Self>(core::marker::PhantomData))}
        }
    };
}
crate::component_schema::component_schema!(define_component_codec);

/// An authored form and its positive stable owner identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Component {
    /// Authored owner, independent of generator lanes.
    pub id: ComponentId,
    /// Authored sound structure.
    pub form: ComponentForm,
}
/// Coupled carrier/offset failure, independent of sample-rate Nyquist checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairIssue {
    /// The computed second carrier is nonfinite.
    Nonfinite,
    /// One computed carrier is zero or negative.
    Nonpositive,
    /// A nonzero authored offset disappears during floating-point addition.
    UnrepresentablePairDifference,
}
impl core::fmt::Display for PairIssue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
/// Why authored owners or generator capacity violate the sound structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureError {
    /// A repeated authored identity.
    Identity {
        /// Repeated owner.
        id: ComponentId,
    },
    /// Too many expanded voices.
    VoiceBudget {
        /// Expanded voice count.
        count: usize,
    },
    /// Too many authored components.
    ComponentBudget {
        /// Authored component count.
        count: usize,
    },
}
/// A requested pair cannot retain its coupled carrier arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub struct PairError {
    /// The failed arithmetic rule.
    pub issue: PairIssue,
    /// Authored owner.
    pub id: ComponentId,
    /// Requested first carrier.
    pub first_hz: f64,
    /// Requested signed offset.
    pub offset_hz: f64,
}
impl core::fmt::Display for StructureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::fmt::Display for PairError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for StructureError {}
impl core::error::Error for PairError {}
/// Expand one authored component in stable primary/secondary generator order.
/// No reordering/regrouping of raw tones or inferred pairing occurs.
///
/// # Errors
/// Refuses unrepresentable pair carrier arithmetic.
pub(crate) fn lower(component: &Component) -> Result<Vec<(VoiceId, Voice)>, PairError> {
    let mut output = Vec::new();
    let id = component.id;
    if let Some(value) = component.form.primitive() {
        output.push((id.into(), value));
    } else {
        let form = component.form;
        {
            let pair = form
                .pair()
                .unwrap_or_else(|| unreachable!("nonprimitive forms are pairs"));
            let (first_ears, second_ears) = match form.route() {
                None => (Ears::Left, Ears::Right),
                Some(ears) => (ears, ears),
            };
            let second_hz = pair.first_hz.get() + pair.offset_hz.get();
            // The finite operands can still overflow during pair addition.
            let issue = if !second_hz.is_finite() {
                Some(PairIssue::Nonfinite)
            } else if !crate::validity::positive(pair.first_hz.get())
                || !crate::validity::positive(second_hz)
            {
                Some(PairIssue::Nonpositive)
            } else if pair.offset_hz.get() != 0.0 && second_hz == pair.first_hz.get() {
                Some(PairIssue::UnrepresentablePairDifference)
            } else {
                None
            };
            if let Some(issue) = issue {
                return Err(PairError {
                    issue,
                    id,
                    first_hz: pair.first_hz.get(),
                    offset_hz: pair.offset_hz.get(),
                });
            }
            let tone = |frequency_hz, peak_dbfs, ears| Voice {
                carrier: Carrier::Tone { frequency_hz },
                peak_dbfs,
                ears,
                modulation: None,
            };
            output.push((
                id.into(),
                tone(pair.first_hz.get(), pair.first_peak_dbfs.get(), first_ears),
            ));
            output.push((
                id.with_lane(Lane::Secondary),
                tone(second_hz, pair.second_peak_dbfs.get(), second_ears),
            ));
        }
    }
    Ok(output)
}

/// Ordered authored sound with unique identities and bounded generator capacity.
/// Fields can only be set through checked construction and mutation.
/// ```compile_fail
/// use auditorylab_core::{Finite, Sound};
/// let _ = Sound {
///     duration_seconds: Finite::ZERO,
///     fade_seconds: Finite::ZERO,
///     components: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    duration_seconds: Finite,
    fade_seconds: Finite,
    components: Vec<Component>,
}
impl Sound {
    /// Maximum number of authored components.
    pub const MAX_COMPONENTS: usize = 16;
    /// Maximum number of expanded generator lanes.
    pub const MAX_VOICES: usize = 16;
    /// Construct a sound while checking owner uniqueness and both budgets.
    ///
    /// # Errors
    /// Returns the first identity or capacity violation.
    pub fn new(
        duration_seconds: Finite,
        fade_seconds: Finite,
        components: Vec<Component>,
    ) -> Result<Self, StructureError> {
        Self::check_structure(&components)?;
        Ok(Self {
            duration_seconds,
            fade_seconds,
            components,
        })
    }
    fn check_structure(components: &[Component]) -> Result<(), StructureError> {
        Self::check_capacity(
            components.len(),
            components.iter().map(|c| c.form.voices()).sum(),
        )?;
        for (index, component) in components.iter().enumerate() {
            if components[..index]
                .iter()
                .any(|other| other.id == component.id)
            {
                return Err(StructureError::Identity { id: component.id });
            }
        }
        Ok(())
    }
    fn check_capacity(components: usize, voices: usize) -> Result<(), StructureError> {
        if components > Self::MAX_COMPONENTS {
            return Err(StructureError::ComponentBudget { count: components });
        }
        if voices > Self::MAX_VOICES {
            return Err(StructureError::VoiceBudget { count: voices });
        }
        Ok(())
    }
    /// Check capacity for one additional form. Insertion checks identity again.
    ///
    /// # Errors
    /// Returns the exceeded authored or expanded budget.
    pub fn check_addition(&self, form: ComponentForm) -> Result<(), StructureError> {
        Self::check_capacity(
            self.components.len() + 1,
            self.components
                .iter()
                .map(|c| c.form.voices())
                .sum::<usize>()
                + form.voices(),
        )
    }
    /// Duration in seconds; target-rate envelope constraints are checked separately.
    #[must_use]
    pub const fn duration_seconds(&self) -> Finite {
        self.duration_seconds
    }
    /// Requested edge fade in seconds.
    #[must_use]
    pub const fn fade_seconds(&self) -> Finite {
        self.fade_seconds
    }
    /// Authored components in summation order.
    #[must_use]
    pub fn components(&self) -> &[Component] {
        &self.components
    }
    /// One authored component by its stable identity.
    #[must_use]
    pub fn component(&self, id: ComponentId) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
    /// Replace the requested duration without changing other content.
    pub const fn set_duration(&mut self, value: Finite) {
        self.duration_seconds = value;
    }
    /// Replace the requested edge fade without changing other content.
    pub const fn set_fade(&mut self, value: Finite) {
        self.fade_seconds = value;
    }
    /// Append a component, leaving the sound unchanged on refusal.
    ///
    /// # Errors
    /// Returns an identity or capacity violation.
    pub fn push(&mut self, component: Component) -> Result<(), StructureError> {
        self.insert(self.components.len(), component)
    }
    /// Insert at an authored position, leaving the sound unchanged on refusal.
    ///
    /// # Errors
    /// Returns an identity or capacity violation.
    /// # Panics
    /// Panics when the index exceeds the component count, like `Vec::insert`.
    pub fn insert(&mut self, index: usize, component: Component) -> Result<(), StructureError> {
        let mut candidate = self.components.clone();
        candidate.insert(index, component);
        Self::check_structure(&candidate)?;
        self.components = candidate;
        Ok(())
    }
    /// Remove an owner, returning its position and complete authored form.
    pub fn remove(&mut self, id: ComponentId) -> Option<(usize, Component)> {
        let index = self.components.iter().position(|c| c.id == id)?;
        Some((index, self.components.remove(index)))
    }
    /// Replace an existing owner's form. Unknown owners return `Ok(false)`.
    ///
    /// # Errors
    /// Returns a capacity violation and retains the original sound.
    pub fn replace(
        &mut self,
        id: ComponentId,
        form: ComponentForm,
    ) -> Result<bool, StructureError> {
        let Some(index) = self.components.iter().position(|c| c.id == id) else {
            return Ok(false);
        };
        let mut candidate = self.components.clone();
        candidate[index].form = form;
        Self::check_structure(&candidate)?;
        self.components = candidate;
        Ok(true)
    }
    /// Compile every generator lane at the chosen rate.
    #[must_use]
    pub fn check(&self, rate: crate::SampleRate) -> crate::TargetCheck {
        crate::TargetCheck::authored(rate, self)
    }
    /// Compile the complete authored target.
    ///
    /// # Errors
    /// Returns pair arithmetic or target-rate validation errors.
    pub fn target(&self, rate: crate::SampleRate) -> Result<crate::Target, crate::SoundError> {
        self.check(rate).into_result()
    }
    /// Give the authored target a finite envelope.
    ///
    /// # Errors
    /// Returns target or finite-envelope validation errors.
    pub fn signal(&self, rate: crate::SampleRate) -> Result<crate::Signal, crate::SoundError> {
        let envelope = crate::signal::Envelope::resolve(
            rate,
            self.duration_seconds.get(),
            self.fade_seconds.get(),
        )?;
        Ok(crate::Signal::new(&self.target(rate)?, envelope))
    }
    /// Whether any authored form needs separate ear channels.
    #[must_use]
    pub fn uses_separate_ears(&self) -> bool {
        self.components.iter().any(|c| c.form.separates_ears())
    }
}

impl ComponentForm {
    /// Number of generator lanes before live transitions.
    #[must_use]
    pub const fn voices(self) -> usize {
        match self {
            Self::Tone { .. } | Self::Clicks { .. } => 1,
            _ => 2,
        }
    }
    /// The explicit pair construction.
    #[must_use]
    pub const fn pair_kind(self) -> Option<PairKind> {
        match self {
            Self::BinauralPair { .. } => Some(PairKind::Binaural),
            Self::MonauralPair { .. } => Some(PairKind::Monaural),
            _ => None,
        }
    }
    /// Explicit signed pair parameters.
    #[must_use]
    pub const fn pair(self) -> Option<Pair> {
        match self {
            Self::BinauralPair {
                left_hz,
                offset_hz,
                left_peak_dbfs,
                right_peak_dbfs,
            } => Some(Pair {
                first_hz: left_hz,
                offset_hz,
                first_peak_dbfs: left_peak_dbfs,
                second_peak_dbfs: right_peak_dbfs,
            }),
            Self::MonauralPair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
                ..
            } => Some(Pair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            }),
            _ => None,
        }
    }

    /// Whether this form needs separate ear channels.
    #[must_use]
    pub const fn separates_ears(self) -> bool {
        !matches!(self.route(), Some(Ears::Both))
    }
    pub(crate) const fn primitive(self) -> Option<Voice> {
        match self {
            Self::Tone {
                ears,
                frequency_hz,
                peak_dbfs,
                modulation,
            } => Some(Voice {
                ears,
                carrier: Carrier::Tone {
                    frequency_hz: frequency_hz.get(),
                },
                peak_dbfs: peak_dbfs.get(),
                modulation,
            }),
            Self::Clicks {
                ears,
                rate_hz,
                width_seconds,
                peak_dbfs,
                modulation,
            } => Some(Voice {
                ears,
                carrier: Carrier::Clicks {
                    rate_hz: rate_hz.get(),
                    width_seconds: width_seconds.get(),
                },
                peak_dbfs: peak_dbfs.get(),
                modulation,
            }),
            _ => None,
        }
    }

    /// Primitive carrier choice; pairs have no carrier choice.
    #[must_use]
    pub const fn primitive_kind(self) -> Option<crate::CarrierKind> {
        match self {
            Self::Tone { .. } => Some(crate::CarrierKind::Tone),
            Self::Clicks { .. } => Some(crate::CarrierKind::Clicks),
            _ => None,
        }
    }
    /// Optional modulation kind.
    #[must_use]
    pub const fn modulation_kind(self) -> Option<crate::ModulationKind> {
        match self.modulation() {
            Some(m) => Some(m.kind()),
            None => None,
        }
    }
    /// Domain heading without target-rate inference.
    #[must_use]
    pub fn heading(self) -> &'static str {
        crate::rhythm::Method::primary_of(self).label()
    }

    pub(crate) fn intrinsic_issues(&self) -> impl Iterator<Item = crate::ValueIssue> + '_ {
        crate::ParameterId::COMPONENT.into_iter().filter_map(|p| {
            self.get(p)
                .and_then(|v| crate::validity::check_value(p, v.get()).err())
        })
    }
}
/// The two supported explicit pair constructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    /// Separate left/right tones.
    Binaural,
    /// Two tones on one identical route.
    Monaural,
}

crate::vocabulary::vocabulary! { PairKind,"pair_kind" {
    Binaural => ("binaural","Binaural pair","binaural pair"),
    Monaural => ("monaural","Monaural pair","monaural pair"),
}}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    PairIssue {
        codes {
            Self::Nonfinite => "pair.nonfinite",
            Self::Nonpositive => "pair.nonpositive",
            Self::UnrepresentablePairDifference => "pair.unrepresentable_difference",
        }
        delegates {}
    }
    StructureError {
        codes {
            Self::Identity { .. } => "structure.identity",
            Self::ComponentBudget { .. } => "structure.component_budget",
            Self::VoiceBudget { .. } => "structure.voice_budget",
        }
        delegates {}
        params(this) {
            match this {
                Self::Identity { id } => crate::diagnostic_params! {"component_id"=>id.get()},
                Self::ComponentBudget { count } => {
                    crate::diagnostic_params! {"count"=>*count,"limit"=>Sound::MAX_COMPONENTS}
                }
                Self::VoiceBudget { count } => {
                    crate::diagnostic_params! {"count"=>*count,"limit"=>Sound::MAX_VOICES}
                }
            }
        }
    }
    PairError {
        codes {}
        delegates {
            Self { issue, .. } => issue,
        }
        params(this) {
            crate::diagnostic_params! {"component_id"=>this.id.get(),"first_hz"=>this.first_hz,
                "offset_hz"=>this.offset_hz}
        }
    }
}

#[cfg(test)]
mod tests;
