//! Typed view projection and keyed reconciliation of retained native widget models.
use super::{
    ComponentEntry, ComponentRow, ComponentView, ControlMenu, EditingField, EditorView,
    FieldHandle, MainWindow, MetadataField, MetadataRow, MetadataView, ModelLane, NumberStyle,
    Operation, ParameterRow, ParameterView, RowKey, SlintAdapter, ViewCaption, scope,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{rc::Rc, sync::Arc};

impl SlintAdapter {
    // ---- projection ----

    pub(super) fn project(self: &Rc<Self>) {
        let Some(ui) = self.main.upgrade() else {
            return;
        };
        if self.updating.replace(true) {
            return;
        }
        let state = self
            .workspace
            .borrow()
            .as_ref()
            .map(|w| (w.revision(), w.audio_phase()));
        let Some(state) = state else {
            self.updating.set(false);
            return;
        };
        if self.projected.get() != Some(state) {
            self.projected.set(Some(state));
            self.project_document(&ui);
        }
        self.project_telemetry(&ui);
        self.sync_auxiliary(&ui);
        self.schedule();
        self.updating.set(false);
    }

    pub(super) fn project_document(&self, ui: &MainWindow) {
        self.counts
            .document_projections
            .set(self.counts.document_projections.get() + 1);
        let views = self.workspace.borrow_mut().as_mut().map(|w| {
            (
                w.editor_view(),
                w.library_view(),
                w.action_view(),
                w.control_choices(),
            )
        });
        let Some((editor, library, actions, controls)) = views else {
            return;
        };
        *self.control_choices.borrow_mut() = controls;
        self.project_control_choices(ui);
        self.project_view_captions(ui, &editor);
        self.reconcile_document(ui, &editor);
        if let Some(workspace) = self.workspace.borrow().as_ref() {
            ui.set_draft_location(
                auditorylab_presentation::library::workspace_document_location_label(workspace)
                    .into(),
            );
            ui.set_draft_location_details(read_only(
                &auditorylab_presentation::library::workspace_document_location(workspace),
            ));
        }
        ui.set_dirty(editor.dirty);
        let editor_text = auditorylab_presentation::workspace::editor_view(&editor);
        ui.set_draft_brief(read_only(&editor_text.brief));
        ui.set_peak(read_only(&editor_text.peak));
        ui.set_metadata_error(read_only(&editor_text.metadata_error));
        ui.set_based_on_title(read_only(&editor_text.based_on_title));
        ui.set_based_on(read_only(&editor_text.based_on));
        ui.set_retained_warning(read_only(&editor_text.retained_warning));

        self.project_library(ui, library);
        if ui.get_query() != self.query() {
            ui.set_query(self.query().into());
        }

        self.project_actions(&actions);
        let action_text = auditorylab_presentation::workspace::action_view(&actions);
        ui.set_can_play(actions.can_play());
        ui.set_library_play_action(action_text.library_play_label.into());
        ui.set_can_import(actions.can_import());
        ui.set_can_manage_library(actions.can_manage_library());
        ui.set_can_add_to_library(actions.can_add_to_library());
        ui.set_can_request_trash(actions.can_request_trash());
        ui.set_can_new(actions.can_new());
        ui.set_can_copy(actions.can_copy());
        ui.set_copy_reason(read_only(&action_text.copy_reason));
        ui.set_operation_summary(read_only(&action_text.operation_summary));
        ui.set_operation_outcome(read_only(&action_text.operation_outcome));
        ui.set_can_preview(actions.can_preview());
        ui.set_play_reason(read_only(&action_text.play_reason));
        ui.set_can_save(actions.can_save());
        ui.set_can_save_copy(actions.can_save_copy());
        ui.set_can_export_recipe(actions.can_export_recipe());
        ui.set_save_reason(read_only(&action_text.save_reason));
        ui.set_can_export(actions.can_export());
        ui.set_export_reason(read_only(&action_text.export_reason));
        ui.set_can_edit(actions.can_edit());
        ui.set_can_add_component(actions.can_add_component());
        ui.set_can_add_pair(actions.can_add_pair());
        ui.set_pair_add_reason(read_only(&action_text.pair_add_reason));
        ui.set_add_reason(read_only(&action_text.add_component_reason));
        ui.set_removed_component(read_only(&action_text.removed_component));
        ui.set_can_restore_component(actions.can_restore_component());
        ui.set_playback_busy(actions.playback_busy());
        ui.set_exporting(actions.exporting);
        ui.set_transport_enabled(actions.transport_enabled());
        ui.set_transport_action(read_only(&action_text.transport_label));
        ui.set_play_target(read_only(&action_text.play_target));
        ui.set_file_activity(read_only(&action_text.file_activity));
        ui.set_status(read_only(&action_text.operation_details));
    }

    pub(super) fn query(&self) -> String {
        self.workspace
            .borrow()
            .as_ref()
            .map(|w| w.query().to_owned())
            .unwrap_or_default()
    }

    /// Reconcile the document's rows by identity. Only a replaced document
    /// resets the models; otherwise rows are inserted, removed or updated
    /// one by one, so surviving fields keep their widgets, focus and caret.
    pub(super) fn reconcile_document(&self, ui: &MainWindow, editor: &EditorView) {
        let replaced = self.document.get().is_none_or(|key| key != editor.key);
        if replaced {
            // Rebinding: no automatic blur commit may reach the new document.
            let suspended = ui.get_edits_suspended();
            ui.set_edits_suspended(true);
            self.document.set(Some(editor.key));
            let mut entries = self.entries.borrow_mut();
            entries.clear();
            let rows: Vec<_> = editor
                .components
                .iter()
                .map(|component| {
                    let entry = Self::new_entry(component, editor);
                    let row =
                        component_row(&entry, component, editor, &self.control_choices.borrow());
                    entries.push(entry);
                    row
                })
                .collect();
            self.components.set_vec(rows);
            set_metadata(ui, editor);
            ui.set_document(editor.key.to_string().into());
            ui.set_document_reset(ui.get_document_reset().wrapping_add(1));
            ui.set_editor_scroll_y(0.0);
            ui.set_edits_suspended(suspended);
            return;
        }
        set_metadata(ui, editor);
        let mut entries = self.entries.borrow_mut();
        let mut index = 0;
        while index < entries.len() {
            if editor
                .components
                .iter()
                .any(|c| c.key.same_owner(entries[index].key))
            {
                index += 1;
            } else {
                entries.remove(index);
                self.components.remove(index);
            }
        }
        for (index, component) in editor.components.iter().enumerate() {
            match entries.iter().position(|e| e.key.same_owner(component.key)) {
                Some(found) if found == index => {}
                Some(found) => {
                    let entry = entries.remove(found);
                    let row = self.components.row_data(found);
                    self.components.remove(found);
                    entries.insert(index, entry);
                    if let Some(row) = row {
                        self.components.insert(index, row);
                    }
                }
                None => {
                    let entry = Self::new_entry(component, editor);
                    self.components.insert(
                        index,
                        component_row(&entry, component, editor, &self.control_choices.borrow()),
                    );
                    entries.insert(index, entry);
                    continue;
                }
            }
            entries[index].key = component.key;
            let entry = &entries[index];
            reconcile_parameters(&entry.parameters, &component.parameters, editor);
            let row = component_row(entry, component, editor, &self.control_choices.borrow());
            if self.components.row_data(index).as_ref() != Some(&row) {
                self.components.set_row_data(index, row);
            }
        }
    }

    pub(super) fn new_entry(component: &ComponentView, editor: &EditorView) -> ComponentEntry {
        ComponentEntry {
            key: component.key,
            parameters: Rc::new(VecModel::from(
                component
                    .parameters
                    .iter()
                    .map(|field| parameter_row(field, editor))
                    .collect::<Vec<_>>(),
            )),
        }
    }

    pub(super) fn project_telemetry(&self, ui: &MainWindow) {
        let views = self.workspace.borrow().as_ref().map(|w| {
            (
                w.transport_view(),
                w.output_view(),
                w.model_view(),
                w.export_progress(),
            )
        });
        let Some((transport, output, model, progress)) = views else {
            return;
        };
        let transport_text = auditorylab_presentation::workspace::transport_view(&transport);
        ui.set_player_title(read_only(&transport_text.player_title));
        ui.set_elapsed(read_only(&transport_text.elapsed));
        ui.set_generated(read_only(&transport_text.generated));
        ui.set_generated_details(read_only(&transport_text.generated_details));
        ui.set_device(read_only(&transport_text.device));
        ui.set_output_state(transport_text.output_state.into());
        ui.set_receipt_age(read_only(&transport_text.receipt_age));
        ui.set_monitor_warning(read_only(&transport_text.warning));
        ui.set_monitor_warning_live(
            transport.output.unavailable.is_some()
                || transport
                    .monitor
                    .as_ref()
                    .and_then(|monitor| monitor.warning)
                    .is_some_and(
                        auditorylab_app::workspace::monitor::MonitorWarning::announces_changes,
                    ),
        );
        ui.set_backend_warning(read_only(&transport_text.backend_warning));
        ui.set_monitor_state(read_only(&transport_text.monitor_state));
        ui.set_meters(read_only(&transport_text.meters));
        ui.set_waveform_caption(read_only(&transport_text.caption));
        ui.set_model_caption(read_only(&transport_text.model_caption));
        ui.set_motion_reason(read_only(&transport_text.motion));
        ui.set_motion_overridden(transport.hold_override.is_some());
        ui.set_held(output.held);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "A 0..=1 display ratio rounds to the renderer's f32"
        )]
        ui.set_progress(progress as f32);
        self.project_waveforms(ui, output, model);
        ui.set_typography_origin(
            self.preferences
                .borrow()
                .as_ref()
                .map_or_else(String::new, |p| {
                    crate::terms::Caption::typography_values(
                        &p.font.family,
                        p.font.size,
                        &p.font.origin,
                    )
                })
                .into(),
        );
    }

    pub(super) fn project_view_captions(&self, ui: &MainWindow, editor: &EditorView) {
        let monitor = self.monitor.upgrade();
        for captions in std::iter::once(ui.global::<crate::generated::ViewCaptions>()).chain(
            monitor
                .as_ref()
                .map(slint::ComponentHandle::global::<crate::generated::ViewCaptions>),
        ) {
            project_captions(&captions, editor.status);
        }
    }

    pub(super) fn project_actions(&self, view: &auditorylab_app::workspace::WorkspaceActions) {
        let main = self.main.upgrade();
        let audio_controls = self.audio_controls.upgrade();
        for actions in [
            main.as_ref()
                .map(slint::ComponentHandle::global::<crate::generated::Actions>),
            audio_controls
                .as_ref()
                .map(slint::ComponentHandle::global::<crate::generated::Actions>),
        ]
        .into_iter()
        .flatten()
        {
            for view in &view.operations {
                let action = crate::generated::Action {
                    label: auditorylab_presentation::workspace::operation_label(view.operation)
                        .into(),
                    enabled: view.enabled,
                };

                match view.operation {
                    Operation::List => actions.set_list(action),
                    Operation::ListTrash => actions.set_list_trash(action),
                    Operation::Describe => actions.set_describe(action),
                    Operation::Inspect => actions.set_inspect(action),
                    Operation::Recipe => actions.set_recipe(action),
                    Operation::Export => actions.set_export(action),
                    Operation::Play => actions.set_play(action),
                    Operation::Stop => actions.set_stop(action),
                    Operation::LibraryPath => actions.set_library_path(action),
                    Operation::LibraryInit => actions.set_library_init(action),
                    Operation::LibraryAdd => actions.set_library_add(action),
                    Operation::LibraryUpdate => actions.set_library_update(action),
                    Operation::LibraryRename => actions.set_library_rename(action),
                    Operation::LibraryTrash => actions.set_library_trash(action),
                    Operation::LibraryRestore => actions.set_library_restore(action),
                    Operation::LibraryPurge => actions.set_library_purge(action),
                    Operation::LibraryCheck => actions.set_library_check(action),
                    Operation::LibraryRefresh => actions.set_library_refresh(action),
                    Operation::LibraryRead => actions.set_library_read(action),
                    Operation::NewSound => actions.set_new_sound(action),
                    Operation::EditCopy => actions.set_edit_copy(action),
                    Operation::Save => actions.set_save(action),
                    Operation::SaveCopy => actions.set_save_copy(action),
                    Operation::OpenRecipe => actions.set_open_recipe(action),
                    Operation::ExportRecipe => actions.set_export_recipe(action),
                    Operation::StopCurrentAndPlayRecipe => {
                        actions.set_stop_current_and_play_recipe(action);
                    }
                    Operation::PlaySelectedSound => actions.set_play_selected_sound(action),
                    Operation::PlayLive => actions.set_play_live(action),
                    Operation::EditSavedRecipe => actions.set_edit_saved_recipe(action),
                    Operation::EditExternalFile => actions.set_edit_external_file(action),
                    Operation::CloseOpenFile => actions.set_close_open_file(action),
                    Operation::ShowSaved => actions.set_show_saved(action),
                    Operation::CancelExport => actions.set_cancel_export(action),
                    Operation::AddComponent => actions.set_add_component(action),
                    Operation::UndoRemove => actions.set_undo_remove(action),
                    Operation::RemoveComponent => actions.set_remove_component(action),
                }
            }
        }
    }

    pub(super) fn project_control_choices(&self, ui: &MainWindow) {
        let choices = self.control_choices.borrow();
        for menu in choices.iter() {
            let labels = menu
                .view
                .options
                .iter()
                .map(|o| auditorylab_presentation::workspace::control_choice(o).into())
                .collect::<Vec<SharedString>>();
            let strings = ModelRc::new(VecModel::from(labels));
            let selected = selected_option(&menu.view);
            match menu.menu {
                ControlMenu::SourceWidth | ControlMenu::ModulationWidth => {
                    ui.set_pulse_width_choices(strings);
                }
                ControlMenu::SourceShape | ControlMenu::ModulationShape => {
                    ui.set_pulse_shape_choices(strings);
                }
                ControlMenu::ChannelRouting => ui.set_routing_choices(strings),
                ControlMenu::Carrier => ui.set_carrier_choices(strings),
                ControlMenu::Modulation => ui.set_modulation_choices(strings),
                ControlMenu::AddComponent => ui.set_add_choices(ModelRc::new(VecModel::from(
                    menu.view
                        .options
                        .iter()
                        .map(|o| crate::generated::ControlOption {
                            label: auditorylab_presentation::workspace::control_choice(o).into(),
                            enabled: o.enabled,
                        })
                        .collect::<Vec<_>>(),
                ))),
                ControlMenu::MonitorSpan => {
                    ui.set_can_change_waveform(
                        menu.view.options.first().is_some_and(|o| o.enabled),
                    );
                    ui.set_span_choices(strings.clone());
                    ui.set_window_index(selected);
                    if let Some(monitor) = self.monitor.upgrade() {
                        monitor.set_span_choices(strings);
                    }
                }
                ControlMenu::WaveformScale => {
                    ui.set_scale_choices(strings.clone());
                    ui.set_scale_index(selected);
                    if let Some(monitor) = self.monitor.upgrade() {
                        monitor.set_scale_choices(strings);
                    }
                }
                ControlMenu::ExportRate => {
                    ui.set_export_rates(strings);
                    ui.set_rate_index(selected);
                }
            }
        }
    }
    pub(super) fn project_waveforms(
        &self,
        ui: &MainWindow,
        output: auditorylab_app::workspace::output::OutputView,
        model: Option<Arc<auditorylab_app::workspace::monitor::ModelView>>,
    ) {
        let changed = match (&*self.capture.borrow(), &output.capture) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            // Paths are rebuilt only for another capture, never for
            // receipt age or other telemetry.
            self.counts
                .path_builds
                .set(self.counts.path_builds.get() + 1);
            if let Some(capture) = &output.capture {
                let [left, right] = scope::paths(capture.bins(), capture.limit());
                ui.set_left_path(left.into());
                ui.set_right_path(right.into());
                ui.set_scale_label(
                    auditorylab_presentation::numbers::format(
                        NumberStyle::AxisLimit,
                        capture.limit(),
                    )
                    .into(),
                );
                ui.set_capture_duration(
                    auditorylab_presentation::numbers::format(
                        NumberStyle::CapturedMilliseconds,
                        capture.milliseconds(),
                    )
                    .into(),
                );
            } else {
                ui.set_left_path(SharedString::new());
                ui.set_right_path(SharedString::new());
            }
            *self.capture.borrow_mut() = output.capture;
        }
        let model_changed = match (&*self.model.borrow(), &model) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if model_changed {
            sync_rows(
                &self.lanes,
                model.as_ref().map_or_else(Vec::new, |model| {
                    model
                        .projection
                        .lanes
                        .iter()
                        .map(|lane| {
                            let stereo: Vec<_> = lane.columns.iter().map(|e| [*e; 2]).collect();
                            let [trace, _] = scope::paths(&stereo, 1.0);
                            ModelLane {
                                component: lane.id.get().to_string().into(),
                                label:

                                auditorylab_presentation::workspace::monitor::model_view_lane_label(
                                        lane,
                                    )
                                    .into(),
                                trace: trace.into(),
                            }
                        })
                        .collect()
                }),
            );
            *self.model.borrow_mut() = model;
        }
    }
}

