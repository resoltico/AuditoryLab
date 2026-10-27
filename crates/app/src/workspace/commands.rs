//! Typed user input; handlers retain their owner-specific admission/status rules.
use super::*;
use crate::discovery::{collection::CatalogQuery, query::RhythmQuery};
use crate::workspace::monitor::WaveformScale;

/// One requested workspace transition. Display labels are never command input.
#[derive(Debug, Clone)]
pub enum Command {
    /// Change the interaction surface.
    SetView(View),
    /// Edit the current document or request its replacement.
    Editor(EditorCommand),
    /// Query or act on the current library listing.
    Library(LibraryCommand),
    /// Control the audio task.
    Audio(AudioCommand),
    /// Request file work.
    File(FileCommand),
    /// Change monitor/preview policy.
    Output(OutputCommand),
    /// Request the shared close decision.
    Close,
}
/// Document input whose identities remain bound to the captured editor lifetime.
#[derive(Debug, Clone)]
pub enum EditorCommand {
    New,
    CopySelected,
    SetExportRate(SampleRate),
    Stage {
        field: FieldHandle,
        text: String,
    },
    Restore(FieldHandle),
    CommitField {
        field: FieldHandle,
        text: String,
    },
    Commit,
    Slide {
        field: FieldHandle,
        position: f64,
    },
    SetEars {
        row: RowKey,
        ears: Ears,
    },
    SetCarrier {
        row: RowKey,
        kind: CarrierKind,
    },
    SetModulation {
        row: RowKey,
        kind: Option<ModulationKind>,
    },
    AddComponent {
        row: RowKey,
        kind: CarrierKind,
    },
    AddPair {
        row: RowKey,
        kind: crate::core::PairKind,
    },
    RemoveComponent(RowKey),
    RestoreComponent(RowKey),
    ShowBasedOnSource,
}
/// Movement through unique visible entries, independent of display grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
    First,
    Last,
}
/// Library input; row actions carry only opaque app-issued handles.
#[derive(Debug, Clone)]
pub enum LibraryCommand {
    SetQuery(Box<CatalogQuery>),
    SetFacets(Vec<(QueryFacet, String)>),
    SetFilters(Box<FilterInput>),
    ClearFilters,
    ResetSort,
    Search(String),
    SetRhythm(RhythmQuery),
    Select(EntryHandle),
    Navigate(Navigation),
    Refresh,
    Initialize,
    EditSelected,
    EditEntry(EntryHandle),
    AddSelected,
    AddEntry(EntryHandle),
    Mutate {
        entry: EntryHandle,
        mutation: EntryMutation,
    },
    CloseSelected,
    CloseEntry(EntryHandle),
    ShowSaved,
}
/// Audio admission is evaluated against the state used to start/stop work.
#[derive(Debug, Clone, Copy)]
pub enum AudioCommand {
    Play,
    Toggle,
    Stop,
}
/// File requests capture their content before asynchronous work starts.
#[derive(Debug, Clone, Copy)]
pub enum FileCommand {
    Import,
    Save,
    SaveCopy,
    ExportRecipe,
    ExportWav,
    CancelExport,
}
/// Monitor settings do not redefine domain validation or audio admission.
#[derive(Debug, Clone, Copy)]
pub enum OutputCommand {
    SetMotion(MotionPreference),
    SetHold(bool),
    FollowMotion,
    SetWaveform { span_ms: u32, scale: WaveformScale },
    Preview,
}

