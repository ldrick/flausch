#[cfg(target_os = "android")]
mod a11y;
mod changes;
mod model;
mod pdf;
mod platform;
mod storage;

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};

use model::{Library, PartSpec, Pdf, Settings};
use platform::{DocumentKind, PickResult};
use storage::{Backup, BackupError, Storage};

slint::include_modules!();

pub fn run() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let data_dir = platform::data_dir();
    // `flausch pattern.pdf` offers the PDF like Android's "Open with".
    #[cfg(not(target_os = "android"))]
    if let Some(path) = std::env::args_os().nth(1) {
        receive_file(&data_dir, std::path::Path::new(&path));
    }
    start(&app, data_dir)?;
    #[cfg(target_os = "android")]
    a11y::start(&app);
    app.run()?;
    stop();
    Ok(())
}

/// Loads the data in `data_dir` and connects it to `app` on the current thread.
/// `run` uses it, and the integration tests drive the app through it with Slint's testing backend.
pub fn start(app: &App, data_dir: PathBuf) -> Result<(), slint::PlatformError> {
    let storage = Storage::new(data_dir).map_err(|e| {
        slint::PlatformError::Other(format!("cannot create the data directory: {e}"))
    })?;

    let renderer = pdf::Renderer::new(|rendered| {
        let _ = slint::invoke_from_event_loop(move || {
            with(|c| c.pdf_rendered(rendered));
        });
    });

    let covers = pdf::Covers::new(|| {
        let _ = slint::invoke_from_event_loop(|| {
            with(|c| c.refresh());
        });
    });

    let controller = Controller {
        app: app.as_weak(),
        library: storage.load_library(),
        settings: storage.load_settings(),
        storage,
        nav: vec![Screen::List],
        current: None,
        pdf: PdfSession::default(),
        renderer,
        covers,
        save_timer: slint::Timer::default(),
        screen_on: None,
        volume_keys: None,
        incoming: None,
        new_pdf: None,
        new_parts: Vec::new(),
        last_back: None,
    };
    CONTROLLER.with_borrow_mut(|c| *c = Some(controller));
    with(|c| {
        c.apply_settings();
        c.refresh();
        c.create_missing_covers();
        c.check_incoming();
    });
    connect(app);
    platform::set_dark_theme(app.global::<Theme>().get_dark());
    Ok(())
}

/// Writes pending changes and disconnects the app started with `start`.
pub fn stop() {
    with(Controller::flush);
    CONTROLLER.with_borrow_mut(Option::take);
}

/// Copies `path` to where a PDF from another app waits for the user (`Storage::incoming_paths`).
#[cfg(not(target_os = "android"))]
fn receive_file(data_dir: &std::path::Path, path: &std::path::Path) {
    let (pdf, title) = Storage::incoming_paths(data_dir);
    let name = path
        .file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned());
    let result = std::fs::create_dir_all(data_dir)
        .and_then(|()| std::fs::copy(path, &pdf))
        .and_then(|_| std::fs::write(&title, name));
    if let Err(e) = result {
        eprintln!("flausch: cannot open {}: {e}", path.display());
    }
}

/// A PDF from another app is waiting, e.g. after the app came back to the foreground.
#[cfg_attr(
    not(target_os = "android"),
    allow(dead_code, reason = "used on Android")
)]
fn check_incoming() {
    with(Controller::check_incoming);
}

/// Volume up (`true`) or down was pressed while `platform::set_volume_keys` was on.
#[cfg_attr(
    not(target_os = "android"),
    allow(dead_code, reason = "used on Android")
)]
fn volume_key(up: bool) {
    with(|c| c.volume_key(up));
}

#[cfg(target_os = "android")]
// android-activity declares this as `extern "Rust"` and looks it up by name.
#[allow(unsafe_code, clippy::no_mangle_with_rust_abi)]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // A PDF opened with or shared to Flausch starts a second activity. It only hands the file
    // over to the app and brings that to the front.
    if platform::receive_document(&app) {
        return;
    }
    platform::init(app.clone());
    slint::android::init(app).unwrap();
    run().unwrap();
}

thread_local! {
    // Lives on the UI thread. Callbacks from other threads go through `invoke_from_event_loop`.
    // The allow works around a clippy false positive on aarch64-linux-android (it is already `const`).
    #[allow(clippy::missing_const_for_thread_local)]
    static CONTROLLER: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Controller) -> R) -> Option<R> {
    CONTROLLER.with_borrow_mut(|c| c.as_mut().map(f))
}

fn connect(app: &App) {
    let store = app.global::<Store>();
    store.on_back(|| with(Controller::back).unwrap_or(false));
    store.on_open_settings(|| {
        with(|c| c.navigate(Screen::Settings));
    });
    store.on_open_page(|screen| {
        with(|c| c.navigate(screen));
    });
    store.on_open_url(|url| platform::open_url(&url));
    store.on_open_project(|id| {
        with(|c| c.open_project(id as u64));
    });
    store.on_create_project(|form| {
        with(|c| c.create_project(&form));
    });
    store.on_edit_project(|name| {
        with(|c| c.edit_project(&name));
    });
    store.on_delete_project(|| {
        with(Controller::delete_project);
    });
    store.on_set_status(|status| {
        with(|c| c.set_status(status.into()));
    });
    store.on_set_notes(|notes| {
        with(|c| c.set_notes(&notes));
    });
    store.on_set_status_filter(|filter| {
        with(|c| c.set_status_filter(filter.into()));
    });
    store.on_set_project_sort(|sort| {
        with(|c| c.set_project_sort(sort.into()));
    });
    store.on_move_project(|id, by| {
        with(|c| c.move_project(id as u64, by));
    });
    store.on_query_changed(|| {
        with(|c| c.refresh());
    });
    store.on_select_part(|id| {
        with(|c| c.select_part(id as u64));
    });
    store.on_continue_project(|id| {
        with(|c| c.continue_project(id as u64));
    });
    store.on_open_part(|id| {
        with(|c| c.open_part(id as u64));
    });
    store.on_add_part(|form| {
        with(|c| c.add_part(&form));
    });
    store.on_edit_part(|part, form| {
        with(|c| c.edit_part(part as u64, &form));
    });
    store.on_delete_part(|part| {
        with(|c| c.delete_part(part as u64));
    });
    store.on_increment(|| {
        with(|c| c.count(Library::increment));
    });
    store.on_decrement(|| {
        with(|c| c.count(Library::decrement));
    });
    store.on_reset(|| {
        with(|c| c.count(Library::reset));
    });
    store.on_next_piece(|| {
        with(|c| c.count(Library::next_piece));
    });
    connect_pdf_and_settings(&store);
}

