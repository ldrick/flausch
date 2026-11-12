//! Controller tests: the real `App` on Slint's testing backend (no window, no event loop) with a
//! temporary data directory. Each test runs on its own thread with its own controller.

use std::time::Duration;

use slint::{ComponentHandle, Model, SharedPixelBuffer};

use super::*;

const PATTERN: &[u8] = include_bytes!("../tests/fixtures/pattern.pdf");

struct Harness {
    app: App,
    dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        let dir = tempfile::tempdir().unwrap();
        let app = App::new().unwrap();
        start(&app, dir.path().to_path_buf()).unwrap();
        Self { app, dir }
    }

    fn store(&self) -> Store<'_> {
        self.app.global::<Store>()
    }

    /// The library as it is on disk.
    fn saved(&self) -> Library {
        Storage::new(self.dir.path().to_path_buf())
            .unwrap()
            .load_library()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        stop();
    }
}

fn ctl<R>(f: impl FnOnce(&mut Controller) -> R) -> R {
    with(f).expect("the controller is running")
}

/// Presses Back after the window in which a second Back counts as a duplicate.
fn back() -> bool {
    ctl(|c| {
        c.last_back = None;
        c.back()
    })
}

fn form(name: &str, rows: &str) -> PartForm {
    PartForm {
        name: name.into(),
        rows: rows.into(),
        ..PartForm::default()
    }
}

/// Creates the project "Bunny" with the part "Head" (10 rows) and returns their ids.
fn project_with_part() -> (u64, u64) {
    ctl(|c| {
        c.create_project("Bunny");
        c.add_part(&form("Head", "10"));
        c.active_part().unwrap()
    })
}

/// Imports `bytes` as the pattern of project `id`, as the file picker would, and returns the file name.
fn import(id: u64, nanos: u128, bytes: &[u8]) -> String {
    let file = Storage::new_pdf_file(id, nanos);
    ctl(|c| {
        std::fs::write(c.storage.pdf_path(&file), bytes).unwrap();
        c.pdf_picked(
            id,
            &file,
            PickResult::Picked {
                title: "pattern.pdf".into(),
            },
        );
    });
    file
}

fn pdf_path(file: &str) -> PathBuf {
    ctl(|c| c.storage.pdf_path(file))
}

fn rendered(serial: u64, page: u32, result: Result<(), &str>) -> pdf::Rendered {
    pdf::Rendered {
        serial,
        page,
        page_count: 2,
        width: 100,
        result: result
            .map(|()| SharedPixelBuffer::new(100, 141))
            .map_err(str::to_owned),
    }
}

#[test]
fn back_closes_a_dialog_before_leaving_the_screen() {
    let h = Harness::new();
    ctl(|c| c.create_project("Bunny"));
    assert_eq!(h.store().get_screen(), Screen::Project);

    h.store().set_dialog(DialogKind::Delete);
    assert!(back());
    assert_eq!(h.store().get_dialog(), DialogKind::None);
    assert_eq!(h.store().get_screen(), Screen::Project);

    assert!(back());
    assert_eq!(h.store().get_screen(), Screen::List);
    assert!(!back(), "an unhandled Back on the list closes the app");
}

#[test]
fn a_second_back_right_after_the_first_is_dropped() {
    let h = Harness::new();
    let (_, part) = project_with_part();
    ctl(|c| c.open_part(part));
    assert_eq!(h.store().get_screen(), Screen::Part);

    assert!(ctl(Controller::back));
    assert!(ctl(Controller::back));
    assert_eq!(h.store().get_screen(), Screen::Project);
}

#[test]
fn a_part_opens_the_pattern_if_the_project_has_one() {
    let h = Harness::new();
    let (id, head) = project_with_part();
    ctl(|c| c.open_part(head));
    assert_eq!(h.store().get_screen(), Screen::Part, "no PDF: the counter");
    assert!(back());

    import(id, 1, PATTERN);
    ctl(|c| c.add_part(&form("Leg", "")));
    ctl(|c| c.open_part(head));
    assert_eq!(h.store().get_screen(), Screen::Pdf);
    assert_eq!(h.store().get_part().name, "Head", "the PDF view counts it");
    assert!(back());
    assert_eq!(h.store().get_screen(), Screen::Project);
}

