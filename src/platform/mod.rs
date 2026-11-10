//! Platform services. Each backend exposes the same free functions; the rest of the app only
//! uses the re-exports below.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
pub use android::*;

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(not(target_os = "android"))]
pub use desktop::*;

/// A rendered PDF page as straight RGBA8 on a white background.
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The kinds of documents the app opens.
#[derive(Clone, Copy, Debug)]
pub enum DocumentKind {
    Pdf,
    Backup,
}

impl DocumentKind {
    /// MIME types for the Android picker. Backups are also offered as `octet-stream`, because
    /// some file providers don't recognize ZIP files.
    #[cfg_attr(
        not(target_os = "android"),
        allow(dead_code, reason = "used on Android")
    )]
    fn mime_types(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Backup => "application/zip,application/x-zip-compressed,application/octet-stream",
        }
    }

    #[cfg_attr(
        target_os = "android",
        allow(dead_code, reason = "used on the desktop")
    )]
    fn extensions(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Pdf => ("PDF", &["pdf", "PDF"]),
            Self::Backup => ("ZIP", &["zip", "ZIP"]),
        }
    }
}

#[derive(Debug)]
pub enum PickResult {
    /// The document was copied (to the app's file when opening, from it when saving).
    Picked {
        /// Display name of the chosen document.
        title: String,
    },
    Cancelled,
    Failed(String),
}

/// Upper bound for rendered pages (in pixels), to keep memory use in check when zooming.
pub const MAX_PAGE_PIXELS: u64 = 8_000_000;