fn connect_pdf_and_settings(store: &Store) {
    store.on_pick_pdf(|| {
        with(Controller::pick_pdf);
    });
    store.on_pick_new_pdf(|| {
        with(Controller::pick_new_pdf);
    });
    store.on_discard_new_pdf(|| {
        with(Controller::discard_new_pdf);
    });
    store.on_add_new_part(|form| {
        with(|c| c.add_new_part(form));
    });
    store.on_remove_new_part(|index| {
        with(|c| c.remove_new_part(index));
    });
    store.on_discard_new_project(|| {
        with(Controller::discard_new_project);
    });
    store.on_remove_pdf(|| {
        with(Controller::remove_pdf);
    });
    store.on_open_pdf(|| {
        with(Controller::open_pdf);
    });
    store.on_pdf_view_changed(|page, zoom, scroll_x, scroll_y, width| {
        with(|c| c.pdf_view_changed(page, zoom, scroll_x, scroll_y, width));
    });
    store.on_pdf_line_shown(|visible, center| {
        with(|c| c.pdf_line_shown(visible, center));
    });
    store.on_pdf_line_moved(|line| {
        with(|c| c.pdf_line_moved(line));
    });
    store.on_dismiss_notice(|| {
        with(|c| c.notify(Notice::None));
    });
    store.on_set_language(|language| {
        with(|c| c.update_settings(|s| s.language = language.into()));
    });
    store.on_set_appearance(|appearance| {
        with(|c| c.update_settings(|s| s.appearance = appearance.into()));
    });
    store.on_dark_changed(platform::set_dark_theme);
    store.on_hide_text_handles(platform::hide_text_handles);
    store.on_set_keep_screen_on(|on| {
        with(|c| c.update_settings(|s| s.keep_screen_on = on));
    });
    store.on_set_line_color(|color| {
        with(|c| c.update_settings(|s| s.line_color = color.into()));
    });
    store.on_set_volume_keys(|on| {
        with(|c| c.update_settings(|s| s.volume_keys = on));
    });
    store.on_export_backup(|| {
        with(Controller::export_backup);
    });
    store.on_import_backup(|| {
        with(Controller::import_backup);
    });
    store.on_restore_backup(|| {
        with(Controller::restore_backup);
    });
    store.on_incoming_to_new_project(|| {
        with(Controller::incoming_to_new_project);
    });
    store.on_incoming_to_project(|id| {
        with(|c| c.incoming_to_project(id as u64));
    });
    store.on_discard_incoming(|| {
        with(Controller::discard_incoming);
    });
}

#[derive(Default)]
struct PdfSession {
    /// Serial of the newest render request; older results are dropped.
    serial: u64,
    /// Number of results shown so far, exposed to the UI as `PdfView.serial`.
    shown: i32,
    page_count: u32,
    view_width: f32,
    aspect: Option<f32>,
    /// Page and width of the newest render request.
    requested: Option<(u32, u32)>,
}

struct Controller {
    app: slint::Weak<App>,
    storage: Storage,
    library: Library,
    settings: Settings,
    nav: Vec<Screen>,
    current: Option<u64>,
    pdf: PdfSession,
    renderer: pdf::Renderer,
    covers: pdf::Covers,
    save_timer: slint::Timer,
    screen_on: Option<bool>,
    volume_keys: Option<bool>,
    /// Title of the PDF from another app that the incoming dialog asks about.
    incoming: Option<String>,
    /// File and title of the PDF picked in the new project form.
    new_pdf: Option<(String, String)>,
    /// Parts added in the new project form.
    new_parts: Vec<PartForm>,
    last_back: Option<Instant>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

impl Controller {
    fn app(&self) -> App {
        self.app.upgrade().expect("the app window is gone")
    }

    fn save(&mut self) {
        self.save_timer.stop();
        if let Err(e) = self.storage.save_library(&self.library) {
            eprintln!("flausch: cannot save projects: {e}");
        }
    }

    /// Saves after a short pause, for frequent changes like scrolling.
    fn save_later(&mut self) {
        self.save_timer
            .start(slint::TimerMode::SingleShot, Duration::from_secs(1), || {
                with(Controller::save);
            });
    }

    fn flush(&mut self) {
        if self.save_timer.running() {
            self.save();
        }
    }

    /// The projects in the list: filtered, searched and sorted.
    fn shown(&self) -> Vec<&model::Project> {
        let app = self.app();
        let store = app.global::<Store>();
        let query = store.get_query();
        let names = store.get_status_names();
        let status_name = |status: model::Status| {
            let index = match status {
                model::Status::Planned => 0,
                model::Status::Active => 1,
                model::Status::Paused => 2,
                model::Status::Finished => 3,
            };
            names.row_data(index).unwrap_or_default()
        };
        self.library
            .sorted(self.settings.project_filter, self.settings.project_sort)
            .into_iter()
            .filter(|p| p.matches(&query, &status_name(p.status)))
            .collect()
    }