#[test]
fn editing_or_deleting_a_part_keeps_the_active_one() {
    let h = Harness::new();
    let (_, head) = project_with_part();
    ctl(|c| c.add_part(&form("Leg", "")));
    let leg = ctl(|c| c.active_part().unwrap().1);

    ctl(|c| c.edit_part(head, &form("Big head", "12")));
    let parts = h.store().get_parts();
    assert_eq!(
        (
            parts.row_data(0).unwrap().name.as_str(),
            parts.row_data(0).unwrap().goal
        ),
        ("Big head", 12)
    );
    assert_eq!(h.store().get_part().name, "Leg");

    ctl(|c| c.delete_part(head));
    assert_eq!(h.store().get_parts().row_count(), 1);
    assert_eq!(ctl(|c| c.active_part()).map(|(_, p)| p), Some(leg));
}

#[test]
fn back_collapses_the_counter_and_ends_the_search() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    ctl(Controller::open_pdf);
    h.store().set_counter_expanded(true);

    assert!(back());
    assert!(!h.store().get_counter_expanded());
    assert_eq!(h.store().get_screen(), Screen::Pdf);
    assert!(back());
    assert!(back());
    assert_eq!(h.store().get_screen(), Screen::List);

    h.store().set_searching(true);
    h.store().set_query("xyz".into());
    ctl(|c| c.refresh());
    assert_eq!(h.store().get_projects().row_count(), 0);
    assert!(back());
    assert!(!h.store().get_searching());
    assert_eq!(h.store().get_query(), "");
    assert_eq!(h.store().get_projects().row_count(), 1);
}

#[test]
fn rows_are_saved_right_away() {
    let h = Harness::new();
    let (id, part) = project_with_part();
    ctl(|c| {
        c.count(Library::increment);
        c.count(Library::increment);
        c.count(Library::increment);
        c.count(Library::decrement);
    });

    assert_eq!(h.store().get_part().row, 2);
    assert_eq!(h.store().get_part().goal, 10);
    let saved = h.saved();
    let saved = &saved.get(id).unwrap().parts;
    assert_eq!(
        (saved[0].id, saved[0].row, saved[0].row_goal),
        (part, 2, Some(10))
    );
}

#[test]
fn only_valid_pdfs_are_imported() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    let first = import(id, 1, PATTERN);
    assert!(h.store().get_current().has_pdf);
    assert_eq!(h.saved().get(id).unwrap().pdf.as_ref().unwrap().file, first);

    let broken = import(id, 2, b"not a pdf");
    assert_eq!(h.store().get_notice(), Notice::PdfInvalid);
    assert!(!pdf_path(&broken).exists());
    assert_eq!(h.saved().get(id).unwrap().pdf.as_ref().unwrap().file, first);

    let cancelled = Storage::new_pdf_file(id, 3);
    std::fs::write(pdf_path(&cancelled), PATTERN).unwrap();
    ctl(|c| c.pdf_picked(id, &cancelled, PickResult::Cancelled));
    assert!(!pdf_path(&cancelled).exists());

    let second = import(id, 4, PATTERN);
    assert!(!pdf_path(&first).exists(), "a replaced pattern is deleted");
    assert_eq!(
        h.saved().get(id).unwrap().pdf.as_ref().unwrap().file,
        second
    );
}

#[test]
fn the_pdf_view_is_saved_after_a_pause() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    ctl(|c| {
        c.open_pdf();
        c.pdf_view_changed(1, 2.0, 0.0, 0.5, 400.0);
    });
    assert_eq!(h.store().get_pdf().page, 1);
    assert_eq!(h.saved().get(id).unwrap().pdf.as_ref().unwrap().page, 0);

    i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
    let saved = h.saved();
    let pdf = saved.get(id).unwrap().pdf.as_ref().unwrap();
    assert_eq!((pdf.page, pdf.zoom, pdf.scroll_y), (1, 2.0, 0.5));
}

#[test]
fn the_reading_line_follows_the_page() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    ctl(|c| {
        c.open_pdf();
        c.pdf_line_shown(true, 0.3);
    });
    let view = h.store().get_pdf();
    assert!(view.line_visible && view.line_saved);
    assert!((view.line - 0.3).abs() < 1e-6);

    ctl(|c| {
        c.pdf_line_moved(0.6);
        c.pdf_view_changed(1, 1.0, 0.0, 0.0, 400.0);
    });
    let view = h.store().get_pdf();
    assert!(
        view.line_visible && !view.line_saved,
        "page 2 has no line yet"
    );

    ctl(|c| c.pdf_view_changed(0, 1.0, 0.0, 0.0, 400.0));
    assert!((h.store().get_pdf().line - 0.6).abs() < 1e-6);
}

