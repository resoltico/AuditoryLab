//! Parent-bound asynchronous native file selection, separate from operation workers.
use super::{Desktop, Operation, Work};
#[cfg(not(target_os = "macos"))]
use slint::ComponentHandle;
use std::{future::Future, rc::Rc};
impl Desktop {
    fn choose(
        self: &Rc<Self>,
        operation: Operation,
        dialog: impl Future<Output = Result<Option<Work>, String>> + 'static,
    ) {
        if self.dialog_active.replace(true) {
            return;
        }
        let desktop = Rc::clone(self);
        if let Err(error) = slint::spawn_local(async move {
            let result = dialog.await;
            desktop.dialog_active.set(false);
            match result {
                Ok(Some(work)) => desktop.start(operation, work),
                Ok(None) => {
                    desktop.refresh();
                }
                Err(error) => {
                    desktop.app.borrow_mut().status = error;
                    desktop.refresh();
                }
            }
        }) {
            self.dialog_active.set(false);
            self.app.borrow_mut().status = error.to_string();
        }
        self.refresh();
    }
    pub(super) fn choose_import(self: &Rc<Self>) {
        if self.app.borrow().file_operation.is_some() || self.dialog_active.get() {
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.choose(Operation::Import, async move {
            Ok(file_path(&ui, None, "toml").await?.map(Work::Import))
        });
    }
    pub(super) fn choose_save(self: &Rc<Self>) {
        let draft = {
            let mut app = self.app.borrow_mut();
            if app.file_operation.is_some() || self.dialog_active.get() {
                return;
            }
            let rate = app.rate;
            app.workbench.commit_pending(rate);
            if !app.workbench.can_save() {
                app.status = app.workbench.draft.to_toml().err().map_or_else(
                    || "Correct the numeric fields before saving.".into(),
                    |error| error.to_string(),
                );
                drop(app);
                self.refresh();
                return;
            }
            (app.workbench.draft.clone(), app.workbench.identity.clone())
        };
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.choose(Operation::Save, async move {
            Ok(file_path(&ui, Some("sound.toml"), "toml")
                .await?
                .map(|path| Work::Save(draft.0, path, draft.1)))
        });
    }
    pub(super) fn choose_export(self: &Rc<Self>) {
        let (sound, name) = {
            let mut app = self.app.borrow_mut();
            if app.operation.is_some() || app.file_operation.is_some() || self.dialog_active.get() {
                return;
            }
            let rate = app.rate;
            app.workbench.commit_pending(rate);
            if app.editing && !app.workbench.can_save() {
                drop(app);
                self.refresh();
                return;
            }
            let name = match (app.editing, app.preset().origin()) {
                (false, auditorylab::Origin::BuiltIn(id)) => format!("{id}.wav"),
                _ => "sound.wav".into(),
            };
            let sound = if app.editing {
                app.workbench.draft.preset().at(rate)
            } else {
                app.preset().at(rate)
            };
            match sound {
                Ok(sound) => (sound, name),
                Err(error) => {
                    app.status = error.to_string();
                    drop(app);
                    self.refresh();
                    return;
                }
            }
        };
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.choose(Operation::Export, async move {
            Ok(file_path(&ui, Some(&name), "wav")
                .await?
                .map(|path| Work::Export(sound, path)))
        });
    }
}

/// Platform file selection with the same native overwrite contract on every caller.
pub(super) async fn file_path(
    ui: &super::MainWindow,
    filename: Option<&str>,
    extension: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        super::native_application::choose_file(ui, filename, extension).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let dialog = rfd::AsyncFileDialog::new()
            .set_parent(&ui.window().window_handle())
            .add_filter(
                if extension == "wav" {
                    "WAV audio"
                } else {
                    "TOML sound"
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
