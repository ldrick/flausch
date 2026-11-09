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

#[derive(Debug)]
pub enum PickResult {
    /// The PDF was copied to the requested destination.
    Picked {
        title: String,
    },
    Cancelled,
    Failed(String),
}

/// Upper bound for rendered pages (in pixels), to keep memory use in check when zooming.
pub const MAX_PAGE_PIXELS: u64 = 8_000_000;