#[test]
fn only_the_newest_render_is_shown() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    ctl(|c| {
        c.open_pdf();
        c.pdf_view_changed(0, 1.0, 0.0, 0.0, 400.0);
    });
    let serial = ctl(|c| c.pdf.serial);

    ctl(|c| c.pdf_rendered(rendered(serial - 1, 0, Ok(()))));
    assert!(h.store().get_pdf().loading, "an outdated render is dropped");

    ctl(|c| c.pdf_rendered(rendered(serial, 0, Ok(()))));
    let view = h.store().get_pdf();
    assert!(!view.loading);
    assert_eq!((view.page_count, view.serial), (2, 1));
    assert!((view.aspect - 1.41).abs() < 1e-6);
}

#[test]
fn swiping_turns_the_page() {
    use slint::LogicalPosition;
    use slint::platform::{PointerEventButton, WindowEvent};

    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    h.app.window().set_size(slint::LogicalSize::new(412., 924.));
    ctl(Controller::open_pdf);
    let serial = ctl(|c| c.pdf.serial);
    ctl(|c| c.pdf_rendered(rendered(serial, 0, Ok(()))));

    let drag = |from: (f32, f32), to: (f32, f32)| {
        let window = h.app.window();
        let at = |(x, y): (f32, f32)| LogicalPosition::new(x, y);
        let button = PointerEventButton::Left;
        window.dispatch_event(WindowEvent::PointerPressed {
            position: at(from),
            button,
        });
        for step in 1..=10 {
            let t = step as f32 / 10.;
            let position = at((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
            window.dispatch_event(WindowEvent::PointerMoved { position });
        }
        window.dispatch_event(WindowEvent::PointerReleased {
            position: at(to),
            button,
        });
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(500));
    };
    let page = || h.store().get_pdf().page;

    drag((300., 400.), (100., 410.));
    assert_eq!(page(), 1, "swipe left: next page");
    let image = || {
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&h.app, "Page 2 of 2")
            .next()
            .unwrap()
    };
    assert!(
        image().absolute_position().x < -300.,
        "the old page slid out"
    );
    let serial = ctl(|c| c.pdf.serial);
    ctl(|c| c.pdf_rendered(rendered(serial, 1, Ok(()))));
    // The first tick runs the change handler that starts the animation.
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(500));
    assert!(
        image().absolute_position().x.abs() < 1.,
        "the new page slid in"
    );
    drag((300., 400.), (100., 410.));
    assert_eq!(page(), 1, "there is no third page");
    drag((300., 400.), (280., 600.));
    assert_eq!(page(), 1, "a vertical drag keeps the page");
    drag((100., 400.), (300., 390.));
    assert_eq!(page(), 0, "swipe right: previous page");

    let mut view = h.store().get_pdf();
    view.zoom = 2.0;
    h.store().set_pdf(view);
    drag((300., 400.), (100., 410.));
    assert_eq!(page(), 0, "zoomed in, a drag moves the view instead");
}

#[test]
fn double_tapping_zooms_in_and_out() {
    use slint::LogicalPosition;
    use slint::platform::{PointerEventButton, WindowEvent};

    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    h.app.window().set_size(slint::LogicalSize::new(412., 924.));
    ctl(Controller::open_pdf);
    let serial = ctl(|c| c.pdf.serial);
    ctl(|c| c.pdf_rendered(rendered(serial, 0, Ok(()))));

    let double_tap = || {
        let window = h.app.window();
        let position = LogicalPosition::new(200., 300.);
        let button = PointerEventButton::Left;
        for _ in 0..2 {
            window.dispatch_event(WindowEvent::PointerPressed { position, button });
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(40));
            window.dispatch_event(WindowEvent::PointerReleased { position, button });
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(60));
        }
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(500));
    };
    let zoom = || ctl(|c| c.library.get(id).unwrap().pdf.as_ref().unwrap().zoom);

    double_tap();
    assert!(zoom() > 2.0, "zoomed in: {}", zoom());
    double_tap();
    assert!((zoom() - 1.0).abs() < 0.01, "zoomed out: {}", zoom());
}

