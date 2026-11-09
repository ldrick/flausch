use std::cell::RefCell;
use std::path::{Path, PathBuf};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

use super::{PageImage, PickResult};

/// `NOSTE_DATA_DIR` overrides the data directory, e.g. to test without touching real data.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NOSTE_DATA_DIR") {
        return dir.into();
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noste")
}

pub fn system_language() -> Option<String> {
    sys_locale::get_locale()
}

pub fn keep_screen_on(_on: bool) {}

pub fn set_dark_theme(_dark: bool) {}

/// Shows a file dialog on a background thread and copies the chosen file to `dest`.
/// `done` runs on that background thread.
pub fn pick_pdf(dest: PathBuf, done: impl FnOnce(PickResult) + Send + 'static) {
    std::thread::spawn(move || {
        let Some(source) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf", "PDF"])
            .pick_file()
        else {
            return done(PickResult::Cancelled);
        };
        let title = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match std::fs::copy(&source, &dest) {
            Ok(_) => done(PickResult::Picked { title }),
            Err(e) => done(PickResult::Failed(e.to_string())),
        }
    });
}

thread_local! {
    // Parsing is the expensive part, so keep the last document per (render) thread.
    static DOCUMENT: RefCell<Option<(PathBuf, Pdf)>> = const { RefCell::new(None) };
}

fn with_document<T>(path: &Path, f: impl FnOnce(&Pdf) -> Result<T, String>) -> Result<T, String> {
    DOCUMENT.with_borrow_mut(|cached| {
        if cached.as_ref().is_none_or(|(p, _)| p != path) {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let pdf = Pdf::new(data).map_err(|e| format!("{e:?}"))?;
            *cached = Some((path.to_path_buf(), pdf));
        }
        f(&cached.as_ref().unwrap().1)
    })
}

pub fn page_count(path: &Path) -> Result<u32, String> {
    with_document(path, |pdf| Ok(pdf.pages().len() as u32))
}

pub fn render_page(path: &Path, index: u32, width: u32) -> Result<PageImage, String> {
    with_document(path, |pdf| {
        let page = pdf.pages().get(index as usize).ok_or("page out of range")?;
        let (page_width, _) = page.render_dimensions();
        let scale = width as f32 / page_width.max(1.0);
        let settings = PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        };
        let pixmap = hayro::render(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &settings,
        );
        Ok(PageImage {
            width: pixmap.width().into(),
            height: pixmap.height().into(),
            // Opaque white background, so premultiplied equals straight alpha.
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    })
}
