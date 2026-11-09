//! Renders PDF pages on background threads: pages for the viewer (only the newest pending
//! request is rendered) and cover thumbnails for the project list.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::platform;

pub struct Request {
    pub serial: u64,
    pub path: PathBuf,
    pub page: u32,
    pub width: u32,
}

pub struct Rendered {
    pub serial: u64,
    pub page: u32,
    pub page_count: u32,
    pub width: u32,
    pub result: Result<SharedPixelBuffer<Rgba8Pixel>, String>,
}

pub struct Renderer {
    sender: mpsc::Sender<Request>,
}

impl Renderer {
    /// `on_done` runs on the render thread.
    pub fn new(on_done: impl Fn(Rendered) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel::<Request>();
        std::thread::Builder::new()
            .name("pdf-render".into())
            .spawn(move || {
                while let Ok(mut request) = receiver.recv() {
                    while let Ok(newer) = receiver.try_recv() {
                        request = newer;
                    }
                    on_done(render(request));
                }
            })
            .expect("cannot start the PDF render thread");
        Self { sender }
    }

    pub fn request(&self, request: Request) {
        let _ = self.sender.send(request);
    }
}

fn render(request: Request) -> Rendered {
    let mut page_count = 0;
    let result = platform::page_count(&request.path).and_then(|count| {
        page_count = count;
        if request.page >= count {
            return Err(format!(
                "page {} out of range ({count} pages)",
                request.page
            ));
        }
        let image = platform::render_page(&request.path, request.page, request.width)?;
        Ok(SharedPixelBuffer::clone_from_slice(
            &image.rgba,
            image.width,
            image.height,
        ))
    });
    Rendered {
        serial: request.serial,
        page: request.page,
        page_count,
        width: request.width,
        result,
    }
}

/// Width of cover thumbnails in pixels.
const COVER_WIDTH: u32 = 288;

/// Renders the first page of PDFs to PNG thumbnails on a background thread, one after another.
pub struct Covers {
    sender: mpsc::Sender<(PathBuf, PathBuf)>,
}

impl Covers {
    /// `on_done` runs on the cover thread after each cover was written.
    pub fn new(on_done: impl Fn() + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel::<(PathBuf, PathBuf)>();
        std::thread::Builder::new()
            .name("pdf-cover".into())
            .spawn(move || {
                while let Ok((pdf, cover)) = receiver.recv() {
                    match write_cover(&pdf, &cover) {
                        Ok(()) => on_done(),
                        Err(e) => {
                            eprintln!("nøste: cannot create a cover for {}: {e}", pdf.display())
                        }
                    }
                }
            })
            .expect("cannot start the PDF cover thread");
        Self { sender }
    }

    pub fn request(&self, pdf: PathBuf, cover: PathBuf) {
        let _ = self.sender.send((pdf, cover));
    }
}

fn write_cover(pdf: &Path, cover: &Path) -> Result<(), String> {
    let image = platform::render_page(pdf, 0, COVER_WIDTH)?;
    let tmp = cover.with_extension("png.tmp");
    let file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&image.rgba))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, cover).map_err(|e| e.to_string())
}

/// Render width in physical pixels for a page shown `view_width` pixels wide at `zoom`,
/// capped so a page never exceeds [`platform::MAX_PAGE_PIXELS`].
pub fn render_width(view_width: f32, zoom: f32, aspect: f32) -> u32 {
    let wanted = (view_width * zoom).round().max(1.0);
    let cap = (platform::MAX_PAGE_PIXELS as f32 / aspect.max(0.1))
        .sqrt()
        .floor();
    wanted.min(cap) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_width_follows_zoom_and_is_capped() {
        assert_eq!(render_width(1000.0, 1.0, 1.414), 1000);
        assert_eq!(render_width(1000.0, 2.0, 1.414), 2000);
        let capped = render_width(1080.0, 4.0, 1.414);
        assert!(capped < 4320);
        assert!((capped as u64).pow(2) as f32 * 1.414 <= platform::MAX_PAGE_PIXELS as f32);
    }
}