#[test]
fn a_page_that_no_longer_exists_shows_the_last_one() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    import(id, 1, PATTERN);
    ctl(|c| {
        c.open_pdf();
        c.pdf_view_changed(5, 1.0, 0.0, 0.0, 400.0);
    });
    let serial = ctl(|c| c.pdf.serial);
    ctl(|c| c.pdf_rendered(rendered(serial, 5, Err("out of range"))));
    assert_eq!(h.store().get_pdf().page, 1);
    assert_eq!(h.store().get_notice(), Notice::None);

    let serial = ctl(|c| c.pdf.serial);
    ctl(|c| c.pdf_rendered(rendered(serial, 1, Err("broken page"))));
    assert_eq!(h.store().get_notice(), Notice::PdfRenderFailed);
}

#[test]
fn continue_opens_the_pattern_and_back_returns_to_the_list() {
    let h = Harness::new();
    let (with_pdf, _) = project_with_part();
    import(with_pdf, 1, PATTERN);
    let without_pdf = ctl(|c| {
        c.create_project("Scarf");
        c.current.unwrap()
    });

    ctl(|c| c.continue_project(with_pdf));
    assert_eq!(h.store().get_screen(), Screen::Pdf);
    assert!(back());
    assert_eq!(h.store().get_screen(), Screen::List);

    ctl(|c| c.continue_project(without_pdf));
    assert_eq!(h.store().get_screen(), Screen::Project);
}

#[test]
fn deleting_a_project_deletes_its_pattern() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    let file = import(id, 1, PATTERN);
    ctl(Controller::delete_project);

    assert!(!pdf_path(&file).exists());
    assert_eq!(h.store().get_screen(), Screen::List);
    assert!(!h.store().get_has_projects());
    assert_eq!(h.saved().projects.len(), 0);
}

#[test]
fn restoring_a_backup_replaces_projects_and_settings() {
    let h = Harness::new();
    ctl(|c| c.create_project("Old"));
    let mut library = Library::default();
    library.create("From the backup", 1);
    let settings = Settings {
        language: model::Language::De,
        ..Settings::default()
    };
    let path = h.dir.path().join("backup.zip");
    ctl(|c| c.storage.export(&library, &settings, &path)).unwrap();

    ctl(|c| c.backup_picked(Some(Storage::read_backup(&path))));
    assert_eq!(h.store().get_dialog(), DialogKind::RestoreBackup);
    assert_eq!(h.store().get_backup_projects(), 1);

    ctl(|c| {
        let restored = c.storage.restore(&path);
        c.backup_restored(restored);
    });
    let names: Vec<_> = h.store().get_projects().iter().map(|p| p.name).collect();
    assert_eq!(names, ["From the backup"]);
    assert_eq!(h.store().get_language(), Language::De);
    assert_eq!(h.store().get_notice(), Notice::BackupRestored);
    assert_eq!(h.store().get_screen(), Screen::List);
    assert_eq!(h.saved().projects[0].name, "From the backup");
}

#[test]
fn unreadable_backups_are_reported() {
    let h = Harness::new();
    for (error, notice) in [
        (BackupError::TooNew, Notice::BackupTooNew),
        (BackupError::Invalid, Notice::BackupInvalid),
    ] {
        ctl(|c| c.backup_picked(Some(Err(error))));
        assert_eq!(h.store().get_notice(), notice);
        assert_eq!(h.store().get_dialog(), DialogKind::None);
        assert!(!h.store().get_backup_busy());
    }
}

/// Puts a PDF where another app hands it over, as `platform::receive_document` does.
fn hand_over(h: &Harness, bytes: &[u8], title: &str) {
    let (pdf, name) = Storage::incoming_paths(h.dir.path());
    std::fs::write(pdf, bytes).unwrap();
    std::fs::write(name, title).unwrap();
    ctl(Controller::check_incoming);
}

#[test]
fn a_part_made_four_times() {
    let h = Harness::new();
    ctl(|c| {
        c.create_project("Bunny");
        c.add_part(&PartForm {
            quantity: "4".into(),
            ..form("Leg", "2")
        });
        c.open_part(c.active_part().unwrap().1);
    });
    let part = || h.store().get_part();
    assert_eq!(
        (part().title.as_str(), part().can_next_piece),
        ("Leg 1/4", false)
    );
    ctl(|c| {
        c.count(Library::increment);
        c.count(Library::increment);
    });
    assert!(part().can_next_piece);
    ctl(|c| c.count(Library::next_piece));
    assert_eq!(
        (part().title.as_str(), part().row, part().piece),
        ("Leg 2/4", 0, 2)
    );
    assert_eq!(
        (
            h.store().get_current().rows_done,
            h.store().get_current().rows_total
        ),
        (2, 8)
    );
    assert_eq!(h.saved().projects[0].parts[0].pieces_done, 1);
}

