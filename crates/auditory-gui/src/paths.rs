// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Native choices and editable text have explicitly different authority.
use std::path::{Component, Path, PathBuf};
#[derive(Clone, Debug, PartialEq, Eq)]
enum Authority {
    ChosenNative(PathBuf),
    ManualDraft(String),
}
#[derive(Clone, Debug)]
pub(crate) struct PathForm {
    authority: Authority,
    restore: Option<Authority>,
}
impl Default for PathForm {
    fn default() -> Self {
        Self {
            authority: Authority::ManualDraft(String::new()),
            restore: None,
        }
    }
}
impl PathForm {
    pub(crate) fn choose(&mut self, path: Option<PathBuf>) {
        if let Some(path) = path {
            self.authority = Authority::ChosenNative(path);
            self.restore = None;
        }
    }
    pub(crate) fn begin_edit(&mut self) {
        if self.restore.is_none() {
            self.restore = Some(self.authority.clone());
            let text = match &self.authority {
                Authority::ChosenNative(path) => path.to_str().unwrap_or("").to_owned(),
                Authority::ManualDraft(text) => text.clone(),
            };
            self.authority = Authority::ManualDraft(text);
        }
    }
    pub(crate) fn edit(&mut self, text: String) {
        self.begin_edit();
        self.authority = Authority::ManualDraft(text);
    }
    pub(crate) fn cancel_edit(&mut self) {
        if let Some(previous) = self.restore.take() {
            self.authority = previous;
        }
    }
    pub(crate) fn editing(&self) -> bool {
        matches!(self.authority, Authority::ManualDraft(_))
    }
    pub(crate) fn display(&self) -> String {
        match &self.authority {
            Authority::ChosenNative(path) => format!("{path:?}"),
            Authority::ManualDraft(text) => text.clone(),
        }
    }
    pub(crate) fn path(&self) -> Result<PathBuf, String> {
        match &self.authority {
            Authority::ChosenNative(path) => Ok(path.clone()),
            Authority::ManualDraft(text) if !text.is_empty() => Ok(PathBuf::from(text)),
            _ => Err("Enter a path; an empty draft never uses the previous native choice.".into()),
        }
    }
}
pub(crate) fn export_destination(folder: &PathForm, filename: &str) -> Result<PathBuf, String> {
    let path = Path::new(filename);
    let mut components = path.components();
    if filename.is_empty()
        || filename.contains(std::path::is_separator)
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Use one new filename, without separators, an absolute path, . or ...".into());
    }
    let folder = folder.path()?;
    Ok(folder.join(path))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chosen_edit_no_selection_and_cancel_keep_explicit_authority() {
        let mut form = PathForm::default();
        form.choose(Some("two  spaces".into()));
        assert_eq!(form.display(), "\"two  spaces\"");
        form.choose(None);
        assert_eq!(form.path().unwrap(), Path::new("two  spaces"));
        form.begin_edit();
        assert_eq!(form.display(), "two  spaces");
        form.edit(String::new());
        assert!(form.path().is_err());
        form.cancel_edit();
        assert_eq!(form.path().unwrap(), Path::new("two  spaces"));
        form.edit("different".into());
        assert_eq!(form.path().unwrap(), Path::new("different"));
        form.choose(None);
        assert_eq!(form.path().unwrap(), Path::new("different"));
    }
    #[cfg(unix)]
    #[test]
    fn non_unicode_choices_never_round_trip_through_display() {
        use std::os::unix::ffi::OsStringExt;
        let native = PathBuf::from(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
        for _role in ["input", "export folder"] {
            let mut form = PathForm::default();
            form.choose(Some(native.clone()));
            assert_eq!(form.path().unwrap(), native);
            form.begin_edit();
            assert_eq!(form.display(), "");
            assert!(form.path().is_err());
            form.cancel_edit();
            assert_eq!(form.path().unwrap(), native);
            form.edit("typed replacement".into());
            assert_eq!(form.path().unwrap(), Path::new("typed replacement"));
        }
    }
    #[test]
    fn filenames_are_syntax_not_an_overwrite_policy() {
        let dir = tempfile::tempdir().unwrap();
        let mut folder = PathForm::default();
        folder.choose(Some(dir.path().into()));
        for bad in ["", ".", "..", "../x", "x/y", "/absolute"] {
            assert!(export_destination(&folder, bad).is_err(), "{bad}");
        }
        let destination = export_destination(&folder, "existing.wav").unwrap();
        std::fs::write(&destination, b"owned by someone else").unwrap();
        assert_eq!(
            export_destination(&folder, "existing.wav").unwrap(),
            destination
        );
        let absent = dir.path().join("not-created");
        folder.choose(Some(absent.clone()));
        assert_eq!(
            export_destination(&folder, "new.wav").unwrap(),
            absent.join("new.wav")
        );
    }
}
