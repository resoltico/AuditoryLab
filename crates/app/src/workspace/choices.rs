//! Runtime library choices and atomic query edits.
use super::*;
use crate::core::rhythm::{Band, Completeness, Method};
use crate::core::{ConditionRole, Population};
use crate::discovery::collection::CatalogQuery;
use crate::discovery::query::{Direction, GroupBy, RhythmState, Scope, SortBy};

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
impl Workspace {
    /// Current options for one library facet. Keys retain meaning across projections.
    #[must_use]
    pub fn library_choices(&self, facet: QueryFacet) -> ChoiceView {
        let q = self.catalog_query();
        let mut options = Vec::new();
        let mut option = |key, label, selected| {
            options.push(ChoiceOption {
                key,
                label,
                selected,
                enabled: true,
            })
        };
        macro_rules! values {
            ($kind:ty, $selected:expr) => {
                for value in <$kind>::ALL {
                    option(value.key(), value.label(), ($selected)(value));
                }
            };
        }
        match facet {
            QueryFacet::Scope => values!(Scope, |v| q.scope == v),
            QueryFacet::Sort => values!(SortBy, |v| q.sort == v),
            QueryFacet::Direction => {
                option(
                    "automatic",
                    crate::vocabulary::AUTOMATIC_DIRECTION.label,
                    q.direction.is_none(),
                );
                values!(Direction, |v| q.direction == Some(v));
            }
            QueryFacet::Group => values!(GroupBy, |v| q.group_by == v),
            QueryFacet::Band => {
                option(
                    "any",
                    crate::vocabulary::ALL_SOUNDS.label,
                    q.rhythm.bands.is_empty() && q.rhythm.state.is_none(),
                );
                values!(Band, |v| q.rhythm.bands.contains(&v));
                values!(RhythmState, |v| q.rhythm.state == Some(v));
            }
            QueryFacet::Method => {
                option(
                    "any",
                    crate::vocabulary::ALL_METHODS.label,
                    q.rhythm.methods.is_empty(),
                );
                values!(Method, |v| q.rhythm.methods.contains(&v));
            }
            QueryFacet::Population => {
                option(
                    "any",
                    crate::vocabulary::ANY_POPULATION.label,
                    q.populations.is_empty(),
                );
                values!(Population, |v| q.populations.contains(&v));
            }
            QueryFacet::Role => {
                option(
                    "any",
                    crate::vocabulary::ANY_CONDITION_ROLE.label,
                    q.roles.is_empty(),
                );
                values!(ConditionRole, |v| q.roles.contains(&v));
            }
            QueryFacet::Analysis => {
                option(
                    "any",
                    crate::vocabulary::ANY_ANALYSIS_COVERAGE.label,
                    q.analysis.is_none(),
                );
                values!(Completeness, |v| q.analysis == Some(v));
            }
        }
        ChoiceView { options }
    }
    pub(super) fn edit_facets(&mut self, facets: Vec<(QueryFacet, String)>) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        self.assign_facets(&mut query, facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }
    fn assign_facets(
        &self,
        query: &mut CatalogQuery,
        facets: Vec<(QueryFacet, String)>,
    ) -> Result<(), Refusal> {
        for (facet, key) in facets {
            // Accept only actual offered keys, including the explicit absence key.
            if !self
                .library_choices(facet)
                .options
                .iter()
                .any(|option| option.key == key)
            {
                return Err(Refusal::SelectionChanged);
            }
            match facet {
                QueryFacet::Scope => {
                    query.scope = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Sort => {
                    query.sort = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Direction => query.direction = key.parse().ok(),
                QueryFacet::Group => {
                    query.group_by = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Band => {
                    query.rhythm.bands = key.parse::<Band>().into_iter().collect();
                    query.rhythm.state = key.parse().ok();
                }
                QueryFacet::Method => query.rhythm.methods = key.parse().into_iter().collect(),
                QueryFacet::Population => query.populations = key.parse().into_iter().collect(),
                QueryFacet::Role => query.roles = key.parse().into_iter().collect(),
                QueryFacet::Analysis => query.analysis = key.parse().ok(),
            }
        }
        Ok(())
    }
    pub(super) fn edit_filters(&mut self, input: FilterInput) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        let ranges = (|| {
            use crate::discovery::query::CalendarField;
            let [created, modified, added] = input.dates;
            Ok::<_, crate::discovery::query::QueryError>([
                crate::discovery::calendar::local_range(
                    CalendarField::Created,
                    &created[0],
                    &created[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Modified,
                    &modified[0],
                    &modified[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Added,
                    &added[0],
                    &added[1],
                )?,
            ])
        })();
        let [created, modified, added] = ranges.map_err(|error| {
            self.library_filter_error(error.clone());
            Refusal::Document(Box::new(error.into()))
        })?;
        query.created = created;
        query.modified = modified;
        query.added = added;
        query.studies = if input.study.trim().is_empty() {
            Vec::new()
        } else {
            vec![input.study]
        };
        self.assign_facets(&mut query, input.facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }
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
impl Workspace {
    /// Every menu, including its current selection and typed command payloads.
    #[must_use]
    pub fn control_choices(&self) -> Vec<ControlChoices> {
        use ControlValue as V;
        let document = self.editor.session.document_key();
        let mut menus = Vec::new();
        let mut add = |menu, row, values: Vec<(V, &'static str)>, selected: Option<V>| {
            menus.push(ControlChoices {
                menu,
                row,
                view: ChoiceView {
                    options: values
                        .into_iter()
                        .map(|(key, label)| ChoiceOption {
                            key,
                            label,
                            selected: selected == Some(key),
                            enabled: match key {
                                ControlValue::AddCarrier(kind) => {
                                    self.component_add_admission(row, kind).is_ok()
                                }
                                ControlValue::AddPair(kind) => {
                                    self.pair_add_admission(row, kind).is_ok()
                                }
                                ControlValue::MonitorSpan(_) | ControlValue::WaveformScale(_) => {
                                    !self.audio.output.held()
                                }
                                _ => true,
                            },
                        })
                        .collect(),
                },
            });
        };
        for component in self.editor.session.draft().components() {
            let row = self
                .editor
                .session
                .component_key(component.id)
                .unwrap_or_else(|| unreachable!("component"));
            add(
                ControlMenu::Ears,
                row,
                Ears::ALL
                    .into_iter()
                    .map(|v| (V::Ears(v), v.label()))
                    .collect(),
                component.form.route().map(V::Ears),
            );
            add(
                ControlMenu::Carrier,
                row,
                CarrierKind::ALL
                    .into_iter()
                    .map(|v| (V::Carrier(v), v.label()))
                    .collect(),
                component.form.primitive_kind().map(V::Carrier),
            );
            add(
                ControlMenu::Modulation,
                row,
                controls::modulation_choices()
                    .into_iter()
                    .map(|v| {
                        (
                            V::Modulation(v),
                            v.map_or(
                                crate::core::vocabulary::NO_MODULATION.label,
                                ModulationKind::label,
                            ),
                        )
                    })
                    .collect(),
                Some(V::Modulation(component.form.modulation_kind())),
            );
        }
        add(
            ControlMenu::AddComponent,
            document,
            CarrierKind::ALL
                .into_iter()
                .map(|v| (V::AddCarrier(v), ""))
                .chain(
                    crate::core::PairKind::ALL
                        .into_iter()
                        .map(|v| (V::AddPair(v), "")),
                )
                .collect(),
            None,
        );
        add(
            ControlMenu::MonitorSpan,
            document,
            controls::MONITOR_SPANS_MS
                .into_iter()
                .map(|v| (V::MonitorSpan(v), ""))
                .collect(),
            Some(V::MonitorSpan(self.audio.output.span_ms())),
        );
        add(
            ControlMenu::WaveformScale,
            document,
            crate::workspace::monitor::WaveformScale::ALL
                .into_iter()
                .map(|v| (V::WaveformScale(v), ""))
                .collect(),
            Some(V::WaveformScale(self.audio.output.scale())),
        );
        add(
            ControlMenu::ExportRate,
            document,
            controls::EXPORT_RATES
                .into_iter()
                .map(|v| (V::ExportRate(v), ""))
                .collect(),
            Some(V::ExportRate(self.editor.export_rate)),
        );
        menus
    }
    pub(super) fn choose_control(
        &mut self,
        row: RowKey,
        value: ControlValue,
    ) -> Result<(), Refusal> {
        if !self.control_choices().iter().any(|menu| {
            menu.row == row
                && menu
                    .view
                    .options
                    .iter()
                    .any(|o| o.key == value && o.enabled)
        }) {
            return Err(Refusal::SelectionChanged);
        }
        let command = match value {
            ControlValue::Ears(ears) => Command::Editor(EditorCommand::SetEars { row, ears }),
            ControlValue::Carrier(kind) => Command::Editor(EditorCommand::SetCarrier { row, kind }),
            ControlValue::Modulation(kind) => {
                Command::Editor(EditorCommand::SetModulation { row, kind })
            }
            ControlValue::AddCarrier(kind) => {
                Command::Editor(EditorCommand::AddComponent { row, kind })
            }
            ControlValue::AddPair(kind) => Command::Editor(EditorCommand::AddPair { row, kind }),
            ControlValue::ExportRate(rate) => Command::Editor(EditorCommand::SetExportRate(rate)),
            ControlValue::MonitorSpan(span_ms) => Command::Output(OutputCommand::SetWaveform {
                span_ms,
                scale: self.audio.output.scale(),
            }),
            ControlValue::WaveformScale(scale) => Command::Output(OutputCommand::SetWaveform {
                span_ms: self.audio.output.span_ms(),
                scale,
            }),
        };
        self.apply(command)
    }
}