#[test]
fn status_and_notes_are_saved() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    ctl(|c| {
        c.set_status(model::Status::Paused);
        c.set_notes("  4 mm hook\n");
    });
    assert_eq!(h.store().get_current().status, ProjectStatus::Paused);
    assert_eq!(h.store().get_current().notes, "4 mm hook");

    let saved = h.saved();
    let project = saved.get(id).unwrap();
    assert_eq!(project.status, model::Status::Paused);
    assert_eq!(project.notes, "4 mm hook");
}

#[test]
fn volume_keys_count_only_on_the_counter_screens() {
    let h = Harness::new();
    let (_, part) = project_with_part();
    ctl(|c| c.update_settings(|s| s.volume_keys = true));
    ctl(|c| c.volume_key(true));
    assert_eq!(
        h.store().get_current().active.row,
        0,
        "the project screen ignores them"
    );

    ctl(|c| c.open_part(part));
    ctl(|c| c.volume_key(true));
    ctl(|c| c.volume_key(true));
    ctl(|c| c.volume_key(false));
    assert_eq!(h.store().get_part().row, 1);

    h.store().set_dialog(DialogKind::Reset);
    ctl(|c| c.volume_key(true));
    assert_eq!(h.store().get_part().row, 1, "not while a dialog is open");
    h.store().set_dialog(DialogKind::None);

    ctl(|c| c.update_settings(|s| s.volume_keys = false));
    ctl(|c| c.volume_key(true));
    assert_eq!(h.store().get_part().row, 1);
}

#[test]
fn a_handed_over_pdf_becomes_a_new_project() {
    let h = Harness::new();
    hand_over(&h, PATTERN, "Amigurumi Cat.pdf");
    assert_eq!(h.store().get_dialog(), DialogKind::IncomingPdf);
    assert_eq!(h.store().get_incoming_title(), "Amigurumi Cat.pdf");

    ctl(Controller::incoming_to_new_project);
    assert_eq!(h.store().get_dialog(), DialogKind::None);
    assert_eq!(h.store().get_screen(), Screen::Project);
    assert_eq!(h.store().get_current().name, "Amigurumi Cat");
    let saved = h.saved();
    let pdf = saved.projects[0].pdf.as_ref().unwrap();
    assert_eq!(pdf.title, "Amigurumi Cat.pdf");
    assert_eq!(std::fs::read(pdf_path(&pdf.file)).unwrap(), PATTERN);
    assert!(ctl(|c| c.storage.incoming()).is_none());
}

#[test]
fn a_handed_over_pdf_replaces_the_pattern_of_a_project() {
    let h = Harness::new();
    let (id, _) = project_with_part();
    let old = import(id, 1, PATTERN);
    assert!(back());
    hand_over(&h, PATTERN, "new.pdf");

    ctl(|c| c.incoming_to_project(id));
    let saved = h.saved();
    let pdf = saved.get(id).unwrap().pdf.as_ref().unwrap();
    assert_eq!(pdf.title, "new.pdf");
    assert_ne!(pdf.file, old);
    assert!(!pdf_path(&old).exists());
    assert_eq!(h.store().get_current().name, "Bunny");
}

#[test]
fn a_handed_over_pdf_can_be_discarded() {
    let h = Harness::new();
    hand_over(&h, PATTERN, "pattern.pdf");
    assert!(back());
    assert_eq!(h.store().get_dialog(), DialogKind::None);
    assert!(ctl(|c| c.storage.incoming()).is_none());
    assert_eq!(h.saved().projects.len(), 0);

    hand_over(&h, b"not a pdf", "broken.pdf");
    assert_eq!(h.store().get_dialog(), DialogKind::None);
    assert_eq!(h.store().get_notice(), Notice::PdfInvalid);
    assert!(ctl(|c| c.storage.incoming()).is_none());
}

#[test]
fn fdroid_version_code_matches_version() {
    let manifest = include_str!("../Cargo.toml");
    let code: u32 = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version_code = "))
        .expect("version_code in [package.metadata.fdroid]")
        .parse()
        .unwrap();
    assert_eq!(
        code,
        changes::current(),
        "update version_code in Cargo.toml"
    );
}