    fn project_items(&self, projects: &[&model::Project]) -> Vec<ProjectItem> {
        let group = |i: usize| projects[i].status.group();
        (0..projects.len())
            .map(|i| {
                let start = (0..i).rev().take_while(|&j| group(j) == group(i)).last();
                let end = (i + 1..projects.len())
                    .take_while(|&j| group(j) == group(i))
                    .last();
                ProjectItem {
                    group_start: start.unwrap_or(i) as i32,
                    group_end: end.unwrap_or(i) as i32,
                    ..self.project_item(projects[i])
                }
            })
            .collect()
    }

    fn refresh(&self) {
        let app = self.app();
        let store = app.global::<Store>();
        let query = store.get_query();
        let filter = self.settings.project_filter;
        // While searching, finished projects show up right in the list.
        let collapse = filter == model::StatusFilter::All && query.trim().is_empty();
        let (finished, shown): (Vec<_>, Vec<_>) = self
            .shown()
            .into_iter()
            .partition(|p| collapse && p.status == model::Status::Finished);
        store.set_projects(ModelRc::new(VecModel::from(self.project_items(&shown))));
        store.set_finished(ModelRc::new(VecModel::from(self.project_items(&finished))));
        store.set_has_projects(!self.library.projects.is_empty());
        if let Some(project) = self.current.and_then(|id| self.library.get(id)) {
            store.set_current(self.project_item(project));
            let parts: Vec<PartItem> = project.parts.iter().map(part_item).collect();
            store.set_parts(ModelRc::new(VecModel::from(parts)));
            store.set_part(project.active().map_or_default(part_item));
        }
    }

    fn notify(&self, notice: Notice) {
        self.app().global::<Store>().set_notice(notice);
    }

    // Navigation

    fn screen(&self) -> Screen {
        *self.nav.last().unwrap_or(&Screen::List)
    }

    fn show(&mut self) {
        let screen = self.screen();
        self.app().global::<Store>().set_screen(screen);
        self.update_keep_screen_on();
        self.update_volume_keys();
    }

    fn navigate(&mut self, screen: Screen) {
        if self.screen() != screen {
            self.nav.push(screen);
            self.show();
        }
    }

    fn back(&mut self) -> bool {
        // With predictive back (targetSdk 34+), Android can deliver one press both as a key
        // event and through the OnBackInvokedCallback; Slint forwards both as Key.Back.
        let now = Instant::now();
        if self
            .last_back
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(300))
        {
            return true;
        }
        let app = self.app();
        let store = app.global::<Store>();
        if store.get_dialog() != DialogKind::None {
            match store.get_dialog() {
                DialogKind::IncomingPdf => self.discard_incoming(),
                DialogKind::Create => self.discard_new_project(),
                _ => {}
            }
            store.set_dialog(DialogKind::None);
            self.last_back = Some(now);
            return true;
        }
        if self.screen() == Screen::Pdf && store.get_counter_expanded() {
            store.set_counter_expanded(false);
            self.last_back = Some(now);
            return true;
        }
        if self.screen() == Screen::List && store.get_searching() {
            store.set_searching(false);
            store.set_query("".into());
            self.last_back = Some(now);
            self.refresh();
            return true;
        }
        if self.nav.len() <= 1 {
            return false;
        }
        self.last_back = Some(now);
        if self.nav.pop() == Some(Screen::Pdf) {
            self.flush();
        }
        self.refresh();
        self.show();
        true
    }

    fn update_keep_screen_on(&mut self) {
        let on = self.settings.keep_screen_on
            && matches!(self.screen(), Screen::Project | Screen::Part | Screen::Pdf);
        if self.screen_on != Some(on) {
            self.screen_on = Some(on);
            platform::keep_screen_on(on);
        }
    }

    /// The volume keys count while the part screen or the PDF view is shown.
    fn update_volume_keys(&mut self) {
        let on = self.settings.volume_keys && matches!(self.screen(), Screen::Part | Screen::Pdf);
        if self.volume_keys != Some(on) {
            self.volume_keys = Some(on);
            platform::set_volume_keys(on);
        }
    }

    fn volume_key(&mut self, up: bool) {
        let open = self.app().global::<Store>().get_dialog() != DialogKind::None;
        if self.volume_keys != Some(true) || open {
            return;
        }
        self.count(if up {
            Library::increment
        } else {
            Library::decrement
        });
    }

    // Projects

    fn open_project(&mut self, id: u64) {
        if self.library.get(id).is_none() {
            return;
        }
        self.current = Some(id);
        self.nav = vec![Screen::List, Screen::Project];
        self.refresh();
        self.show();
    }

    /// Opens the pattern of a project straight from the list; Back returns to the list.
    fn continue_project(&mut self, id: u64) {
        if self.library.get(id).and_then(|p| p.pdf.as_ref()).is_none() {
            return self.open_project(id);
        }
        self.current = Some(id);
        self.nav = vec![Screen::List];
        self.refresh();
        self.open_pdf();
    }

    fn create_project(&mut self, form: &ProjectForm) {
        if form.name.trim().is_empty() {
            return;
        }
        let now = now();
        let id = self.library.create(&form.name, now);
        self.library.set_status(id, form.status.into(), now);
        self.library.set_notes(id, &form.notes, now);
        let pdf = self.new_pdf.take();
        if let Some((file, title)) = &pdf {
            let project = self.library.touch(id, now).expect("just created");
            project.pdf = Some(Pdf::new(file.clone(), title.clone()));
        }
        let parts = std::mem::take(&mut self.new_parts);
        let first = parts
            .iter()
            .filter_map(|form| self.library.add_part(id, &part_spec(form), now))
            .reduce(|first, _| first);
        // Work starts with the first part.
        if let Some(first) = first {
            self.library.activate_part(id, first);
        }
        self.discard_new_project();
        self.save();
        if let Some((file, _)) = &pdf {
            self.create_cover(file);
        }
        self.open_project(id);
    }

