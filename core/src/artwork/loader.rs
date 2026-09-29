//! Background album-artwork loader: disk cache + thumbnail decoding.

use super::cache::ArtworkCache;
use super::thumb::{decode_rgba, Thumb};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

type Request = (String, PathBuf);

/// Loads album thumbnails on a background thread (disk cache, creating them if needed).
pub struct ArtworkLoader {
    tx: Option<Sender<Request>>,
    rx: Receiver<(String, Option<Thumb>)>,
    handle: Option<JoinHandle<()>>,
}

impl ArtworkLoader {
    /// Starts the worker thread. `on_ready` is called after each result
    /// (e.g. to request a UI repaint).
    pub fn new(cache: ArtworkCache, on_ready: Box<dyn Fn() + Send>) -> Self {
        let (tx, req_rx) = mpsc::channel::<Request>();
        let (res_tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("artwork".into())
            .spawn(move || {
                for (key, path) in req_rx {
                    let thumb = cache
                        .get_or_create(&key, &path)
                        .and_then(|b| decode_rgba(&b));
                    if res_tx.send((key, thumb)).is_err() {
                        break;
                    }
                    on_ready();
                }
            })
            .ok();
        Self {
            tx: Some(tx),
            rx,
            handle,
        }
    }

    /// Queues a thumbnail request for an album (track = any track of that album).
    pub fn request(&self, album_key: &str, track: &Path) {
        if let Some(tx) = &self.tx {
            let _ = tx.send((album_key.to_string(), track.to_path_buf()));
        }
    }

    /// Results that are ready (non-blocking).
    pub fn try_results(&self) -> Vec<(String, Option<Thumb>)> {
        self.rx.try_iter().collect()
    }
}

impl Drop for ArtworkLoader {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a test PNG with the given dimensions and a solid green color (0, 255, 0).
    fn green_png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([0, 255, 0]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn loads_in_background() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let cache_dir = tmp.path().join("artwork");
        fs::create_dir_all(&music).unwrap();

        // with_art/track.flac + cover.png (400×400 green)
        let with_art = music.join("with_art");
        fs::create_dir_all(&with_art).unwrap();
        fs::write(with_art.join("track.flac"), b"x").unwrap();
        let cover_buf = green_png(400, 400);
        fs::write(with_art.join("cover.png"), cover_buf).unwrap();

        // no_art/track.flac (no cover)
        let no_art = music.join("no_art");
        fs::create_dir_all(&no_art).unwrap();
        fs::write(no_art.join("track.flac"), b"x").unwrap();

        let cache = ArtworkCache::new(cache_dir).unwrap();
        let loader = ArtworkLoader::new(cache, Box::new(|| {}));

        loader.request("A", with_art.join("track.flac").as_path());
        loader.request("B", no_art.join("track.flac").as_path());

        // Wait up to 5 s for both results (poll every 20 ms).
        let mut results: Vec<(String, Option<Thumb>)> = Vec::new();
        for _ in 0..250 {
            results.extend(loader.try_results());
            if results.len() >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert_eq!(results.len(), 2, "expected 2 results within timeout");

        // Sort by key for deterministic assertion.
        results.sort_by_key(|(k, _)| k.clone());

        // Key "A" should have Some thumb with 320×320.
        let (_, thumb_a) = &results[0];
        assert!(thumb_a.is_some(), "album A should have artwork");
        let t = thumb_a.as_ref().unwrap();
        assert_eq!(t.width, 320);
        assert_eq!(t.height, 320);

        // Key "B" should be None (no cover art).
        let (_, thumb_b) = &results[1];
        assert!(thumb_b.is_none(), "album B should have no artwork");
    }
}
