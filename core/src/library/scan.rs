//! Folder scanning with incremental diff detection.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use super::FileState;
use super::LibraryError;
use super::db::Library;
use super::tags::read_meta;

/// Statistics returned by a single scan.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanStats {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    pub unchanged: u32,
    pub errors: u32,
    pub folder_missing: bool,
}

impl ScanStats {
    /// Whether the scan changed the library (anything added, updated or removed).
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}

/// Audio extensions recognised by the library scanner (lowercase).
const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "m4a", "mp3", "aac", "wav", "aif", "aiff", "ogg", "oga",
];

/// Returns `true` when *path* has an audio extension (case-insensitive).
pub fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read file-level metadata (mtime, size) for change detection.
///
/// Returns `None` when the file cannot be stat'd.
fn file_state(path: &Path) -> Option<FileState> {
    let md = fs::metadata(path).ok()?;
    let mtime_ns = md
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos() as i64;
    let size = md.len() as i64;
    Some(FileState { mtime_ns, size })
}

/// Scan *folder* and incrementally update the library.
///
/// If the folder does not exist or is not a directory, returns
/// `ScanStats { folder_missing: true, ..Default::default() }` without
/// touching the database.
///
/// Files that fail tag reading are counted as `errors` and left in the
/// database if they already exist.  New files that fail tag reading are
/// simply skipped.
pub fn scan_folder(lib: &mut Library, folder: &Path) -> Result<ScanStats, LibraryError> {
    if !folder.is_dir() {
        return Ok(ScanStats {
            folder_missing: true,
            ..Default::default()
        });
    }

    let folder_id = lib.add_folder(folder)?;
    let known = lib.file_states(folder_id)?;

    lib.transaction(|tx| {
        let mut stats = ScanStats::default();
        let mut seen = HashSet::new();

        for entry in WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_audio_path(path) {
                continue;
            }

            let Some(state) = file_state(path) else {
                stats.errors += 1;
                continue;
            };

            seen.insert(path.to_path_buf());

            match known.get(path) {
                Some(old) if *old == state => {
                    stats.unchanged += 1;
                }
                prev => match read_meta(path) {
                    Ok(meta) => {
                        tx.upsert_track(folder_id, path, state, &meta)?;
                        if prev.is_some() {
                            stats.updated += 1;
                        } else {
                            stats.added += 1;
                        }
                    }
                    Err(_) => {
                        stats.errors += 1;
                    }
                },
            }
        }

        // Remove tracks that no longer exist on disk.
        for old_path in known.keys() {
            if !seen.contains(old_path) {
                tx.remove_track(old_path)?;
                stats.removed += 1;
            }
        }

        Ok(stats)
    })
}
