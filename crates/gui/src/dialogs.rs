//! Parent-bound asynchronous native file selection, separate from operation workers.
use super::{Desktop, Operation, Work};
use slint::ComponentHandle;
use std::{future::Future, rc::Rc};
impl Desktop {
    fn choose(
        self: &Rc<Self>,
        operation: Operation,
        dialog: impl Future<Output = Option<Work>> + 'static,
    ) {
        if self.dialog_active.replace(true) {
            return;
        }
        let desktop = Rc::clone(self);
        if let Err(error) = slint::spawn_local(async move {
            let result = dialog.await;
            desktop.dialog_active.set(false);
            if let Some(work) = result {
                desktop.start(operation, work);
            } else {
                desktop.refresh();
            }
        }) {
            self.dialog_active.set(false);
            self.app.borrow_mut().status = error.to_string();
        }
        self.refresh();
    }
    pub(super) fn choose_import(self: &Rc<Self>) {
        if self.app.borrow().operation.is_some() || self.dialog_active.get() {
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.choose(Operation::Import, async move {
            rfd::AsyncFileDialog::new()
                .set_parent(&ui.window().window_handle())
                .add_filter("TOML recipe", &["toml"])
                .pick_file()
                .await
                .map(|file| Work::Import(file.path().to_owned()))
        });
    }
    pub(super) fn choose_save(self: &Rc<Self>) {
        let draft = {
            let mut app = self.app.borrow_mut();
            if app.operation.is_some() || self.dialog_active.get() {
                return;
            }
            let rate = app.rate;
            app.workbench.commit_pending(rate);
            if !app.workbench.can_save() {
                drop(app);
                self.refresh();
                return;
            }
            app.workbench.draft.clone()
        };
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        self.choose(Operation::Save, async move {
            rfd::AsyncFileDialog::new()
                .set_parent(&ui.window().window_handle())
                .set_file_name("sound.toml")
                .add_filter("TOML recipe", &["toml"])
                .save_file()
                .await
                .map(|file| Work::Save(draft, file.path().to_owned()))
        });
    }
    pub(super) fn choose_export(self: &Rc<Self>) {
        let (sound, name) = {
            let mut app = self.app.borrow_mut();
            if app.operation.is_some() || self.dialog_active.get() {
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
            rfd::AsyncFileDialog::new()
                .set_parent(&ui.window().window_handle())
                .set_file_name(name)
                .add_filter("WAV audio", &["wav"])
                .save_file()
                .await
                .map(|file| Work::Export(sound, file.path().to_owned()))
        });
    }
}