impl Workspace {
    /// Apply one typed user command through its owning admission handler.
    ///
    /// # Errors
    /// Returns the typed refusal while retaining its owner-specific status.
    pub fn apply(&mut self, command: Command) -> Result<(), Refusal> {
        match command {
            Command::SetView(view) => {
                self.set_view(view);
                Ok(())
            }
            Command::Close => self.close(),
            Command::Editor(command) => match command {
                EditorCommand::New => self.new_document(),
                EditorCommand::CopySelected => self.copy_selected(),
                EditorCommand::SetExportRate(rate) => {
                    self.set_export_rate(rate);
                    Ok(())
                }
                EditorCommand::Stage { field, text } => self.stage(field, &text),
                EditorCommand::Restore(field) => self.restore(field).map(|_| ()),
                EditorCommand::CommitField { field, text } => self.commit_field(field, &text),
                EditorCommand::Commit => self.commit(),
                EditorCommand::Slide { field, position } => self.slide(field, position),
                EditorCommand::SetEars { row, ears } => self.set_ears(row, ears),
                EditorCommand::SetCarrier { row, kind } => self.set_carrier(row, kind),
                EditorCommand::SetModulation { row, kind } => self.set_modulation(row, kind),
                EditorCommand::AddComponent { row, kind } => self.add_component(row, kind),
                EditorCommand::AddPair { row, kind } => self.add_pair(row, kind),
                EditorCommand::RemoveComponent(row) => self.remove_component(row),
                EditorCommand::RestoreComponent(row) => self.restore_component(row),
                EditorCommand::ShowBasedOnSource => {
                    let revealed = self.editor().draft().based_on().is_some();
                    self.show_based_on_source();
                    if revealed {
                        self.effects.push(Effect::ResetLibraryInputs);
                    }
                    Ok(())
                }
            },
            Command::Library(command) => match command {
                LibraryCommand::SetFacets(facets) => self.edit_facets(facets),
                LibraryCommand::SetFilters(input) => self.edit_filters(*input),
                LibraryCommand::ClearFilters => {
                    let query = self.catalog_query().cleared_filters();
                    self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))?;
                    self.effects.push(Effect::ResetLibraryInputs);
                    Ok(())
                }
                LibraryCommand::ResetSort => {
                    let query = self.catalog_query().reset_sort();
                    self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
                }
                LibraryCommand::SetQuery(query) => {
                    self.set_catalog_query(*query).map_err(|error| {
                        self.library_filter_error(error.clone());
                        Refusal::Document(Box::new(error.into()))
                    })
                }
                LibraryCommand::Search(text) => {
                    self.set_query(&text);
                    Ok(())
                }
                LibraryCommand::SetRhythm(query) => {
                    self.set_rhythm_query(query);
                    Ok(())
                }
                LibraryCommand::Select(entry) => self.select_entry(entry),
                LibraryCommand::Navigate(direction) => {
                    self.navigate_library(match direction {
                        Navigation::Previous => -1,
                        Navigation::Next => 1,
                        Navigation::First => -2,
                        Navigation::Last => 2,
                    });
                    Ok(())
                }
                LibraryCommand::Refresh => {
                    self.refresh_library();
                    Ok(())
                }
                LibraryCommand::Initialize => self.initialize_library(),
                LibraryCommand::EditSelected => self.edit_selected(),
                LibraryCommand::EditEntry(entry) => self.edit_entry(entry),
                LibraryCommand::AddSelected => self.add_selected_to_library(),
                LibraryCommand::AddEntry(entry) => self.add_entry(entry),
                LibraryCommand::Mutate { entry, mutation } => self.mutate_entry(entry, mutation),
                LibraryCommand::CloseSelected => {
                    self.close_open_file();
                    Ok(())
                }
                LibraryCommand::CloseEntry(entry) => self.close_entry(entry),
                LibraryCommand::ShowSaved => {
                    if self.show_saved() {
                        self.effects.push(Effect::ResetLibraryInputs);
                    }
                    Ok(())
                }
            },
            Command::Audio(command) => match command {
                AudioCommand::Play => self.play(),
                AudioCommand::Toggle => self.play_stop(),
                AudioCommand::Stop => self.stop().map(|_| ()),
            },
            Command::File(command) => match command {
                FileCommand::Import => self.import(),
                FileCommand::Save => self.save(),
                FileCommand::SaveCopy => self.save_independent_copy(),
                FileCommand::ExportRecipe => self.export_recipe(),
                FileCommand::ExportWav => self.export(),
                FileCommand::CancelExport => self.cancel_export(),
            },
            Command::Output(command) => {
                match command {
                    OutputCommand::SetMotion(preference) => self.set_motion_preference(preference),
                    OutputCommand::SetHold(held) => self.set_hold(held),
                    OutputCommand::FollowMotion => self.follow_motion(),
                    OutputCommand::SetWaveform { span_ms, scale } => {
                        self.set_waveform(span_ms, scale)
                    }
                    OutputCommand::Preview => self.preview_current(),
                };
                Ok(())
            }
        }
    }
}