pub(super) fn selected_option<K>(view: &auditorylab_app::workspace::ChoiceView<K>) -> i32 {
    view.options
        .iter()
        .position(|o| o.selected)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

pub(super) fn parameter_row(view: &ParameterView, editor: &EditorView) -> ParameterRow {
    let text = auditorylab_presentation::workspace::parameter_text(view, editor);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A normalized slider position rounds to the renderer's f32; values stay binary64"
    )]
    let position = view.slider.unwrap_or(0.0) as f32;
    ParameterRow {
        key: view.handle.to_string().into(),
        label: text.label.as_str().into(),
        help: text.help.into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        has_slider: view.slider.is_some(),
        position,
        diagnostic: text.diagnostic.as_str().into(),
        show_diagnostic: !text.diagnostic.is_empty(),
    }
}

pub(super) fn component_row(
    entry: &ComponentEntry,
    view: &ComponentView,
    editor: &EditorView,
    choices: &[auditorylab_app::workspace::ControlChoices],
) -> ComponentRow {
    let text = auditorylab_presentation::workspace::component_text(view, editor);
    ComponentRow {
        key: entry.key.to_string().into(),
        id: view.id.get().to_string().into(),
        route: control_index(
            choices,
            view.key,
            auditorylab_app::workspace::ControlMenu::ChannelRouting,
        ),
        carrier: control_index(
            choices,
            view.key,
            auditorylab_app::workspace::ControlMenu::Carrier,
        ),
        routing_editable: view.committed.route().is_some(),
        modulation: control_index(
            choices,
            view.key,
            auditorylab_app::workspace::ControlMenu::Modulation,
        ),
        source_width: control_index(choices, view.key, ControlMenu::SourceWidth),
        source_shape: control_index(choices, view.key, ControlMenu::SourceShape),
        modulation_width: control_index(choices, view.key, ControlMenu::ModulationWidth),
        modulation_shape: control_index(choices, view.key, ControlMenu::ModulationShape),
        heading: text.heading.into(),
        name: text.name.as_str().into(),
        context: text.context.as_str().into(),
        error: text.error.as_str().into(),
        parameters: ModelRc::from(entry.parameters.clone()),
    }
}

