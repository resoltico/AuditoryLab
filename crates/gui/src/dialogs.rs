//! The production shell: parent-bound asynchronous native dialogs and file
//! panels, and the platform's answer to a close request.
use crate::adapter::{Shell, SlintAdapter};
use auditorylab_app::workspace::{PathPurpose, PromptAnswer, PromptId};
use std::{future::Future, rc::Rc};

/// Native dialogs and application termination.
pub(crate) struct NativeShell;

/// Run a dialog on the UI thread and deliver its answer to the workspace. A
/// dialog that cannot be shown answers its prompt with the failure, so the
/// workspace never waits on it.
fn answer(
    adapter: &Rc<SlintAdapter>,
    prompt: PromptId,
    dialog: impl Future<Output = PromptAnswer> + 'static,
) {
    let target = Rc::clone(adapter);
    if let Err(error) = slint::spawn_local(async move {
        let response = dialog.await;
        target.respond(prompt, response);
    }) {
        adapter.respond(prompt, PromptAnswer::Failed(error.to_string()));
    }
}

impl Shell for NativeShell {
    fn ask_purge(&self, adapter: &Rc<SlintAdapter>, prompt: PromptId, key: String, title: String) {
        let Some(ui) = adapter.main_window() else {
            adapter.respond(prompt, PromptAnswer::Cancel);
            return;
        };
        answer(adapter, prompt, async move {
            use slint::ComponentHandle;
            let choice = rfd::AsyncMessageDialog::new()
                .set_parent(&ui.window().window_handle())
                .set_title(crate::terms::Action::PurgeConfirmation.label())
                .set_description(auditorylab_presentation::vocabulary::purge_description(
                    &title, &key,
                ))
                .set_buttons(rfd::MessageButtons::OkCancelCustom(
                    auditorylab_app::core::vocabulary::Operation::LibraryPurge
                        .label()
                        .into(),
                    crate::terms::Action::Cancel.label().into(),
                ))
                .show()
                .await;
            match choice {
                rfd::MessageDialogResult::Ok => PromptAnswer::Confirm,
                rfd::MessageDialogResult::Custom(label)
                    if label
                        == auditorylab_app::core::vocabulary::Operation::LibraryPurge.label() =>
                {
                    PromptAnswer::Confirm
                }
                _ => PromptAnswer::Cancel,
            }
        });
    }

    fn ask_unsaved(&self, adapter: &Rc<SlintAdapter>, prompt: PromptId) {
        let Some(ui) = adapter.main_window() else {
            adapter.respond(prompt, PromptAnswer::Cancel);
            return;
        };
        let description =
            auditorylab_presentation::vocabulary::unsaved_description(&adapter.document_title());
        answer(adapter, prompt, async move {
            use slint::ComponentHandle;
            let choice = rfd::AsyncMessageDialog::new()
                .set_parent(&ui.window().window_handle())
                .set_title(crate::terms::Status::UnsavedSound.label())
                .set_description(description)
                .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                    crate::terms::Action::Save.label().into(),
                    crate::terms::Action::Discard.label().into(),
                    crate::terms::Action::Cancel.label().into(),
                ))
                .show()
                .await;
            match choice {
                rfd::MessageDialogResult::Yes => PromptAnswer::Save,
                rfd::MessageDialogResult::No => PromptAnswer::Discard,
                rfd::MessageDialogResult::Custom(label)
                    if label == crate::terms::Action::Save.label() =>
                {
                    PromptAnswer::Save
                }
                rfd::MessageDialogResult::Custom(label)
                    if label == crate::terms::Action::Discard.label() =>
                {
                    PromptAnswer::Discard
                }
                _ => PromptAnswer::Cancel,
            }
        });
    }

    fn choose_path(
        &self,
        adapter: &Rc<SlintAdapter>,
        prompt: PromptId,
        purpose: PathPurpose,
        suggested_name: Option<String>,
    ) {
        let Some(ui) = adapter.main_window() else {
            adapter.respond(prompt, PromptAnswer::Path(None));
            return;
        };
        let extension = match purpose {
            PathPurpose::OpenRecipe | PathPurpose::ExportRecipe => "toml",
            PathPurpose::ExportWav => "wav",
        };
        let destination = match purpose {
            PathPurpose::OpenRecipe => None,
            PathPurpose::ExportRecipe | PathPurpose::ExportWav => Some(
                suggested_name
                    .unwrap_or_else(|| crate::terms::Caption::suggested_filename(extension)),
            ),
        };
        // A native Stop control in the panel stops exactly the audio playing now.
        let control = adapter.audio_control();
        answer(adapter, prompt, async move {
            match file_path(&ui, destination.as_deref(), extension, control).await {
                Ok(path) => PromptAnswer::Path(path),
                Err(error) => PromptAnswer::Failed(error),
            }
        });
    }

    fn close_resolved(&self, adapter: &SlintAdapter, approved: bool) {
        #[cfg(target_os = "macos")]
        if let Some(request) = adapter.take_termination() {
            crate::native_application::reply_to_termination(request, approved);
            return;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = adapter;
        if approved && let Err(error) = slint::quit_event_loop() {
            eprintln!(
                "{}",
                crate::native_text::InterfaceOperation::Quit.failure(error)
            );
        }
    }
}

/// Platform file selection with the same native overwrite contract on every caller.
async fn file_path(
    ui: &crate::MainWindow,
    filename: Option<&str>,
    extension: &str,
    control: Option<auditorylab_app::services::playback::PlaybackControl>,
) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        crate::native_application::choose_file(ui, filename, extension, control).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        use slint::ComponentHandle;
        // Elsewhere the separate audio controls window offers Stop while a panel is open.
        drop(control);
        let dialog = rfd::AsyncFileDialog::new()
            .set_parent(&ui.window().window_handle())
            .add_filter(
                if extension == "wav" {
                    crate::terms::Caption::WavAudio.label()
                } else {
                    crate::terms::Caption::TomlSound.label()
                },
                &[extension],
            );
        let selected = if let Some(name) = filename {
            dialog.set_file_name(name).save_file().await
        } else {
            dialog.pick_file().await
        };
        Ok(selected.map(|file| file.path().to_owned()))
    }
}
