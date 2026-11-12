//! UI tests: the real app on Slint's testing backend (no window), driven through the same
//! accessibility labels and actions TalkBack uses, with a temporary data directory.

use std::path::Path;
use std::time::Duration;

use flausch::{App, ChangeItem, DialogKind, Language, Screen, Store};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

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
fn status_and_notes() {
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

    let app = launch(dir.path());
    settle();
    tap(&app, "Bunny (Paused)");
    assert_eq!(store(&app).get_current().notes, "4 mm hook");
}

#[test]
fn a_part_made_several_times() {
    let (app, _dir) = fresh();
    create_project(&app, "Bunny");
    tap(&app, "Add part");
    type_into(&app, "Name", "Leg");
    type_into(&app, "Rows (optional)", "2");
    type_into(&app, "How many (optional)", "4");
    tap(&app, "Add");
    tap(&app, "Leg 1/4, row 0 of 2");
    tap(&app, "Next row");
    assert!(!exists(&app, "Next piece (2/4)"));
    tap(&app, "Next row");
    tap(&app, "Next piece (2/4)");
    assert_eq!(store(&app).get_part().title, "Leg 2/4");
    assert_eq!(store(&app).get_current().rows_done, 2);
    tap(&app, "Previous row");
    assert_eq!(
        (store(&app).get_part().piece, store(&app).get_part().row),
        (1, 2)
    );
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
fn a_part_is_edited_and_deleted_from_the_project() {
    let (app, _dir) = fresh();
    create_project(&app, "Bunny");
    add_part(&app, "Head", "");
    add_part(&app, "Leg", "");

    tap(&app, "Edit Head");
    assert_eq!(store(&app).get_screen(), Screen::Project);
    type_into(&app, "Rows (optional)", "8");
    tap(&app, "Save");
    assert!(exists(&app, "Head, row 0 of 8"));
    assert_eq!(store(&app).get_part().name, "Leg", "still counting the leg");

    tap(&app, "Edit Head");
    tap(&app, "Delete part");
    tap(&app, "Delete");
    assert_eq!(store(&app).get_screen(), Screen::Project);
    assert!(!exists(&app, "Head, row 0 of 8"));
    assert!(exists(&app, "Leg, row 0"));
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
fn the_list_filters_and_groups_by_status() {
    let (app, dir) = fresh();
    create_project(&app, "Bunny");
    tap(&app, "Finished");
    go_back(&app);
    create_project(&app, "Hat");
    tap(&app, "Planned");
    go_back(&app);
    create_project(&app, "Scarf");
    go_back(&app);

    // Finished projects wait in a collapsed section at the bottom.
    assert!(exists(&app, "Scarf") && exists(&app, "Hat (Planned)"));
    assert!(!exists(&app, "Bunny (Finished)"));
    tap(&app, "Show finished projects (1)");
    assert!(exists(&app, "Bunny (Finished)"));
    tap(&app, "Hide finished projects (1)");
    assert!(!exists(&app, "Bunny (Finished)"));

    tap(&app, "Planned");
    assert!(exists(&app, "Hat (Planned)"));
    assert!(!exists(&app, "Scarf"));
    let app = launch(dir.path());
    settle();
    assert!(exists(&app, "Hat (Planned)"));
    assert!(!exists(&app, "Scarf"), "the filter is kept");

    tap(&app, "All");
    tap(&app, "Search");
    type_into(&app, "Search", "finis");
    assert!(exists(&app, "Bunny (Finished)"));
    assert!(!exists(&app, "Scarf") && !exists(&app, "Hat (Planned)"));
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

#[test]
fn donating_opens_the_page_in_the_browser() {
    let (app, _dir) = fresh();
    app.window().set_size(slint::LogicalSize::new(412., 2000.));
    tap(&app, "Settings");
    tap(&app, "Donate");
    assert_eq!(store(&app).get_screen(), Screen::Donate);
    // Don't start a browser in the test.
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = opened.clone();
    store(&app).on_open_url(move |url| log.borrow_mut().push(url.to_string()));
    tap(&app, "GitHub Sponsors");
    tap(&app, "Liberapay");
    assert_eq!(
        *opened.borrow(),
        [
            "https://github.com/sponsors/ldrick",
            "https://liberapay.com/ldrick"
        ]
    );
    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::Settings);
}

#[test]
fn about_pages_show_the_app_and_the_changelog() {
    let (app, _dir) = fresh();
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    // The element tree only holds what is visible, and About is at the end of the settings.
    app.window().set_size(slint::LogicalSize::new(412., 2000.));
    tap(&app, "Settings");
    tap(&app, "About Flausch");
    assert_eq!(store(&app).get_screen(), Screen::About);
    let description = "A crochet companion. Flausch is German for fluff.";
    assert!(find(&app, AccessibleRole::Text, description).is_some());
    assert!(
        find(
            &app,
            AccessibleRole::Text,
            "Made with love in the Black Forest"
        )
        .is_some()
    );
    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::Settings);

    let text = "Longer text that has to wrap onto several lines on the page, ".repeat(3);
    let points = |points: &[&str]| {
        ModelRc::new(VecModel::from(
            points
                .iter()
                .map(|&p| p.into())
                .collect::<Vec<slint::SharedString>>(),
        ))
    };
    store(&app).set_changes(ModelRc::new(VecModel::from(vec![
        ChangeItem {
            version: "1.1.0".into(),
            points: points(&[&text, "Dark mode"]),
        },
        ChangeItem {
            version: "1.0.0".into(),
            points: points(&["First release"]),
        },
    ])));
    tap(&app, "Changelog");
    assert_eq!(store(&app).get_screen(), Screen::Changelog);
    let point = find(&app, AccessibleRole::Text, &text).expect("the point is listed");
    // The row must be tall enough for the wrapped text.
    assert!(point.size().height > 40.);
    for label in [
        "Version 1.1.0",
        "Dark mode",
        "Version 1.0.0",
        "First release",
    ] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "{label} is listed"
        );
    }
    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::Settings);
}
