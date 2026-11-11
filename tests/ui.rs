//! UI tests: the real app on Slint's testing backend (no window), driven through the same
//! accessibility labels and actions TalkBack uses, with a temporary data directory.

use std::path::Path;
use std::time::Duration;

use flausch::{App, DialogKind, Language, Screen, Store};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model};

/// Starts the app on `dir` as if it was launched again.
fn launch(dir: &Path) -> App {
    flausch::stop();
    let app = App::new().unwrap();
    flausch::start(&app, dir.to_path_buf()).unwrap();
    app
}

/// Starts the app on an empty data directory, in English.
fn fresh() -> (App, tempfile::TempDir) {
    i_slint_backend_testing::init_no_event_loop();
    let dir = tempfile::tempdir().unwrap();
    let app = launch(dir.path());
    app.global::<Store>().invoke_set_language(Language::En);
    settle();
    (app, dir)
}

/// Runs pending change handlers and instantiates elements that appeared.
fn settle() {
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(1));
}

fn find(app: &App, role: AccessibleRole, label: &str) -> Option<ElementHandle> {
    ElementHandle::find_by_accessible_label(app, label).find(|e| e.accessible_role() == Some(role))
}

fn exists(app: &App, label: &str) -> bool {
    find(app, AccessibleRole::Button, label).is_some()
}

fn tap(app: &App, label: &str) {
    find(app, AccessibleRole::Button, label)
        .unwrap_or_else(|| panic!("no button {label:?}"))
        .invoke_accessible_default_action();
    settle();
}

fn type_into(app: &App, label: &str, text: &str) {
    find(app, AccessibleRole::TextInput, label)
        .unwrap_or_else(|| panic!("no field {label:?}"))
        .set_accessible_value(text);
    settle();
}

/// Taps Back in the top bar. Waits first, because the app drops a second Back within 300 ms
/// (Android delivers one press twice).
fn go_back(app: &App) {
    std::thread::sleep(Duration::from_millis(310));
    tap(app, "Back");
}

fn store(app: &App) -> Store<'_> {
    app.global::<Store>()
}

fn create_project(app: &App, name: &str) {
    tap(app, "New project");
    type_into(app, "Name", name);
    tap(app, "Create");
}

fn add_part(app: &App, name: &str, rows: &str) {
    tap(app, "Add part");
    type_into(app, "Name", name);
    type_into(app, "Rows (optional)", rows);
    tap(app, "Add");
}

#[test]
fn counting_rows_survives_a_restart() {
    let (app, dir) = fresh();
    create_project(&app, "Bunny");
    assert_eq!(store(&app).get_screen(), Screen::Project);
    add_part(&app, "Head", "12");
    assert_eq!(
        store(&app).get_screen(),
        Screen::Project,
        "adding a part stays"
    );

    tap(&app, "Head, row 0 of 12");
    assert_eq!(store(&app).get_screen(), Screen::Part);
    tap(&app, "Next row");
    tap(&app, "Next row");
    tap(&app, "Next row");
    tap(&app, "Previous row");
    assert_eq!(store(&app).get_part().row, 2);

    let app = launch(dir.path());
    settle();
    tap(&app, "Bunny");
    assert!(exists(&app, "Head, row 2 of 12"));
}

#[test]
fn status_notes_and_a_second_counter() {
    let (app, dir) = fresh();
    create_project(&app, "Bunny");
    tap(&app, "Paused");
    assert_eq!(
        store(&app).get_current().status,
        flausch::ProjectStatus::Paused
    );

    tap(&app, "Add notes");
    assert_eq!(store(&app).get_dialog(), DialogKind::Notes);
    type_into(&app, "Notes", "4 mm hook");
    tap(&app, "Save");
    assert!(exists(&app, "Edit notes"));

    tap(&app, "Add part");
    type_into(&app, "Name", "Head");
    type_into(&app, "Second counter (optional)", "Repeats");
    tap(&app, "Add");
    tap(&app, "Head, row 0");
    tap(&app, "Repeats plus one");
    tap(&app, "Repeats plus one");
    tap(&app, "Repeats minus one");
    assert_eq!(store(&app).get_part().counter, 1);
    tap(&app, "Reset Repeats");
    assert_eq!(store(&app).get_part().counter, 0);

    let app = launch(dir.path());
    settle();
    tap(&app, "Bunny (Paused)");
    assert_eq!(store(&app).get_current().notes, "4 mm hook");
}

#[test]
fn resetting_and_deleting_ask_first() {
    let (app, _dir) = fresh();
    create_project(&app, "Bunny");
    add_part(&app, "Head", "");
    tap(&app, "Head, row 0");
    tap(&app, "Next row");

    tap(&app, "Reset counter");
    tap(&app, "Cancel");
    assert_eq!(store(&app).get_part().row, 1);
    tap(&app, "Reset counter");
    tap(&app, "Reset");
    assert_eq!(store(&app).get_part().row, 0);

    tap(&app, "Delete part");
    tap(&app, "Delete");
    assert_eq!(store(&app).get_screen(), Screen::Project);
    assert_eq!(store(&app).get_parts().row_count(), 0);

    tap(&app, "Delete project");
    tap(&app, "Delete");
    assert_eq!(store(&app).get_screen(), Screen::List);
    assert!(!store(&app).get_has_projects());
}

#[test]
fn search_finds_projects_by_part_name() {
    let (app, _dir) = fresh();
    create_project(&app, "Bunny");
    add_part(&app, "Ears", "");
    go_back(&app);
    create_project(&app, "Scarf");
    go_back(&app);
    assert!(exists(&app, "Bunny") && exists(&app, "Scarf"));

    tap(&app, "Search");
    type_into(&app, "Search", "ears");
    assert!(exists(&app, "Bunny"));
    assert!(!exists(&app, "Scarf"));

    tap(&app, "Close search");
    assert!(exists(&app, "Scarf"));
}

#[test]
fn escape_closes_the_dialog_first() {
    let (app, _dir) = fresh();
    create_project(&app, "Bunny");
    tap(&app, "Rename project");
    assert_eq!(store(&app).get_dialog(), DialogKind::Edit);

    let press = |key: Key| {
        let window = app.window();
        window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        settle();
    };
    press(Key::Escape);
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    assert_eq!(store(&app).get_screen(), Screen::Project);

    std::thread::sleep(Duration::from_millis(310));
    press(Key::Escape);
    assert_eq!(store(&app).get_screen(), Screen::List);
}

#[test]
fn the_language_setting_is_applied_and_kept() {
    let (app, dir) = fresh();
    tap(&app, "Settings");
    tap(&app, "Deutsch");
    assert!(exists(&app, "Zurück"));

    let app = launch(dir.path());
    settle();
    assert!(exists(&app, "Neues Projekt"));
}
