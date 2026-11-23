//! Thumbnail disk cache: stable key, JPEG storage, and no-artwork marker.

use super::source::find_artwork_bytes;
use super::thumb::{THUMB_SIZE, make_thumbnail_jpeg};
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Stable file key for an album (FNV-1a 64-bit, hex).
/// std::hasher is not stable across runs, so we implement FNV-1a directly.
pub fn cache_key(album_key: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in album_key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Thumbnail cache on disk: `<dir>/<key>.jpg`, or `<dir>/<key>.none` when the album has no artwork.
pub struct ArtworkCache {
    dir: PathBuf,
}

impl ArtworkCache {
    /// Create (or reuse) the cache directory.
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn jpg_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.jpg"))
    }

    fn none_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.none"))
    }

    /// Cached thumbnail JPEG for the album, creating it from track on first use.
    /// Returns `Some(jpeg_bytes)` when found (cache or freshly generated),
    /// or `None` when no artwork exists (and a marker file prevents re-scanning).
    pub fn get_or_create(&self, album_key: &str, track: &Path) -> Option<Vec<u8>> {
        let key = cache_key(album_key);
        let jpg = self.jpg_path(&key);
        if let Ok(b) = fs::read(&jpg) {
            return Some(b);
        }
        let none = self.none_path(&key);
        if none.exists() {
            return None;
        }
        match find_artwork_bytes(track).and_then(|b| make_thumbnail_jpeg(&b, THUMB_SIZE)) {
            Some(j) => {
                let tmp = self.dir.join(format!(".{key}.jpg.tmp"));
                if fs::write(&tmp, &j).is_ok() {
                    let _ = fs::rename(&tmp, &jpg);
                }
                Some(j)
            }
            None => {
                let _ = fs::write(&none, b"");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cache_key_is_stable() {
        // Empty string → FNV offset basis.
        assert_eq!(cache_key(""), "cbf29ce484222325");
        // Single char "a".
        assert_eq!(cache_key("a"), "af63dc4c8601ec8c");
        // Different inputs produce different keys.
        assert_ne!(cache_key("a"), cache_key("b"));
    }

    #[test]
    fn creates_then_reuses() {
        let music = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let track = music.path().join("track.flac");
        fs::write(&track, b"x").unwrap();

        // Write a 400×400 blue PNG as cover art.
        let cover = music.path().join("cover.png");
        let img = image::RgbImage::from_pixel(400, 400, image::Rgb([0, 0, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        fs::write(&cover, buf).unwrap();

        let cache_dir = cache.path().join("art");
        let cache = ArtworkCache::new(cache_dir).unwrap();
        let album_key = "Album|Artist";

        // First call: generates and caches.
        let result = cache.get_or_create(album_key, &track);
        assert!(result.is_some());
        let key = cache_key(album_key);
        let jpg = cache.jpg_path(&key);
        assert!(jpg.exists(), "cache file should exist");

        // Delete cover art and call again: should return cached JPEG.
        fs::remove_file(&cover).unwrap();
        let result2 = cache.get_or_create(album_key, &track);
        assert!(result2.is_some());
    }

    #[test]
    fn no_artwork_writes_marker() {
        let music = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let track = music.path().join("track.flac");
        fs::write(&track, b"x").unwrap();

        let cache_dir = cache.path().join("art");
        let cache = ArtworkCache::new(cache_dir).unwrap();
        let album_key = "Solo|Artist";

        // No cover art → None, marker created.
        let result = cache.get_or_create(album_key, &track);
        assert!(result.is_none());
        let key = cache_key(album_key);
        let none = cache.none_path(&key);
        assert!(none.exists(), ".none marker should exist");

        // Later, place a cover.png — but the marker prevents re-exploration.
        let cover = music.path().join("cover.png");
        let img = image::RgbImage::from_pixel(400, 400, image::Rgb([0, 0, 255]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        fs::write(&cover, buf).unwrap();

        let result2 = cache.get_or_create(album_key, &track);
        assert!(result2.is_none(), "marker should prevent re-exploration");
    }
}
