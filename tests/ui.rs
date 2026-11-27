//! UI tests: the real app on Slint's testing backend (no window), driven through the same
//! accessibility labels and actions TalkBack uses, with a temporary data directory.

use std::path::Path;
use std::time::Duration;

use flausch::{App, ChangeItem, DialogKind, Language, ProjectSort, Screen, Store};
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

/// A button or a project card.
fn control(app: &App, label: &str) -> Option<ElementHandle> {
    find(app, AccessibleRole::Button, label).or_else(|| find(app, AccessibleRole::ListItem, label))
}

fn exists(app: &App, label: &str) -> bool {
    control(app, label).is_some()
}

fn tap(app: &App, label: &str) {
    control(app, label)
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

/// Opens an entry of the ⋮ menu on the project list.
fn menu(app: &App, label: &str) {
    tap(app, "More options");
    tap(app, label);
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
fn a_new_project_starts_with_status_parts_and_notes() {
    let (app, _dir) = fresh();
    // Tall enough for the whole form: elements scrolled out of view can't be found.
    app.window()
        .set_size(slint::LogicalSize::new(412.0, 1600.0));
    tap(&app, "New project");
    type_into(&app, "Name", "Scarf");
    tap(&app, "Planned");
    type_into(&app, "Part", "Left half");
    type_into(&app, "Rows", "120");
    tap(&app, "Add part");
    type_into(&app, "Part", "Fringe");
    type_into(&app, "How many", "12");
    type_into(&app, "Notes", "Blue yarn");
    tap(&app, "Create");
    assert_eq!(store(&app).get_screen(), Screen::Project);
    let current = store(&app).get_current();
    assert_eq!(current.status, flausch::ProjectStatus::Planned);
    assert_eq!(current.notes, "Blue yarn");
    let parts: Vec<_> = store(&app).get_parts().iter().collect();
    assert_eq!(parts.len(), 2, "a part typed in but not added counts, too");
    assert_eq!(parts[0].goal, 120);
    assert_eq!(parts[1].quantity, 12);
    assert_eq!(current.active.name, "Left half");
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
fn projects_are_organized_in_folders() {
    let (app, dir) = fresh();
    create_project(&app, "Scarf");
    go_back(&app);
    assert!(
        !exists(&app, "Move to folder"),
        "no folder row without folders"
    );

    // A new folder opens right away, and projects created in it stay there.
    tap(&app, "New folder");
    type_into(&app, "Name", "Toys");
    tap(&app, "Create");
    assert_eq!(store(&app).get_folder().name, "Toys");
    assert!(!exists(&app, "Scarf"));
    create_project(&app, "Bunny");
    go_back(&app);
    assert!(exists(&app, "Bunny"));
    go_back(&app);
    assert!(exists(&app, "Folder Toys, 1 project"));
    assert!(exists(&app, "Scarf") && !exists(&app, "Bunny"));

    // Moving a project from its screen.
    tap(&app, "Scarf");
    tap(&app, "Move to folder");
    tap(&app, "Toys");
    assert!(exists(&app, "In folder Toys"));
    go_back(&app);
    assert!(exists(&app, "Folder Toys, 2 projects"));
    assert!(!exists(&app, "Scarf"));

    // The search looks into every folder.
    tap(&app, "Search");
    type_into(&app, "Search", "scarf");
    assert!(exists(&app, "Scarf"));
    assert!(!exists(&app, "Folder Toys, 2 projects"));
    tap(&app, "Close search");

    let app = launch(dir.path());
    settle();
    tap(&app, "Folder Toys, 2 projects");
    tap(&app, "Rename folder");
    type_into(&app, "Name", "Amigurumi");
    tap(&app, "Save");
    assert_eq!(store(&app).get_folder().name, "Amigurumi");

    // Deleting the folder keeps its projects.
    tap(&app, "Rename folder");
    tap(&app, "Delete folder");
    assert_eq!(store(&app).get_folder().id, 0);
    assert!(exists(&app, "Scarf") && exists(&app, "Bunny"));
    assert!(!exists(&app, "Move to folder"));
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
fn the_list_can_be_sorted_by_name() {
    let (app, dir) = fresh();
    create_project(&app, "bunny");
    go_back(&app);
    create_project(&app, "Hat");
    go_back(&app);
    let names = |app: &App| -> Vec<String> {
        store(app)
            .get_projects()
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    };
    assert_eq!(names(&app), ["Hat", "bunny"]);

    tap(&app, "Sort");
    assert_eq!(store(&app).get_dialog(), DialogKind::Sort);
    tap(&app, "Name");
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    assert_eq!(names(&app), ["bunny", "Hat"]);

    let app = launch(dir.path());
    settle();
    assert_eq!(names(&app), ["bunny", "Hat"], "the order is kept");
}

#[test]
fn projects_can_be_moved_by_hand() {
    use slint::platform::PointerEventButton;
    use slint::{LogicalPosition, LogicalSize};

    let (app, dir) = fresh();
    app.window().set_size(LogicalSize::new(412.0, 924.0));
    for name in ["A", "B", "C"] {
        create_project(&app, name);
        go_back(&app);
    }
    let names = |app: &App| -> Vec<String> {
        store(app)
            .get_projects()
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    };
    let card = |app: &App, name: &str| find(app, AccessibleRole::ListItem, name).unwrap();
    assert_eq!(names(&app), ["C", "B", "A"]);

    // TalkBack's "Move up" action.
    card(&app, "A").invoke_accessible_increment_action();
    settle();
    assert_eq!(names(&app), ["C", "A", "B"]);
    assert_eq!(store(&app).get_project_sort(), ProjectSort::Custom);

    // Hold C, then drag it down past A.
    let c = card(&app, "C");
    let start = c.absolute_position();
    let size = c.size();
    let mut position =
        LogicalPosition::new(start.x + size.width / 3.0, start.y + size.height / 2.0);
    let window = app.window();
    let button = PointerEventButton::Left;
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed { position, button });
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(700));
    for _ in 0..12 {
        position.y += 10.0;
        window.dispatch_event(WindowEvent::PointerMoved { position });
        settle();
    }
    window.dispatch_event(WindowEvent::PointerReleased { position, button });
    settle();
    settle();
    assert_eq!(names(&app), ["A", "C", "B"]);
    assert_eq!(store(&app).get_screen(), Screen::List, "a drag is no tap");

    let app = launch(dir.path());
    settle();
    assert_eq!(names(&app), ["A", "C", "B"], "the order is kept");
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
    menu(&app, "Settings");
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
    menu(&app, "About");
    assert_eq!(store(&app).get_screen(), Screen::Info);
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
    assert_eq!(store(&app).get_screen(), Screen::Info);
}

#[test]
fn the_website_opens_in_the_browser() {
    let (app, _dir) = fresh();
    menu(&app, "About");
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = opened.clone();
    store(&app).on_open_url(move |url| log.borrow_mut().push(url.to_string()));
    tap(&app, "Website");
    assert_eq!(*opened.borrow(), ["https://ldrick.github.io/flausch/"]);
    assert_eq!(store(&app).get_screen(), Screen::Info);
}

#[test]
fn about_pages_show_the_app_and_the_changelog() {
    let (app, _dir) = fresh();
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    app.window().set_size(slint::LogicalSize::new(412., 924.));
    menu(&app, "About");
    tap(&app, "About Flausch");
    assert_eq!(store(&app).get_screen(), Screen::About);
    let description = "A crochet companion. Flausch is German for fluff and sounds like “fl”, then “ow” as in “now”, then “sh”.";
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
    assert_eq!(store(&app).get_screen(), Screen::Info);

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
    assert_eq!(store(&app).get_screen(), Screen::Info);
    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::List);
}

#[test]
fn the_abbreviation_table_translates_and_searches() {
    let (app, _dir) = fresh();
    menu(&app, "Abbreviations");
    assert_eq!(store(&app).get_screen(), Screen::Abbreviations);
    let chain = "Luftmasche (Lm). US: ch, chain. UK: ch, chain";
    assert!(find(&app, AccessibleRole::Text, chain).is_some());

    // "dc" is a German Stäbchen in a US pattern, but a feste Masche in a UK one.
    type_into(&app, "Search", "dc");
    let rows = |app: &App| {
        store(app)
            .get_abbreviations()
            .iter()
            .map(|t| t.de_name.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&app)[..2], ["feste Masche", "Stäbchen"]);
    assert!(find(&app, AccessibleRole::Text, chain).is_none());
    assert!(
        find(
            &app,
            AccessibleRole::Text,
            "feste Masche (fM, fe M). US: sc, single crochet. UK: dc, double crochet"
        )
        .is_some()
    );

    type_into(&app, "Search", "xyz");
    assert_eq!(rows(&app), Vec::<String>::new());
    assert!(find(&app, AccessibleRole::Text, "No matching abbreviations").is_some());

    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::List);
    menu(&app, "Abbreviations");
    assert_eq!(rows(&app).len(), 34, "the search starts over");
}