    fn edit_project(&mut self, name: &str) {
        let Some(id) = self.current else { return };
        if name.trim().is_empty() {
            return;
        }
        self.library.rename(id, name, now());
        self.save();
        self.refresh();
    }

    fn set_status_filter(&mut self, filter: model::StatusFilter) {
        self.update_settings(|s| s.project_filter = filter);
        self.refresh();
    }

    fn set_project_sort(&mut self, sort: model::ProjectSort) {
        self.update_settings(|s| s.project_sort = sort);
        self.refresh();
    }

    /// Moves a project `by` places in the shown list, but not out of its status group.
    fn move_project(&mut self, id: u64, by: i32) {
        let shown: Vec<_> = self
            .shown()
            .iter()
            .map(|p| (p.id, p.status.group()))
            .collect();
        let Some(index) = shown.iter().position(|&(p, _)| p == id) else {
            return;
        };
        let group = shown[index].1;
        let start = shown.iter().position(|&(_, g)| g == group).unwrap_or(index);
        let end = shown
            .iter()
            .rposition(|&(_, g)| g == group)
            .unwrap_or(index);
        let to = index.saturating_add_signed(by as isize).clamp(start, end);
        if to == index {
            return;
        }
        if self
            .library
            .move_project(id, shown[to].0, self.settings.project_sort)
        {
            self.update_settings(|s| s.project_sort = model::ProjectSort::Custom);
            self.save_later();
            self.refresh();
        }
    }

    fn set_status(&mut self, status: model::Status) {
        let Some(id) = self.current else { return };
        self.library.set_status(id, status, now());
        self.save();
        self.refresh();
    }

    fn set_notes(&mut self, notes: &str) {
        let Some(id) = self.current else { return };
        self.library.set_notes(id, notes, now());
        self.save();
        self.refresh();
    }

    fn delete_project(&mut self) {
        let Some(id) = self.current.take() else {
            return;
        };
        if let Some(pdf) = self.library.remove(id).and_then(|p| p.pdf) {
            self.storage.remove_pdf(&pdf.file);
        }
        self.save();
        self.nav = vec![Screen::List];
        self.refresh();
        self.show();
    }

    // Parts

    fn active_part(&self) -> Option<(u64, u64)> {
        let id = self.current?;
        Some((id, self.library.get(id)?.active_part?))
    }

    fn open_part(&mut self, part: u64) {
        let Some(id) = self.current else { return };
        if self.library.activate_part(id, part) {
            self.save();
            self.refresh();
            if self.current_pdf().is_some() {
                self.open_pdf();
            } else {
                self.navigate(Screen::Part);
            }
        }
    }

    /// Makes `part` the active part without leaving the current screen, e.g. in the PDF view.
    fn select_part(&mut self, part: u64) {
        let Some(id) = self.current else { return };
        if self.library.activate_part(id, part) {
            self.save();
            self.refresh();
        }
    }

    fn add_part(&mut self, form: &PartForm) {
        let Some(id) = self.current else { return };
        if form.name.trim().is_empty() {
            return;
        }
        // Stay on the project screen; the new part becomes the one being worked on.
        if let Some(part) = self.library.add_part(id, &part_spec(form), now()) {
            self.save();
            self.select_part(part);
        }
    }

    fn edit_part(&mut self, part: u64, form: &PartForm) {
        let Some(id) = self.current else { return };
        if form.name.trim().is_empty() {
            return;
        }
        self.library.edit_part(id, part, &part_spec(form), now());
        self.save();
        self.refresh();
    }

    fn delete_part(&mut self, part: u64) {
        let Some(id) = self.current else { return };
        self.library.remove_part(id, part, now());
        self.save();
        self.nav = vec![Screen::List, Screen::Project];
        self.refresh();
        self.show();
    }

    fn count(&mut self, op: impl FnOnce(&mut Library, u64, u64, u64) -> Option<u32>) {
        let Some((id, part)) = self.active_part() else {
            return;
        };
        if op(&mut self.library, id, part, now()).is_some() {
            self.save();
            self.refresh();
        }
    }

    // PDF patterns

