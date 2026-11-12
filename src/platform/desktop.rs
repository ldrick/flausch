use std::cell::RefCell;
use std::path::{Path, PathBuf};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

use super::{DocumentKind, PageImage, PickResult};

/// `FLAUSCH_DATA_DIR` overrides the data directory, e.g. to test without touching real data.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FLAUSCH_DATA_DIR") {
        return dir.into();
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("flausch")
}

pub fn system_language() -> Option<String> {
    sys_locale::get_locale()
}

pub fn keep_screen_on(_on: bool) {}

pub fn set_dark_theme(_dark: bool) {}

pub fn hide_text_handles() {}

pub fn set_volume_keys(_on: bool) {}

/// Opens a web page in the browser.
pub fn open_url(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(opener).arg(url).spawn() {
        eprintln!("flausch: cannot open {url}: {e}");
    }
}

/// Shows a file dialog on a background thread and copies the chosen file to `dest`.
/// `done` runs on that background thread.
pub fn pick_document(
    kind: DocumentKind,
    dest: PathBuf,
    done: impl FnOnce(PickResult) + Send + 'static,
) {
    std::thread::spawn(move || {
        let (name, extensions) = kind.extensions();
        let Some(source) = rfd::FileDialog::new()
            .add_filter(name, extensions)
            .pick_file()
        else {
            return done(PickResult::Cancelled);
        };
        done(copy(&source, &dest, &source));
    });
}

/// Shows a save dialog on a background thread, suggesting `name`, and copies `source` there.
/// `done` runs on that background thread.
pub fn save_document(
    kind: DocumentKind,
    source: PathBuf,
    name: String,
    done: impl FnOnce(PickResult) + Send + 'static,
) {
    std::thread::spawn(move || {
        let (filter, extensions) = kind.extensions();
        let Some(dest) = rfd::FileDialog::new()
            .add_filter(filter, extensions)
            .set_file_name(name)
            .save_file()
        else {
            return done(PickResult::Cancelled);
        };
        done(copy(&source, &dest, &dest));
    });
}

fn copy(source: &Path, dest: &Path, chosen: &Path) -> PickResult {
    let title = chosen
        .file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned());
    match std::fs::copy(source, dest) {
        Ok(_) => PickResult::Picked { title },
        Err(e) => PickResult::Failed(e.to_string()),
    }
}

thread_local! {
    // Parsing is the expensive part, so keep the last document per (render) thread. The key
    // includes size and modification time, because a file can be replaced under the same path.
    static DOCUMENT: RefCell<Option<(DocumentKey, Pdf)>> = const { RefCell::new(None) };
}

type DocumentKey = (PathBuf, u64, Option<std::time::SystemTime>);

fn with_document<T>(path: &Path, f: impl FnOnce(&Pdf) -> Result<T, String>) -> Result<T, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let key = (path.to_path_buf(), meta.len(), meta.modified().ok());
    DOCUMENT.with_borrow_mut(|cached| {
        if cached.as_ref().is_none_or(|(k, _)| *k != key) {
            *cached = None;
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let pdf = Pdf::new(data).map_err(|e| format!("{e:?}"))?;
            *cached = Some((key, pdf));
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
