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

/// Creates the project "Bunny" with the part "Head" (10 rows) and returns their ids.
fn project_with_part() -> (u64, u64) {
    ctl(|c| {
        c.create_project("Bunny");
        c.add_part("Head", "10");
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

#[test]
fn fdroid_version_code_matches_version() {
    let manifest = include_str!("../Cargo.toml");
    let code: u32 = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version_code = "))
        .expect("version_code in [package.metadata.fdroid]")
        .parse()
        .unwrap();
    let [major, minor, patch] = [
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    ]
    .map(|part| part.parse::<u32>().unwrap());
    assert_eq!(
        code,
        1 << 24 | major << 16 | minor << 8 | patch,
        "update version_code in Cargo.toml"
    );
}