pub(super) fn same_parameter(a: &str, b: &str) -> bool {
    match (a.parse::<FieldHandle>(), b.parse::<FieldHandle>()) {
        (Ok(a), Ok(b)) => a.row.same_owner(b.row) && a.field == b.field,
        _ => false,
    }
}

/// Reconcile one component's parameters by key with minimal notifications.
pub(super) fn reconcile_parameters(
    model: &VecModel<ParameterRow>,
    views: &[ParameterView],
    editor: &EditorView,
) {
    let mut rows: Vec<_> = views
        .iter()
        .map(|field| parameter_row(field, editor))
        .collect();
    let mut shown = std::collections::BTreeSet::new();
    for row in rows.iter_mut().rev() {
        row.show_diagnostic = !row.diagnostic.is_empty() && shown.insert(row.diagnostic.clone());
    }
    let mut index = 0;
    while index < model.row_count() {
        if model
            .row_data(index)
            .is_some_and(|row| rows.iter().any(|r| same_parameter(&r.key, &row.key)))
        {
            index += 1;
        } else {
            model.remove(index);
        }
    }
    for (index, row) in rows.into_iter().enumerate() {
        let found = (index..model.row_count()).find(|&i| {
            model
                .row_data(i)
                .is_some_and(|r| same_parameter(&r.key, &row.key))
        });
        match found {
            Some(i) if i == index => {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
            Some(i) => {
                model.remove(i);
                model.insert(index, row);
            }
            None => model.insert(index, row),
        }
    }
}

pub(super) fn metadata_row(view: &MetadataView, editor: &EditorView) -> MetadataRow {
    let text = auditorylab_presentation::workspace::metadata_text(view, editor);
    MetadataRow {
        key: view.handle.to_string().into(),
        label: text.label.as_str().into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        diagnostic: text.diagnostic.as_str().into(),
    }
}

pub(super) fn set_metadata(ui: &MainWindow, editor: &EditorView) {
    for view in &editor.metadata {
        let EditingField::Metadata(field) = view.handle.field else {
            continue;
        };
        let row = metadata_row(view, editor);
        match field {
            MetadataField::Title => ui.set_title_field(row),
            MetadataField::Summary => ui.set_summary_field(row),
            MetadataField::Duration => ui.set_duration_field(row),
            MetadataField::Fade => ui.set_fade_field(row),
        }
    }
}

/// Positional update for read-only lists whose rows have no identity of their own.
pub(super) fn sync_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for (index, row) in rows.into_iter().enumerate() {
        if index < model.row_count() {
            if model.row_data(index).as_ref() != Some(&row) {
                model.set_row_data(index, row);
            }
        } else {
            model.push(row);
        }
    }
}