#[test]
fn the_settings_hold_only_settings() {
    let (app, _dir) = fresh();
    // Everything fits on a phone screen (Pixel 9a), without scrolling.
    app.window().set_size(slint::LogicalSize::new(412., 924.));
    menu(&app, "Settings");
    for label in [
        "Deutsch",
        "Keep screen on",
        "Count with volume keys",
        "Green",
        "Backup",
    ] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "{label} is there"
        );
    }
    for label in ["Abbreviations", "About Flausch", "Donate", "Website"] {
        assert!(!exists(&app, label), "{label} is in the menu now");
    }

    // On by default.
    find(&app, AccessibleRole::Switch, "Keep screen on")
        .unwrap()
        .invoke_accessible_default_action();
    settle();
    assert!(!store(&app).get_keep_screen_on());
    tap(&app, "Backup");
    assert_eq!(store(&app).get_screen(), Screen::Backup);
    assert!(exists(&app, "Export") && exists(&app, "Import"));
    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::Settings);
}

#[test]
fn the_yarn_stash_keeps_yarns() {
    let (app, dir) = fresh();
    // The menu closes with Back, before anything else.
    tap(&app, "More options");
    assert!(store(&app).get_menu_open());
    assert!(store(&app).invoke_back());
    assert!(!store(&app).get_menu_open());
    assert_eq!(store(&app).get_screen(), Screen::List);

    menu(&app, "Yarn stash");
    assert_eq!(store(&app).get_screen(), Screen::Stash);
    assert!(!store(&app).get_menu_open());
    tap(&app, "Add yarn");
    type_into(&app, "Brand", "Schachenmayr");
    type_into(&app, "Yarn", "Catania");
    type_into(&app, "Color", "Rosa 0222");
    type_into(&app, "Dye lot", "4711");
    type_into(&app, "Fiber", "100% cotton");
    type_into(&app, "Grams per skein", "50");
    type_into(&app, "Meters per skein", "125");
    type_into(&app, "Skeins", "3");
    tap(&app, "Add");
    // Without a number of skeins, it is one.
    tap(&app, "Add yarn");
    type_into(&app, "Color", "Leftover green");
    tap(&app, "Add");
    assert_eq!(store(&app).get_yarn_skeins(), 4);
    assert_eq!(store(&app).get_yarn_grams(), 150);

    // Kept after a restart, and edited from its card.
    let app = launch(dir.path());
    store(&app).invoke_set_language(Language::En);
    settle();
    menu(&app, "Yarn stash");
    let catania = "Schachenmayr Catania · Rosa 0222 · Lot 4711, 3 skeins";
    tap(&app, catania);
    assert_eq!(store(&app).get_dialog(), DialogKind::EditYarn);
    type_into(&app, "Skeins", "2");
    tap(&app, "Save");
    assert!(exists(
        &app,
        "Schachenmayr Catania · Rosa 0222 · Lot 4711, 2 skeins"
    ));

    // Deleting asks first, right in the form.
    tap(&app, "Leftover green, 1 skein");
    tap(&app, "Delete yarn");
    tap(&app, "Keep");
    assert_eq!(store(&app).get_yarns().row_count(), 2);
    tap(&app, "Delete yarn");
    tap(&app, "Delete");
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    assert_eq!(store(&app).get_yarns().row_count(), 1);
    assert_eq!(store(&app).get_yarn_skeins(), 2);

    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::List);
}

