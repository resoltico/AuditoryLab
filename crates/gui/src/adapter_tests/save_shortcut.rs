//! The generated File menu owns Save's primary-modifier binding; focused Edit keys stay local.
use super::*;
fn primary_shortcut(h: &Harness, key: &str) {
    let window = h.ui().window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}
#[test]
fn save_menu_shortcut_captures_focused_title_once_and_preserves_edit_keys() {
    assert_focused_save("Sound title", "Shortcut title");
}
#[test]
fn save_menu_shortcut_captures_focused_notes_once_and_preserves_edit_keys() {
    assert_focused_save("Custom recipe notes", "Shortcut notes\nsecond line");
}
fn assert_focused_save(label: &str, text: &str) {
    let h = Harness::new();
    h.workbench();
    let input = h.field(label);
    WindowInner::from_pub(h.ui().window()).set_focus_item(&input, true, FocusReason::Programmatic);
    primary_shortcut(&h, "a");
    h.key(text);
    assert_eq!(text_of(&input), text);
    primary_shortcut(&h, "s");
    let jobs = h.adapter.take_jobs();
    assert_eq!(jobs.len(), 1, "one physical shortcut admits one save");
    assert!(h.adapter.inspect(|w| w.file_busy()));
    assert_eq!(
        text_of(&input),
        text,
        "the save shortcut inserts no s into the field"
    );
    primary_shortcut(&h, "s");
    assert_eq!(
        h.adapter.take_jobs().len(),
        0,
        "a busy save stays unavailable"
    );
    let job = jobs.into_iter().next().unwrap();
    h.key(" later");
    finish_file(&h, job);
    h.wait_for_files();
    let entries = h
        .store()
        .scan(&auditorylab_app::job::Cancel::default())
        .entries;
    assert_eq!(entries.len(), 1);
    let saved = h.store().load(&entries[0].key).unwrap();
    let description = saved.snapshot.recipe().description().unwrap();
    if label == "Sound title" {
        assert_eq!(description.title, text);
    } else {
        assert_eq!(description.notes.as_deref(), Some(text));
    }
    assert!(
        h.adapter.inspect(|w| w.editor().dirty()),
        "later text remains separate from captured publication"
    );
    assert_eq!(text_of(&input), format!("{text} later"));
}
#[test]
fn save_menu_shortcut_obeys_invalid_staged_input_without_consuming_text() {
    let h = Harness::new();
    h.workbench();
    h.ui().invoke_add_component(h.ui().get_document(), 0);
    h.settle();
    let row = h.ui().get_components().row_data(0).unwrap();
    let frequency = row
        .parameters
        .iter()
        .find(|p| field_parameter(&p.key) == ParameterId::ToneFrequency)
        .unwrap();
    h.ui()
        .invoke_stage_parameter(frequency.key.clone(), "1e309".into());
    h.settle();
    let input = find_control(h.ui(), |item| {
        item.downcast::<TextInput>().is_some() && text_of(item) == "1e309"
    })
    .unwrap();
    WindowInner::from_pub(h.ui().window()).set_focus_item(&input, true, FocusReason::Programmatic);
    primary_shortcut(&h, "s");
    assert_eq!(
        text_of(&input),
        "1e309",
        "disabled Save does not type s into invalid input"
    );
    assert_eq!(h.adapter.take_jobs().len(), 0);
    assert!(!h.adapter.inspect(|w| w.file_busy()));
    assert!(h.adapter.inspect(|w| w.editor().has_staged()));
    assert_eq!(
        h.store()
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .len(),
        0
    );
}

#[test]
fn save_menu_shortcut_stays_disabled_for_an_uninitialized_managed_destination() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("missing library");
    let store = auditorylab_app::services::library::Library::at_path(root.clone()).unwrap();
    let workspace = Workspace::with_library(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
        store,
    )
    .unwrap();
    let h = Harness::with_workspace(workspace);
    h.workbench();
    let input = h.field("Sound title");
    WindowInner::from_pub(h.ui().window()).set_focus_item(&input, true, FocusReason::Programmatic);
    primary_shortcut(&h, "a");
    h.key("Unpublished shortcut title");
    primary_shortcut(&h, "s");
    assert_eq!(h.adapter.take_jobs().len(), 0);
    assert_eq!(text_of(&input), "Unpublished shortcut title");
    assert!(!root.exists());
    assert!(h.adapter.inspect(|w| w.editor().has_staged()));
}