    fn pick_pdf(&mut self) {
        let Some(id) = self.current else { return };
        let file = Storage::new_pdf_file(id, now_nanos());
        let dest = self.storage.pdf_path(&file);
        platform::pick_document(DocumentKind::Pdf, dest, move |result| {
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| c.pdf_picked(id, &file, result));
            });
        });
    }

    /// Checks a picked PDF and returns its title, or removes the file and tells why not.
    fn checked_pdf(&mut self, file: &str, result: PickResult) -> Option<String> {
        let title = match result {
            PickResult::Picked { title } => title,
            PickResult::Cancelled => {
                self.storage.remove_pdf(file);
                return None;
            }
            PickResult::Failed(e) => {
                eprintln!("flausch: PDF import failed: {e}");
                self.storage.remove_pdf(file);
                self.notify(Notice::PdfImportFailed);
                return None;
            }
        };
        if let Err(e) = platform::page_count(&self.storage.pdf_path(file)).and_then(|n| match n {
            0 => Err("no pages".into()),
            n => Ok(n),
        }) {
            eprintln!("flausch: {title} is not a usable PDF: {e}");
            self.storage.remove_pdf(file);
            self.notify(Notice::PdfInvalid);
            return None;
        }
        Some(title)
    }

    fn pdf_picked(&mut self, id: u64, file: &str, result: PickResult) {
        let Some(title) = self.checked_pdf(file, result) else {
            return;
        };
        let Some(project) = self.library.touch(id, now()) else {
            self.storage.remove_pdf(file);
            return;
        };
        if let Some(old) = project.pdf.replace(Pdf::new(file.to_owned(), title)) {
            self.storage.remove_pdf(&old.file);
        }
        self.save();
        self.refresh();
        self.create_cover(file);
    }

    fn pick_new_pdf(&mut self) {
        // Project ids start at 1, so 0 marks a PDF without a project yet.
        let file = Storage::new_pdf_file(0, now_nanos());
        let dest = self.storage.pdf_path(&file);
        platform::pick_document(DocumentKind::Pdf, dest, move |result| {
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| c.new_pdf_picked(&file, result));
            });
        });
    }

    fn new_pdf_picked(&mut self, file: &str, result: PickResult) {
        let Some(title) = self.checked_pdf(file, result) else {
            return;
        };
        let app = self.app();
        let store = app.global::<Store>();
        if store.get_dialog() != DialogKind::Create {
            return self.storage.remove_pdf(file);
        }
        self.discard_new_pdf();
        store.set_new_pdf_title(title.as_str().into());
        self.new_pdf = Some((file.to_owned(), title));
    }

    fn discard_new_pdf(&mut self) {
        if let Some((file, _)) = self.new_pdf.take() {
            self.storage.remove_pdf(&file);
        }
        self.app().global::<Store>().set_new_pdf_title("".into());
    }

    fn add_new_part(&mut self, form: PartForm) {
        if form.name.trim().is_empty() {
            return;
        }
        self.new_parts.push(form);
        self.show_new_parts();
    }

    fn remove_new_part(&mut self, index: i32) {
        if let Ok(index) = usize::try_from(index)
            && index < self.new_parts.len()
        {
            self.new_parts.remove(index);
            self.show_new_parts();
        }
    }

    fn show_new_parts(&self) {
        self.app()
            .global::<Store>()
            .set_new_parts(ModelRc::new(VecModel::from(self.new_parts.clone())));
    }

    fn discard_new_project(&mut self) {
        self.discard_new_pdf();
        self.new_parts.clear();
        self.show_new_parts();
    }

    /// Asks what to do with a PDF that another app opened with or shared to Flausch.
    fn check_incoming(&mut self) {
        if self.incoming.is_some() {
            return;
        }
        let Some((path, title)) = self.storage.incoming() else {
            return;
        };
        if let Err(e) = platform::page_count(&path).and_then(|n| match n {
            0 => Err("no pages".into()),
            n => Ok(n),
        }) {
            eprintln!("flausch: {title} is not a usable PDF: {e}");
            self.storage.discard_incoming();
            return self.notify(Notice::PdfInvalid);
        }
        let app = self.app();
        let store = app.global::<Store>();
        store.set_incoming_title(title.as_str().into());
        store.set_dialog(DialogKind::IncomingPdf);
        self.incoming = Some(title);
    }

    fn incoming_to_new_project(&mut self) {
        let Some(title) = self.incoming.clone() else {
            return;
        };
        let name = match title.rsplit_once('.') {
            Some((stem, ext)) if ext.eq_ignore_ascii_case("pdf") && !stem.trim().is_empty() => stem,
            _ => &title,
        };
        let id = self.library.create(name, now());
        self.incoming_to_project(id);
    }

    /// Makes the incoming PDF the pattern of project `id`, replacing its old one, and opens it.
    fn incoming_to_project(&mut self, id: u64) {
        let Some(title) = self.incoming.take() else {
            return;
        };
        self.app().global::<Store>().set_dialog(DialogKind::None);
        let file = Storage::new_pdf_file(id, now_nanos());
        if self.library.get(id).is_none() {
            return self.storage.discard_incoming();
        }
        if let Err(e) = self.storage.adopt_incoming(&file) {
            eprintln!("flausch: cannot import {title}: {e}");
            self.storage.discard_incoming();
            return self.notify(Notice::PdfImportFailed);
        }
        let project = self.library.touch(id, now()).expect("checked above");
        if let Some(old) = project.pdf.replace(Pdf::new(file.clone(), title)) {
            self.storage.remove_pdf(&old.file);
        }
        self.save();
        self.create_cover(&file);
        self.open_project(id);
    }

    fn discard_incoming(&mut self) {
        self.incoming = None;
        self.storage.discard_incoming();
        let app = self.app();
        let store = app.global::<Store>();
        if store.get_dialog() == DialogKind::IncomingPdf {
            store.set_dialog(DialogKind::None);
        }
    }

    fn remove_pdf(&mut self) {
        let Some(id) = self.current else { return };
        if let Some(old) = self.library.touch(id, now()).and_then(|p| p.pdf.take()) {
            self.storage.remove_pdf(&old.file);
            self.save();
            self.refresh();
        }
    }

    fn create_cover(&self, file: &str) {
        self.covers
            .request(self.storage.pdf_path(file), self.storage.cover_path(file));
    }

    /// Covers are missing for PDFs imported before covers existed, or if rendering failed.
    fn create_missing_covers(&self) {
        for project in &self.library.projects {
            if let Some(pdf) = &project.pdf
                && !self.storage.cover_path(&pdf.file).exists()
            {
                self.create_cover(&pdf.file);
            }
        }
    }

    fn project_item(&self, project: &model::Project) -> ProjectItem {
        let cover = project
            .pdf
            .as_ref()
            .map(|pdf| self.storage.cover_path(&pdf.file))
            .filter(|path| path.exists())
            .and_then(|path| Image::load_from_path(&path).ok());
        let (done, total) = project.progress();
        ProjectItem {
            id: project.id as i32,
            name: project.name.as_str().into(),
            status: project.status.into(),
            notes: project.notes.as_str().into(),
            part_count: project.parts.len() as i32,
            has_active: project.active().is_some(),
            active: project.active().map_or_default(part_item),
            rows_done: done as i32,
            rows_total: total as i32,
            has_pdf: project.pdf.is_some(),
            pdf_title: project
                .pdf
                .as_ref()
                .map_or_default(|p| p.title.as_str())
                .into(),
            has_cover: cover.is_some(),
            cover: cover.unwrap_or_default(),
            group_start: 0,
            group_end: 0,
        }
    }

    fn current_pdf(&self) -> Option<&Pdf> {
        self.library.get(self.current?)?.pdf.as_ref()
    }

    fn open_pdf(&mut self) {
        let Some(pdf) = self.current_pdf().cloned() else {
            return;
        };
        self.pdf = PdfSession {
            serial: self.pdf.serial,
            ..PdfSession::default()
        };
        self.app().global::<Store>().set_pdf(PdfView {
            page: pdf.page as i32,
            page_count: 0,
            zoom: pdf.zoom,
            scroll_x: pdf.scroll_x,
            scroll_y: pdf.scroll_y,
            image: Image::default(),
            aspect: 1.414,
            loading: true,
            serial: 0,
            line_visible: pdf.show_line,
            line_saved: pdf.line(pdf.page).is_some(),
            line: pdf.line(pdf.page).unwrap_or(0.5),
        });
        // The page view reports its size when it appears, which triggers the first render.
        self.navigate(Screen::Pdf);
    }

    fn pdf_view_changed(
        &mut self,
        page: i32,
        zoom: f32,
        scroll_x: f32,
        scroll_y: f32,
        view_width: f32,
    ) {
        let Some(id) = self.current else { return };
        let mut page = page.max(0) as u32;
        if self.pdf.page_count > 0 {
            page = page.min(self.pdf.page_count - 1);
        }
        let Some(pdf) = self.library.get_mut(id).and_then(|p| p.pdf.as_mut()) else {
            return;
        };
        pdf.set_view(page, zoom, scroll_x, scroll_y);
        let (file, zoom) = (pdf.file.clone(), pdf.zoom);
        self.save_later();
        self.pdf.view_width = view_width;

        let app = self.app();
        let store = app.global::<Store>();
        let mut view = store.get_pdf();
        if view.page != page as i32 {
            view.page = page as i32;
            view.loading = true;
            if let Some(pdf) = self.current_pdf() {
                set_line_view(&mut view, pdf);
            }
            store.set_pdf(view);
        }

        let width = pdf::render_width(view_width, zoom, self.pdf.aspect.unwrap_or(1.414));
        // Skip re-rendering for small zoom changes; the image is scaled meanwhile.
        if let Some((requested_page, requested_width)) = self.pdf.requested
            && requested_page == page
            && (width as f32 / requested_width as f32 - 1.0).abs() < 0.15
        {
            return;
        }
        self.pdf.requested = Some((page, width));
        self.pdf.serial += 1;
        self.renderer.request(pdf::Request {
            serial: self.pdf.serial,
            path: self.storage.pdf_path(&file),
            page,
            width,
        });
    }

    /// Shows or hides the reading line. A page without a saved line gets it at `center`.
    fn pdf_line_shown(&mut self, visible: bool, center: f32) {
        self.update_line(|pdf, page| {
            pdf.show_line = visible;
            if visible && pdf.line(page).is_none() {
                pdf.set_line(page, center);
            }
        });
    }

    fn pdf_line_moved(&mut self, line: f32) {
        self.update_line(|pdf, page| pdf.set_line(page, line));
    }

    fn update_line(&mut self, change: impl FnOnce(&mut Pdf, u32)) {
        let Some(id) = self.current else { return };
        let app = self.app();
        let store = app.global::<Store>();
        let mut view = store.get_pdf();
        let Some(pdf) = self.library.get_mut(id).and_then(|p| p.pdf.as_mut()) else {
            return;
        };
        change(pdf, view.page.max(0) as u32);
        set_line_view(&mut view, pdf);
        store.set_pdf(view);
        self.save_later();
    }

    fn pdf_rendered(&mut self, rendered: pdf::Rendered) {
        if rendered.serial != self.pdf.serial || self.screen() != Screen::Pdf {
            return;
        }
        let app = self.app();
        let store = app.global::<Store>();
        let mut view = store.get_pdf();
        view.loading = false;
        match rendered.result {
            Ok(buffer) => {
                let aspect = buffer.height() as f32 / buffer.width().max(1) as f32;
                let aspect_changed = self.pdf.aspect.is_some_and(|a| (a - aspect).abs() > 0.01);
                self.pdf.aspect = Some(aspect);
                self.pdf.page_count = rendered.page_count;
                self.pdf.shown += 1;
                view.page = rendered.page as i32;
                view.page_count = rendered.page_count as i32;
                view.aspect = aspect;
                view.image = Image::from_rgba8(buffer);
                view.serial = self.pdf.shown;
                store.set_pdf(view);
                // The render width depends on the aspect ratio (memory cap), so re-check it.
                if aspect_changed || self.pdf.shown == 1 {
                    let zoom = self.current_pdf().map_or(1.0, |p| p.zoom);
                    let width = pdf::render_width(self.pdf.view_width, zoom, aspect);
                    if width < rendered.width {
                        self.pdf.requested = None;
                        let pdf = self.current_pdf().cloned();
                        if let Some(pdf) = pdf {
                            self.pdf_view_changed(
                                rendered.page as i32,
                                pdf.zoom,
                                pdf.scroll_x,
                                pdf.scroll_y,
                                self.pdf.view_width,
                            );
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("flausch: cannot render page {}: {e}", rendered.page);
                self.pdf.page_count = rendered.page_count;
                view.page_count = rendered.page_count as i32;
                store.set_pdf(view);
                if rendered.page_count > 0 && rendered.page >= rendered.page_count {
                    // The saved page no longer exists (e.g. a shorter file): show the last one.
                    self.pdf.requested = None;
                    self.pdf_view_changed(
                        rendered.page_count as i32 - 1,
                        1.0,
                        0.0,
                        0.0,
                        self.pdf.view_width,
                    );
                } else {
                    self.notify(Notice::PdfRenderFailed);
                }
            }
        }
    }

    // Backups

    fn set_busy(&self, busy: bool) {
        self.app().global::<Store>().set_backup_busy(busy);
    }

    /// Writes a backup on a worker thread and lets the user choose where to save it.
    fn export_backup(&mut self) {
        self.flush();
        self.set_busy(true);
        let storage = self.storage.clone();
        let library = self.library.clone();
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            let path = storage.export_path();
            if let Err(e) = storage.export(&library, &settings, &path) {
                eprintln!("flausch: cannot write the backup: {e}");
                let _ = std::fs::remove_file(&path);
                return backup_exported(PickResult::Failed(e.to_string()));
            }
            let name = storage::backup_name(now());
            platform::save_document(DocumentKind::Backup, path.clone(), name, move |result| {
                let _ = std::fs::remove_file(&path);
                backup_exported(result);
            });
        });
    }

    fn import_backup(&mut self) {
        self.set_busy(true);
        let path = self.storage.import_path();
        platform::pick_document(DocumentKind::Backup, path.clone(), move |result| {
            let backup = match result {
                PickResult::Picked { .. } => Some(Storage::read_backup(&path)),
                PickResult::Cancelled => None,
                PickResult::Failed(e) => {
                    eprintln!("flausch: cannot copy the backup: {e}");
                    Some(Err(BackupError::Io(std::io::Error::other(e))))
                }
            };
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| c.backup_picked(backup));
            });
        });
    }

    fn backup_picked(&mut self, backup: Option<Result<Backup, BackupError>>) {
        self.set_busy(false);
        let app = self.app();
        let store = app.global::<Store>();
        match backup {
            Some(Ok(backup)) => {
                store.set_backup_projects(backup.library.projects.len() as i32);
                store.set_dialog(DialogKind::RestoreBackup);
                return;
            }
            Some(Err(e)) => {
                eprintln!("flausch: cannot read the backup: {e}");
                self.notify(match e {
                    BackupError::TooNew => Notice::BackupTooNew,
                    BackupError::Invalid => Notice::BackupInvalid,
                    BackupError::Io(_) => Notice::BackupImportFailed,
                });
            }
            None => {}
        }
        let _ = std::fs::remove_file(self.storage.import_path());
    }

    /// Replaces all data with the backup chosen before, on a worker thread.
    fn restore_backup(&mut self) {
        self.app().global::<Store>().set_dialog(DialogKind::None);
        self.flush();
        self.set_busy(true);
        let storage = self.storage.clone();
        std::thread::spawn(move || {
            let path = storage.import_path();
            let result = storage.restore(&path);
            let _ = std::fs::remove_file(&path);
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| c.backup_restored(result));
            });
        });
    }

    fn backup_restored(&mut self, result: Result<Backup, BackupError>) {
        self.set_busy(false);
        let backup = match result {
            Ok(backup) => backup,
            Err(e) => {
                eprintln!("flausch: cannot restore the backup: {e}");
                return self.notify(Notice::BackupRestoreFailed);
            }
        };
        self.save_timer.stop();
        self.library = backup.library;
        if let Some(settings) = backup.settings {
            self.settings = settings;
            self.apply_settings();
        }
        self.current = None;
        self.nav = vec![Screen::List];
        self.refresh();
        self.show();
        self.create_missing_covers();
        self.notify(Notice::BackupRestored);
    }

    // Settings

    fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings);
        if let Err(e) = self.storage.save_settings(&self.settings) {
            eprintln!("flausch: cannot save settings: {e}");
        }
        self.apply_settings();
    }

    fn apply_settings(&mut self) {
        let app = self.app();
        let store = app.global::<Store>();
        store.set_version(env!("CARGO_PKG_VERSION").into());
        store.set_language(self.settings.language.into());
        store.set_appearance(self.settings.appearance.into());
        store.set_keep_screen_on(self.settings.keep_screen_on);
        store.set_line_color(self.settings.line_color.into());
        store.set_volume_keys(self.settings.volume_keys);
        store.set_status_filter(self.settings.project_filter.into());
        store.set_project_sort(self.settings.project_sort.into());
        let language = self
            .settings
            .language
            .translation(platform::system_language().as_deref());
        if let Err(e) = slint::select_bundled_translation(language) {
            eprintln!("flausch: cannot select language {language}: {e}");
        }
        let changes: Vec<ChangeItem> = changes::list(language)
            .into_iter()
            .map(|(version, points)| ChangeItem {
                version: version.into(),
                points: ModelRc::new(VecModel::from(
                    points
                        .into_iter()
                        .map(slint::SharedString::from)
                        .collect::<Vec<_>>(),
                )),
            })
            .collect();
        store.set_changes(ModelRc::new(VecModel::from(changes)));
        self.update_keep_screen_on();
        self.update_volume_keys();
    }
}

