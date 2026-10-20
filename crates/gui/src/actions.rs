//! Destructive navigation and Save/Discard/Cancel coordinated across audio and file lifetimes.
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
    Request(Action),
    AfterSave(Action, std::sync::Arc<()>),
    Save {
        draft: Draft,
        path: PathBuf,
        action: Action,
        owner: std::sync::Arc<()>,
    },
    Playback(Work),
}
impl Desktop {
    pub(super) fn request_action(self: &Rc<Self>, action: Action) {
        if self.after.borrow().is_some() || self.dialog_active.get() {
            self.app.borrow_mut().status =
                "Finish or cancel the open dialog/task before closing or replacing this sound."
                    .into();
            super::reply_native_quit(false);
            return;
        }
        if self.file_worker.borrow().is_some() {
            self.after_cleanup(Continuation::Request(action));
            self.refresh();
            return;
        }
        let (dirty, can_save, draft, owner) = {
            let mut app = self.app.borrow_mut();
            let rate = app.rate;
            app.workbench.commit_pending(rate);
            (
                app.workbench.dirty(),
                app.workbench.can_save(),
                app.workbench.draft.clone(),
                app.workbench.identity.clone(),
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
                    match super::dialogs::file_path(&ui, Some("sound.toml"), "toml").await {
                        Ok(path) => path.map(|path| Continuation::Save {
                            draft,
                            path,
                            action,
                            owner,
                        }),
                        Err(error) => {
                            d.app.borrow_mut().status = error;
                            None
                        }
                    }
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
        let stop_audio = matches!(
            &next,
            Continuation::Action(Action::Close)
                | Continuation::Request(Action::Close)
                | Continuation::Save {
                    action: Action::Close,
                    ..
                }
                | Continuation::Playback(_)
        );
        let wait_file = !matches!(&next, Continuation::Playback(_));
        if stop_audio && let Some(worker) = self.audio_worker.borrow().as_ref() {
            worker.cancel.request();
        }
        if wait_file && let Some(worker) = self.file_worker.borrow().as_ref() {
            worker.cancel.request();
        }
        let busy = stop_audio && self.audio_worker.borrow().is_some()
            || wait_file && self.file_worker.borrow().is_some();
        if busy {
            *self.after.borrow_mut() = Some(next);
        } else {
            self.continue_with(next);
        }
    }
    pub(super) fn continue_with(self: &Rc<Self>, next: Continuation) {
        match next {
            Continuation::Request(action) => self.request_action(action),
            Continuation::Save {
                draft,
                path,
                action,
                owner,
            } => {
                *self.after.borrow_mut() = Some(Continuation::AfterSave(action, owner.clone()));
                self.start(Operation::Save, Work::Save(draft, path, owner));
            }
            Continuation::AfterSave(action, owner) => {
                if !std::sync::Arc::ptr_eq(&owner, &self.app.borrow().workbench.identity)
                    || self.app.borrow().workbench.dirty()
                {
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
                self.reset_metadata();
            }
            Continuation::Action(Action::Copy(preset)) => {
                self.reset_editors();
                self.app.borrow_mut().open_workbench(&preset);
                if self.app.borrow().editing
                    && let Some(ui) = self.ui.upgrade()
                {
                    ui.set_tab_index(1);
                }
                self.reset_metadata();
            }
            Continuation::Playback(work) => {
                let operation = if matches!(work, Work::Live(..)) {
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
            if matches!(
                app.file_operation,
                Some(Operation::Export | Operation::Import)
            ) {
                return;
            }
            if let Work::Live(draft, _) = &work
                && (!app.workbench.input_valid()
                    || draft.target(app.workbench.rate(app.rate)).is_err())
            {
                return;
            }
        }
        self.after_cleanup(Continuation::Playback(work));
        self.refresh();
    }
}
