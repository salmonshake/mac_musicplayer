use lofty::picture::PictureType;
use lofty::prelude::*;
use std::path::{Path, PathBuf};

/// Candidate file stems for folder artwork (compared case-insensitively).
const FOLDER_NAMES: &[&str] = &["cover", "folder", "front", "album"];
const FOLDER_EXTS: &[&str] = &["jpg", "jpeg", "png"];

/// Embedded picture bytes: front cover preferred, else the first picture.
pub fn embedded_picture(path: &Path) -> Option<Vec<u8>> {
    let tagged = lofty::read_from_path(path).ok()?;
    let mut first: Option<&[u8]> = None;
    for tag in tagged.tags() {
        for pic in tag.pictures() {
            if pic.pic_type() == PictureType::CoverFront {
                return Some(pic.data().to_vec());
            }
            if first.is_none() {
                first = Some(pic.data());
            }
        }
    }
    first.map(|d| d.to_vec())
}

/// An image file such as `cover.jpg` / `Folder.PNG` in *dir*.
/// Prefers entries whose stem matches `FOLDER_NAMES` in order.
pub fn folder_image(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Some(stem) = p.file_stem() else { continue };
        let stem = stem.to_string_lossy().to_lowercase();
        let Some(ext) = p.extension() else { continue };
        let ext = ext.to_string_lossy().to_lowercase();
        if FOLDER_NAMES.contains(&stem.as_str()) && FOLDER_EXTS.contains(&ext.as_str()) {
            candidates.push(p);
        }
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| {
        let a_name = a.file_stem().map(|s| s.to_string_lossy().to_lowercase());
        let b_name = b.file_stem().map(|s| s.to_string_lossy().to_lowercase());
        let a_pos = a_name.as_deref().and_then(|s| FOLDER_NAMES.iter().position(|&n| n == s));
        let b_pos = b_name.as_deref().and_then(|s| FOLDER_NAMES.iter().position(|&n| n == s));
        a_pos.cmp(&b_pos).then_with(|| a_name.as_deref().cmp(&b_name.as_deref()))
    });
    candidates.into_iter().next()
}

/// Artwork bytes for a track: embedded first, then a folder image.
pub fn find_artwork_bytes(track: &Path) -> Option<Vec<u8>> {
    if let Some(b) = embedded_picture(track) {
        return Some(b);
    }
    let dir = track.parent()?;
    let p = folder_image(dir)?;
    std::fs::read(p).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn folder_image_finds_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("notes.txt"), "hello").unwrap();
        fs::write(tmp.path().join("Cover.JPG"), b"IMG").unwrap();
        assert_eq!(folder_image(tmp.path()), Some(tmp.path().join("Cover.JPG")));
    }

    #[test]
    fn folder_image_prefers_cover() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("folder.png"), b"F").unwrap();
        fs::write(tmp.path().join("cover.jpg"), b"C").unwrap();
        assert_eq!(
            folder_image(tmp.path()),
            Some(tmp.path().join("cover.jpg"))
        );
    }

    #[test]
    fn folder_image_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("back.jpg"), b"B").unwrap();
        assert!(folder_image(tmp.path()).is_none());
    }

    #[test]
    fn find_artwork_bytes_uses_folder_image() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("track.flac"), b"x").unwrap();
        fs::write(tmp.path().join("cover.jpg"), b"IMG").unwrap();
        let result = find_artwork_bytes(tmp.path().join("track.flac").as_path());
        assert_eq!(result, Some(b"IMG".to_vec()));
    }

    #[test]
    fn folder_image_skips_files_without_extension() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".DS_Store"), b"DATA").unwrap();
        fs::write(tmp.path().join("README"), b"readme").unwrap();
        fs::write(tmp.path().join("cover.png"), b"IMG").unwrap();
        assert_eq!(
            folder_image(tmp.path()),
            Some(tmp.path().join("cover.png"))
        );
    }

    #[test]
    fn embedded_picture_missing_file() {
        assert!(embedded_picture(Path::new("/nonexistent/path.flac")).is_none());
    }
}