fn backup_exported(result: PickResult) {
    let _ = slint::invoke_from_event_loop(move || {
        with(|c| {
            c.set_busy(false);
            match result {
                PickResult::Picked { .. } => c.notify(Notice::BackupSaved),
                PickResult::Cancelled => {}
                PickResult::Failed(e) => {
                    eprintln!("flausch: cannot save the backup: {e}");
                    c.notify(Notice::BackupSaveFailed);
                }
            }
        });
    });
}

/// Copies the reading line of the page shown in `view` from `pdf`.
fn set_line_view(view: &mut PdfView, pdf: &Pdf) {
    let line = pdf.line(view.page.max(0) as u32);
    view.line_visible = pdf.show_line;
    view.line_saved = line.is_some();
    view.line = line.unwrap_or(0.5);
}

fn parse_number(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

fn part_spec(form: &PartForm) -> PartSpec<'_> {
    PartSpec {
        name: &form.name,
        row_goal: parse_number(&form.rows),
        quantity: parse_number(&form.quantity).unwrap_or(1),
    }
}

fn part_item(part: &model::Part) -> PartItem {
    let piece = part.pieces_done + 1;
    PartItem {
        id: part.id as i32,
        name: part.name.as_str().into(),
        title: if part.quantity > 1 {
            format!("{} {piece}/{}", part.name, part.quantity).into()
        } else {
            part.name.as_str().into()
        },
        row: part.row as i32,
        goal: part.row_goal.unwrap_or(0) as i32,
        quantity: part.quantity as i32,
        piece: piece as i32,
        can_next_piece: part.can_start_next_piece(),
    }
}

