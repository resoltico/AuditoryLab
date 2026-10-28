//! Runtime library choices and atomic query edits.
use super::*;

/// One library query facet, independent of control position or wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryFacet {
    Scope,
    Sort,
    Direction,
    Group,
    Band,
    Method,
    Population,
    Role,
    Analysis,
}
/// One offered semantic key and its current selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOption<K = &'static str> {
    pub key: K,
    pub label: &'static str,
    pub selected: bool,
    pub enabled: bool,
}
/// Runtime options; absence/automatic choices are ordinary keyed options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceView<K = &'static str> {
    pub options: Vec<ChoiceOption<K>>,
}
/// Native filter entry captured together before date/query validation.
#[derive(Debug, Clone)]
pub struct FilterInput {
    pub study: String,
    pub facets: Vec<(QueryFacet, String)>,
    /// Created, modified, added; each pair is inclusive local from/through text.
    pub dates: [[String; 2]; 3],
}

/// A menu whose values and selection are owned by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMenu {
    Ears,
    Carrier,
    Modulation,
    AddComponent,
    MonitorSpan,
    WaveformScale,
    ExportRate,
}
/// Typed payload retained in the table for one rendered menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlValue {
    Ears(Ears),
    Carrier(CarrierKind),
    Modulation(Option<ModulationKind>),
    AddCarrier(CarrierKind),
    AddPair(crate::core::PairKind),
    MonitorSpan(u32),
    WaveformScale(crate::workspace::monitor::WaveformScale),
    ExportRate(SampleRate),
}
/// An offered menu bound to an exact rendered document or component lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlChoices {
    pub menu: ControlMenu,
    pub row: RowKey,
    pub view: ChoiceView<ControlValue>,
}
