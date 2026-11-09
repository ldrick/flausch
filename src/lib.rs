mod model;
mod pdf;
mod platform;
mod storage;

use std::cell::RefCell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Image, ModelRc, VecModel};

use model::{Library, Pdf, Settings};
use platform::PickResult;
use storage::Storage;

slint::include_modules!();

pub fn run() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let storage = Storage::new(platform::data_dir()).map_err(|e| {
        slint::PlatformError::Other(format!("cannot create the data directory: {e}"))
    })?;

    let renderer = pdf::Renderer::new(|rendered| {
        let _ = slint::invoke_from_event_loop(move || {
            with(|c| c.pdf_rendered(rendered));
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
        save_timer: slint::Timer::default(),
        screen_on: None,
        last_back: None,
    };
    CONTROLLER.with_borrow_mut(|c| *c = Some(controller));
    with(|c| {
        c.apply_settings();
        c.refresh();
    });
    connect(&app);
    platform::set_dark_theme(app.global::<Theme>().get_dark());
    app.run()?;
    with(|c| c.flush());
    Ok(())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    platform::init(app.clone());
    slint::android::init(app).unwrap();
    run().unwrap();
}

thread_local! {
    // Lives on the UI thread. Callbacks from other threads go through `invoke_from_event_loop`.
    static CONTROLLER: RefCell<Option<Controller>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Controller) -> R) -> Option<R> {
    CONTROLLER.with_borrow_mut(|c| c.as_mut().map(f))
}

fn connect(app: &App) {
    let store = app.global::<Store>();
    store.on_back(|| with(|c| c.back()).unwrap_or(false));
    store.on_open_settings(|| {
        with(|c| c.navigate(Screen::Settings));
    });
    store.on_open_project(|id| {
        with(|c| c.open_project(id as u64));
    });
    store.on_create_project(|name, goal| {
        with(|c| c.create_project(&name, &goal));
    });
    store.on_edit_project(|name, goal| {
        with(|c| c.edit_project(&name, &goal));
    });
    store.on_delete_project(|| {
        with(|c| c.delete_project());
    });
    store.on_increment(|| {
        with(|c| c.count(Library::increment));
    });
    store.on_decrement(|| {
        with(|c| c.count(Library::decrement));
    });
    store.on_reset(|| {
        with(|c| c.count(|lib, id, now| lib.reset(id, now).then_some(0)));
    });
    store.on_pick_pdf(|| {
        with(|c| c.pick_pdf());
    });
    store.on_remove_pdf(|| {
        with(|c| c.remove_pdf());
    });
    store.on_open_pdf(|| {
        with(|c| c.open_pdf());
    });
    store.on_pdf_view_changed(|page, zoom, scroll_x, scroll_y, width| {
        with(|c| c.pdf_view_changed(page, zoom, scroll_x, scroll_y, width));
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
    store.on_set_keep_screen_on(|on| {
        with(|c| c.update_settings(|s| s.keep_screen_on = on));
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
    save_timer: slint::Timer,
    screen_on: Option<bool>,
    last_back: Option<Instant>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Controller {
    fn app(&self) -> App {
        self.app.upgrade().expect("the app window is gone")
    }

    fn save(&mut self) {
        self.save_timer.stop();
        if let Err(e) = self.storage.save_library(&self.library) {
            eprintln!("nøste: cannot save projects: {e}");
        }
    }

    /// Saves after a short pause, for frequent changes like scrolling.
    fn save_later(&mut self) {
        self.save_timer
            .start(slint::TimerMode::SingleShot, Duration::from_secs(1), || {
                with(|c| c.save());
            });
    }

    fn flush(&mut self) {
        if self.save_timer.running() {
            self.save();
        }
    }

    fn refresh(&self) {
        let app = self.app();
        let store = app.global::<Store>();
        let items: Vec<ProjectItem> = self
            .library
            .recent()
            .into_iter()
            .map(project_item)
            .collect();
        store.set_projects(ModelRc::new(VecModel::from(items)));
        if let Some(project) = self.current.and_then(|id| self.library.get(id)) {
            store.set_current(project_item(project));
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
            store.set_dialog(DialogKind::None);
            self.last_back = Some(now);
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
        let on =
            self.settings.keep_screen_on && matches!(self.screen(), Screen::Project | Screen::Pdf);
        if self.screen_on != Some(on) {
            self.screen_on = Some(on);
            platform::keep_screen_on(on);
        }
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

    fn create_project(&mut self, name: &str, goal: &str) {
        if name.trim().is_empty() {
            return;
        }
        let id = self.library.create(name, parse_goal(goal), now());
        self.save();
        self.open_project(id);
    }

    fn edit_project(&mut self, name: &str, goal: &str) {
        let Some(id) = self.current else { return };
        if name.trim().is_empty() {
            return;
        }
        self.library.edit(id, name, parse_goal(goal), now());
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

    fn count(&mut self, op: impl FnOnce(&mut Library, u64, u64) -> Option<u32>) {
        let Some(id) = self.current else { return };
        if op(&mut self.library, id, now()).is_some() {
            self.save();
            self.refresh();
        }
    }

    // PDF patterns

    fn pick_pdf(&mut self) {
        let Some(id) = self.current else { return };
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let file = self.storage.new_pdf_file(id, nanos);
        let dest = self.storage.pdf_path(&file);
        platform::pick_pdf(dest, move |result| {
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| c.pdf_picked(id, file, result));
            });
        });
    }

    fn pdf_picked(&mut self, id: u64, file: String, result: PickResult) {
        let title = match result {
            PickResult::Picked { title } => title,
            PickResult::Cancelled => {
                self.storage.remove_pdf(&file);
                return;
            }
            PickResult::Failed(e) => {
                eprintln!("nøste: PDF import failed: {e}");
                self.storage.remove_pdf(&file);
                return self.notify(Notice::PdfImportFailed);
            }
        };
        if let Err(e) = platform::page_count(&self.storage.pdf_path(&file)).and_then(|n| match n {
            0 => Err("no pages".into()),
            n => Ok(n),
        }) {
            eprintln!("nøste: {title} is not a usable PDF: {e}");
            self.storage.remove_pdf(&file);
            return self.notify(Notice::PdfInvalid);
        }
        let Some(project) = self.library.touch(id, now()) else {
            self.storage.remove_pdf(&file);
            return;
        };
        if let Some(old) = project.pdf.replace(Pdf::new(file, title)) {
            self.storage.remove_pdf(&old.file);
        }
        self.save();
        self.refresh();
    }

    fn remove_pdf(&mut self) {
        let Some(id) = self.current else { return };
        if let Some(old) = self.library.touch(id, now()).and_then(|p| p.pdf.take()) {
            self.storage.remove_pdf(&old.file);
            self.save();
            self.refresh();
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
                eprintln!("nøste: cannot render page {}: {e}", rendered.page);
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

    // Settings

    fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings);
        if let Err(e) = self.storage.save_settings(&self.settings) {
            eprintln!("nøste: cannot save settings: {e}");
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
        let language = self
            .settings
            .language
            .translation(platform::system_language().as_deref());
        if let Err(e) = slint::select_bundled_translation(language) {
            eprintln!("nøste: cannot select language {language}: {e}");
        }
        self.update_keep_screen_on();
    }
}

fn parse_goal(goal: &str) -> Option<u32> {
    goal.trim().parse().ok()
}

fn project_item(project: &model::Project) -> ProjectItem {
    ProjectItem {
        id: project.id as i32,
        name: project.name.as_str().into(),
        row: project.row as i32,
        goal: project.row_goal.unwrap_or(0) as i32,
        has_pdf: project.pdf.is_some(),
        pdf_title: project
            .pdf
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or_default()
            .into(),
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
            _ => model::Language::System,
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
            _ => model::Appearance::System,
        }
    }
}