impl From<model::Status> for ProjectStatus {
    fn from(status: model::Status) -> Self {
        match status {
            model::Status::Planned => ProjectStatus::Planned,
            model::Status::Active => ProjectStatus::Active,
            model::Status::Paused => ProjectStatus::Paused,
            model::Status::Finished => ProjectStatus::Finished,
        }
    }
}

impl From<ProjectStatus> for model::Status {
    fn from(status: ProjectStatus) -> Self {
        match status {
            ProjectStatus::Planned => model::Status::Planned,
            ProjectStatus::Active => model::Status::Active,
            ProjectStatus::Paused => model::Status::Paused,
            ProjectStatus::Finished => model::Status::Finished,
        }
    }
}

impl From<model::StatusFilter> for StatusFilter {
    fn from(filter: model::StatusFilter) -> Self {
        match filter {
            model::StatusFilter::All => StatusFilter::All,
            model::StatusFilter::Active => StatusFilter::Active,
            model::StatusFilter::Planned => StatusFilter::Planned,
            model::StatusFilter::Paused => StatusFilter::Paused,
            model::StatusFilter::Finished => StatusFilter::Finished,
        }
    }
}

impl From<StatusFilter> for model::StatusFilter {
    fn from(filter: StatusFilter) -> Self {
        match filter {
            StatusFilter::All => model::StatusFilter::All,
            StatusFilter::Active => model::StatusFilter::Active,
            StatusFilter::Planned => model::StatusFilter::Planned,
            StatusFilter::Paused => model::StatusFilter::Paused,
            StatusFilter::Finished => model::StatusFilter::Finished,
        }
    }
}

