//! Destructive navigation, native Save/Discard/Cancel and serialized worker continuations.
use super::{Desktop, Operation, Work, app};
use auditorylab::{Draft, Preset};
use slint::ComponentHandle;
use std::{path::PathBuf, rc::Rc};
pub(super) enum Action {
    New,
    Copy(Preset),
    Close,
}
pub(super) enum Continuation {
    Action(Action),
    AfterSave(Action),
    Save {
        draft: Draft,
        path: PathBuf,
        action: Action,
    },
    Playback(Work),
}
impl Desktop {
    pub(super) fn request_action(self: &Rc<Self>, action: Action) {
        if self.after.borrow().is_some() || self.dialog_active.get() {
            super::reply_native_quit(false);
            return;
        }
        let (dirty, can_save, draft) = {
            let mut app = self.app.borrow_mut();
            let rate = app.rate;
            app.workbench.commit_pending(rate);
            (
                app.workbench.dirty(),
                app.workbench.can_save(),
                app.workbench.draft.clone(),
            )
        };
        if !dirty {
            self.after_cleanup(Continuation::Action(action));
            self.refresh();
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            super::reply_native_quit(false);
            return;
        };
        self.dialog_active.set(true);
        let d = Rc::clone(self);
        if let Err(error) = slint::spawn_local(async move {
            let answer = rfd::AsyncMessageDialog::new()
                .set_parent(&ui.window().window_handle())
                .set_title("Unsaved sound")
                .set_description("Save changes to this sound before continuing?")
                .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                    "Save".into(),
                    "Discard".into(),
                    "Cancel".into(),
                ))
                .show()
                .await;
            let save = answer == rfd::MessageDialogResult::Yes
                || answer == rfd::MessageDialogResult::Custom("Save".into());
            let discard = answer == rfd::MessageDialogResult::No
                || answer == rfd::MessageDialogResult::Custom("Discard".into());
            let next = if save {
                if !can_save {
                    d.app.borrow_mut().status =
                        "Correct the numeric fields before saving. The sound remains open.".into();
                    None
                } else {
                    rfd::AsyncFileDialog::new()
                        .set_parent(&ui.window().window_handle())
                        .set_file_name("sound.toml")
                        .add_filter("TOML recipe", &["toml"])
                        .save_file()
                        .await
                        .map(|file| Continuation::Save {
                            draft,
                            path: file.path().to_owned(),
                            action,
                        })
                }
            } else if discard {
                Some(Continuation::Action(action))
            } else {
                None
            };
            d.dialog_active.set(false);
            if let Some(next) = next {
                d.after_cleanup(next);
            } else {
                super::reply_native_quit(false);
            }
            d.refresh();
        }) {
            self.dialog_active.set(false);
            super::reply_native_quit(false);
            self.app.borrow_mut().status = error.to_string();
        }
        self.refresh();
    }
    fn after_cleanup(self: &Rc<Self>, next: Continuation) {
        let busy = self.worker.borrow().is_some();
        if busy {
            if let Some(worker) = self.worker.borrow().as_ref() {
                worker.cancel.request();
            }
            *self.after.borrow_mut() = Some(next);
        } else {
            self.continue_with(next);
        }
    }
    pub(super) fn continue_with(self: &Rc<Self>, next: Continuation) {
        match next {
            Continuation::Save {
                draft,
                path,
                action,
            } => {
                *self.after.borrow_mut() = Some(Continuation::AfterSave(action));
                self.start(Operation::Save, Work::Save(draft, path));
            }
            Continuation::AfterSave(action) => {
                if self.app.borrow().workbench.dirty() {
                    self.app.borrow_mut().status =
                        "Saved snapshot; newer edits remain unsaved. The sound remains open."
                            .into();
                    super::reply_native_quit(false);
                } else {
                    self.continue_with(Continuation::Action(action));
                }
            }
            Continuation::Action(Action::Close) => {
                if super::reply_native_quit(true) {
                    return;
                }
                if let Some(ui) = self.ui.upgrade() {
                    let _ = slint::ComponentHandle::hide(&ui);
                }
                let _ = slint::quit_event_loop();
            }
            Continuation::Action(Action::New) => {
                self.reset_editors();
                let mut app = self.app.borrow_mut();
                app.workbench = app::Workbench::new(Draft::new(), app.rate);
                app.editing = true;
                app.status.clear();
                drop(app);
                if let Some(ui) = self.ui.upgrade() {
                    ui.set_tab_index(1);
                }
            }
            Continuation::Action(Action::Copy(preset)) => {
                self.reset_editors();
                self.app.borrow_mut().open_workbench(&preset);
                if self.app.borrow().editing
                    && let Some(ui) = self.ui.upgrade()
                {
                    ui.set_tab_index(1);
                }
            }
            Continuation::Playback(work) => {
                let operation = if matches!(work, Work::Live(_)) {
                    Operation::Live
                } else {
                    Operation::Play
                };
                self.start(operation, work);
            }
        }
    }
    pub(super) fn replace_playback(self: &Rc<Self>, work: Work) {
        if self.after.borrow().is_some() || self.dialog_active.get() {
            return;
        }
        {
            let app = self.app.borrow();
            if app.operation.is_some()
                && !matches!(app.operation, Some(Operation::Play | Operation::Live))
            {
                return;
            }
            if let Work::Live(draft) = &work
                && (!app.workbench.parse_errors.is_empty()
                    || draft.target(app.workbench.rate(app.rate)).is_err())
            {
                return;
            }
        }
        self.after_cleanup(Continuation::Playback(work));
        self.refresh();
    }
}
