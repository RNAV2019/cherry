use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

/// Largest texture dimension uploaded to the GPU. Wallpapers are downscaled to
/// this before becoming textures so a 4K image doesn't stall the first paint.
const MAX_DIM: u32 = 1600;

pub struct LoadedImage {
    pub path: PathBuf,
    /// Downscaled texture size in pixels.
    pub size: [usize; 2],
    /// Original image dimensions, for the preview caption.
    pub original: (u32, u32),
    pub rgba: Vec<u8>,
}

/// Loads images on a background thread and hands them back via `poll`.
pub struct Loader {
    tx: Sender<LoadedImage>,
    rx: Receiver<LoadedImage>,
}

impl Loader {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Loader { tx, rx }
    }

    /// Loads `paths` sequentially on one background thread, requesting a
    /// repaint as each image completes.
    pub fn request_all(&self, paths: Vec<PathBuf>, ctx: egui::Context) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for path in paths {
                let Ok(img) = image::open(&path) else { continue };
                let original = (img.width(), img.height());
                let img = if img.width() > MAX_DIM || img.height() > MAX_DIM {
                    img.resize(MAX_DIM, MAX_DIM, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                if tx
                    .send(LoadedImage { path, size, original, rgba: rgba.into_raw() })
                    .is_err()
                {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }

    pub fn poll(&self) -> Option<LoadedImage> {
        self.rx.try_recv().ok()
    }
}