// Keep display escaping separate from lossless editable field models and bindings.
pub(super) fn read_only(text: &str) -> SharedString {
    auditorylab_presentation::display_text(text).into()
}

pub(super) fn control_index(
    choices: &[auditorylab_app::workspace::ControlChoices],
    row: RowKey,
    menu: auditorylab_app::workspace::ControlMenu,
) -> i32 {
    choices
        .iter()
        .find(|c| c.row == row && c.menu == menu)
        .map_or(-1, |c| selected_option(&c.view))
}

pub(crate) fn project_captions(
    captions: &crate::generated::ViewCaptions<'_>,
    status: auditorylab_app::vocabulary::EditorStatus,
) {
    captions.set_editor_status(auditorylab_presentation::workspace::editor_status(status).into());
    for caption in auditorylab_app::vocabulary::ViewCaption::VALUES {
        let label = auditorylab_presentation::workspace::view_caption(*caption).into();

        match caption {
            ViewCaption::AddedHereThroughInclusiveLocalCalendarDate => {
                captions.set_added_through_date(label);
            }
            ViewCaption::AddedHereThroughYyyyMmDd => {
                captions.set_added_here_through_yyyy_mm_dd(label);
            }
            ViewCaption::AddedHereFromLocalCalendarDate => captions.set_added_from_date(label),
            ViewCaption::AddedHereFromYyyyMmDd => captions.set_added_here_from_yyyy_mm_dd(label),
            ViewCaption::ExportSampleRate => captions.set_export_sample_rate(label),
            ViewCaption::AppliedLibraryFiltersPrefix => {
                captions.set_applied_library_filters_prefix(label);
            }
            ViewCaption::AppliedLibraryFilters => captions.set_applied_library_filters(label),
            ViewCaption::StudyEvidenceFilter => captions.set_study_evidence_filter(label),
            ViewCaption::PopulationEvidenceFilter => captions.set_population_evidence_filter(label),
            ViewCaption::ConditionRoleEvidenceFilter => {
                captions.set_condition_role_evidence_filter(label);
            }
            ViewCaption::NominalAnalysisCoverage => captions.set_nominal_analysis_coverage(label),
            ViewCaption::SortDirection => captions.set_sort_direction(label),
            ViewCaption::LibraryResultGrouping => captions.set_library_result_grouping(label),
            ViewCaption::CreatedFromLocalCalendarDate => {
                captions.set_created_from_local_calendar_date(label);
            }
            ViewCaption::CreatedThroughInclusiveLocalCalendarDate => {
                captions.set_created_through_date(label);
            }
            ViewCaption::ModifiedFromLocalCalendarDate => {
                captions.set_modified_from_local_calendar_date(label);
            }
            ViewCaption::ModifiedThroughInclusiveLocalCalendarDate => {
                captions.set_modified_through_date(label);
            }
            ViewCaption::ExactUtcDates => captions.set_exact_utc_dates(label),
            ViewCaption::ExactUtcRecipeAndMembershipDates => {
                captions.set_exact_utc_dates_description(label);
            }
            ViewCaption::Library => captions.set_library(label),
            ViewCaption::PlaybackTransport => captions.set_playback_transport(label),
            ViewCaption::CommittedLibraryQuery => captions.set_committed_library_query(label),
            ViewCaption::Collections => captions.set_collections(label),
            ViewCaption::DatesIdentity => captions.set_dates_identity(label),
            ViewCaption::SoundFacts => captions.set_sound_facts(label),
            ViewCaption::OutputMonitor => captions.set_output_monitor(label),
            ViewCaption::Left => captions.set_left(label),
            ViewCaption::Right => captions.set_right(label),
            ViewCaption::CapturedEnd => captions.set_captured_end(label),
            ViewCaption::WaveformTimeWindow => captions.set_waveform_time_window(label),
            ViewCaption::WaveformAmplitudeScale => captions.set_waveform_amplitude_scale(label),
            ViewCaption::WavExportProgress => captions.set_wav_export_progress(label),
            ViewCaption::LibraryCollection => captions.set_library_collection(label),
            ViewCaption::NominalRhythmReferenceBand => {
                captions.set_nominal_rhythm_reference_band(label);
            }
            ViewCaption::StructuralMethod => captions.set_structural_method(label),
            ViewCaption::StudyDoiOrKeySource => captions.set_study_doi_or_key_source(label),
            ViewCaption::CreatedFromYyyyMmDd => captions.set_created_from_yyyy_mm_dd(label),
            ViewCaption::CreatedThroughYyyyMmDd => captions.set_created_through_yyyy_mm_dd(label),
            ViewCaption::ModifiedFromYyyyMmDd => captions.set_modified_from_yyyy_mm_dd(label),
            ViewCaption::ModifiedThroughYyyyMmDd => captions.set_modified_through_yyyy_mm_dd(label),
            ViewCaption::Workbench => captions.set_workbench(label),
            ViewCaption::SoundTitle => captions.set_sound_title(label),
            ViewCaption::ChannelRouting => captions.set_channel_routing(label),
            ViewCaption::Carrier => captions.set_carrier(label),
            ViewCaption::Modulation => captions.set_modulation(label),
            ViewCaption::SoundNotes => captions.set_sound_notes(label),
            ViewCaption::AudioDetails => captions.set_audio_details(label),
        }
    }
}
