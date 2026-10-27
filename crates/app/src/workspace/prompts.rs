//! One current prompt; pending cross-state actions remain with the coordinator.
use super::*;
#[derive(Debug)]
pub(super) enum PromptState {
    Unsaved(DocumentAction),
    Purge {
        key: crate::document::origin::LibraryKey,
        expect: crate::document::origin::VersionToken,
    },
    Import,
    Export(Rendition),
    ExportRecipe(Arc<str>),
}
#[derive(Debug)]
pub(super) struct Prompt {
    pub(super) id: PromptId,
    pub(super) state: PromptState,
}
#[derive(Default)]
pub(super) struct Prompts {
    pub(super) current: Option<Prompt>,
}
impl Prompts {
    pub(super) const fn is_open(&self) -> bool {
        self.current.is_some()
    }
    pub(super) fn admit_start(&self, pending: bool) -> Result<(), Refusal> {
        if self.is_open() {
            return Err(Refusal::PromptOpen);
        }
        if pending {
            return Err(Refusal::ActionPending);
        }
        Ok(())
    }
    pub(super) fn open(&mut self, id: PromptId, state: PromptState) {
        assert!(
            self.current.is_none(),
            "an admitted prompt never replaces another prompt"
        );
        self.current = Some(Prompt { id, state });
    }
    pub(super) fn clear(&mut self) {
        self.current = None;
    }
    pub(super) fn take(&mut self, id: PromptId) -> Option<PromptState> {
        if self.current.as_ref().is_none_or(|prompt| prompt.id != id) {
            return None;
        }
        self.current.take().map(|prompt| prompt.state)
    }
}