#[test]
fn crochet_hooks_get_their_us_size() {
    let (app, dir) = fresh();
    menu(&app, "Crochet hooks");
    assert_eq!(store(&app).get_screen(), Screen::Hooks);
    tap(&app, "Add hook");
    // A comma works, too, and the form shows the US size while typing.
    type_into(&app, "Size in mm", "3,5");
    assert!(find(&app, AccessibleRole::Text, "US E-4").is_some());
    type_into(&app, "Brand", "Clover");
    type_into(&app, "Type", "Amour");
    tap(&app, "Add");

    tap(&app, "Add hook");
    type_into(&app, "Size in mm", "0");
    assert!(find(&app, AccessibleRole::Text, "Not a hook size").is_some());
    tap(&app, "Add");
    assert_eq!(store(&app).get_dialog(), DialogKind::AddHook, "not taken");
    type_into(&app, "Size in mm", "3");
    assert!(find(&app, AccessibleRole::Text, "No US size").is_some());
    tap(&app, "Add");
    tap(&app, "Add hook");
    type_into(&app, "Size in mm", "2");
    tap(&app, "Add");

    let labels: Vec<_> = store(&app)
        .get_hooks()
        .iter()
        .map(|h| h.form.size.to_string())
        .collect();
    assert_eq!(labels, ["2", "3", "3.5"], "from the smallest");
    assert!(exists(&app, "2 mm, US 4 (steel)"));
    assert!(exists(&app, "3 mm"));

    // Kept after a restart, in German with a comma, and edited from its card.
    let app = launch(dir.path());
    store(&app).invoke_set_language(Language::De);
    settle();
    store(&app).invoke_open_page(Screen::Hooks);
    settle();
    tap(&app, "3,5 mm, US E-4, Clover Amour");
    assert_eq!(store(&app).get_dialog(), DialogKind::EditHook);
    store(&app).invoke_set_language(Language::En);
    settle();
    type_into(&app, "Size in mm", "4");
    tap(&app, "Save");
    assert!(exists(&app, "4 mm, US G-6, Clover Amour"));

    // Deleting asks first, right in the form.
    tap(&app, "3 mm");
    tap(&app, "Delete hook");
    tap(&app, "Keep");
    assert_eq!(store(&app).get_hooks().row_count(), 3);
    tap(&app, "Delete hook");
    tap(&app, "Delete");
    assert_eq!(store(&app).get_dialog(), DialogKind::None);
    assert_eq!(store(&app).get_hooks().row_count(), 2);

    go_back(&app);
    assert_eq!(store(&app).get_screen(), Screen::List);
}