impl From<model::ProjectSort> for ProjectSort {
    fn from(sort: model::ProjectSort) -> Self {
        match sort {
            model::ProjectSort::Recent => ProjectSort::Recent,
            model::ProjectSort::Name => ProjectSort::Name,
            model::ProjectSort::Newest => ProjectSort::Newest,
            model::ProjectSort::Progress => ProjectSort::Progress,
            model::ProjectSort::Custom => ProjectSort::Custom,
        }
    }
}

impl From<ProjectSort> for model::ProjectSort {
    fn from(sort: ProjectSort) -> Self {
        match sort {
            ProjectSort::Recent => model::ProjectSort::Recent,
            ProjectSort::Name => model::ProjectSort::Name,
            ProjectSort::Newest => model::ProjectSort::Newest,
            ProjectSort::Progress => model::ProjectSort::Progress,
            ProjectSort::Custom => model::ProjectSort::Custom,
        }
    }
}

impl From<model::Language> for Language {
    fn from(language: model::Language) -> Self {
        match language {
            model::Language::System => Language::System,
            model::Language::De => Language::De,
            model::Language::En => Language::En,
        }
    }
}

impl From<Language> for model::Language {
    fn from(language: Language) -> Self {
        match language {
            Language::De => model::Language::De,
            Language::En => model::Language::En,
            Language::System => model::Language::System,
        }
    }
}

impl From<model::Appearance> for Appearance {
    fn from(appearance: model::Appearance) -> Self {
        match appearance {
            model::Appearance::System => Appearance::System,
            model::Appearance::Light => Appearance::Light,
            model::Appearance::Dark => Appearance::Dark,
        }
    }
}

impl From<Appearance> for model::Appearance {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => model::Appearance::Light,
            Appearance::Dark => model::Appearance::Dark,
            Appearance::System => model::Appearance::System,
        }
    }
}

impl From<model::LineColor> for LineColor {
    fn from(color: model::LineColor) -> Self {
        match color {
            model::LineColor::Magenta => LineColor::Magenta,
            model::LineColor::Red => LineColor::Red,
            model::LineColor::Orange => LineColor::Orange,
            model::LineColor::Blue => LineColor::Blue,
            model::LineColor::Green => LineColor::Green,
        }
    }
}

impl From<LineColor> for model::LineColor {
    fn from(color: LineColor) -> Self {
        match color {
            LineColor::Magenta => model::LineColor::Magenta,
            LineColor::Red => model::LineColor::Red,
            LineColor::Orange => model::LineColor::Orange,
            LineColor::Blue => model::LineColor::Blue,
            LineColor::Green => model::LineColor::Green,
        }
    }
}

#[cfg(test)]
mod tests;
