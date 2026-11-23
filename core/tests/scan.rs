//! Tests for library folder scanning (Task 5.4).

use std::fs;
use std::path::Path;
use std::time::Duration;

use mmp_core::library::{Library, is_audio_path, scan_folder};

/// Copies *src* directory recursively into *dst*.
fn copy_recursive(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use walkdir::WalkDir;
    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let relative = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| format!("strip_prefix: {e}"))?;
        let target = dst.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Set up a temporary library directory from testdata/lib.
///
/// Returns `None` when the source directory does not exist.
fn setup() -> Option<tempfile::TempDir> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/lib");
    if !src.is_dir() {
        eprintln!("testdata/lib not found — skipping scan tests");
        return None;
    }
    let tmp = tempfile::tempdir().expect("create tempdir");
    copy_recursive(&src, tmp.path()).expect("copy testdata/lib into tempdir");
    Some(tmp)
}

/// Full diff cycle: add → re-scan (unchanged) → update one file → remove one file.
#[test]
fn full_diff_cycle() {
    let tmp = match setup() {
        Some(d) => d,
        None => return,
    };
    let root = tmp.path();

    let mut lib = Library::open_in_memory().expect("open in-memory library");

    // 1st scan: should add 4 tracks, 1 error (broken.mp3).
    let s1 = scan_folder(&mut lib, root).expect("scan 1");
    assert_eq!(s1.added, 4);
    assert_eq!(s1.errors, 1);
    assert_eq!(s1.removed, 0);
    assert_eq!(s1.unchanged, 0);
    assert!(!s1.folder_missing);
    assert_eq!(lib.track_count().expect("count"), 4);

    // 2nd scan: everything unchanged.
    let s2 = scan_folder(&mut lib, root).expect("scan 2");
    assert_eq!(s2.unchanged, 4);
    assert_eq!(s2.added, 0);
    assert_eq!(s2.updated, 0);
    assert_eq!(s2.removed, 0);
    assert_eq!(lib.track_count().expect("count"), 4);

    // 3rd scan: overwrite 01.flac with notag.flac content → updated=1.
    // Sleep briefly so mtime advances (same-second mtime would look unchanged).
    std::thread::sleep(Duration::from_millis(20));
    fs::copy(root.join("notag.flac"), root.join("AlbumA").join("01.flac"))
        .expect("overwrite 01.flac with notag.flac");
    let s3 = scan_folder(&mut lib, root).expect("scan 3");
    assert_eq!(s3.updated, 1);
    assert_eq!(s3.added, 0);
    assert_eq!(s3.removed, 0);
    assert_eq!(lib.track_count().expect("count"), 4);

    // 4th scan: delete track.ogg → removed=1, track_count=3.
    let ogg_path = root.join("AlbumB").join("track.ogg");
    assert!(
        ogg_path.exists(),
        "track.ogg should exist for deletion test"
    );
    fs::remove_file(&ogg_path).expect("delete track.ogg");
    let s4 = scan_folder(&mut lib, root).expect("scan 4");
    assert_eq!(s4.removed, 1);
    assert_eq!(s4.added, 0);
    assert_eq!(s4.updated, 0);
    assert_eq!(lib.track_count().expect("count"), 3);
}

/// Scanning a missing folder does not remove existing tracks.
#[test]
fn missing_folder_keeps_tracks() {
    let tmp = match setup() {
        Some(d) => d,
        None => return,
    };
    let root = tmp.path().to_path_buf();

    let mut lib = Library::open_in_memory().expect("open in-memory library");

    // First scan: register 4 tracks.
    let s1 = scan_folder(&mut lib, &root).expect("scan 1");
    assert_eq!(s1.added, 4);
    assert_eq!(lib.track_count().expect("count"), 4);

    // Scan a non-existent folder → folder_missing=true, tracks unchanged.
    let nope = root.join("nope");
    let s2 = scan_folder(&mut lib, &nope).expect("scan missing");
    assert!(s2.folder_missing);
    assert_eq!(s2.added, 0);
    assert_eq!(s2.removed, 0);
    assert_eq!(lib.track_count().expect("count"), 4);

    // Drop the tempdir (files gone), then scan the original root again.
    drop(tmp);
    let s3 = scan_folder(&mut lib, &root).expect("scan after drop");
    assert!(s3.folder_missing);
    assert_eq!(s3.added, 0);
    assert_eq!(s3.removed, 0);
    assert_eq!(lib.track_count().expect("count"), 4);
}

/// is_audio_path recognises every extension in the spec.
#[test]
fn is_audio_path_recognises_all() {
    let exts = [
        "flac", "m4a", "mp3", "aac", "wav", "aif", "aiff", "ogg", "oga",
    ];
    for ext in &exts {
        let path = Path::new("song").with_extension(ext);
        assert!(is_audio_path(&path), "{ext} should be audio");
    }
    // Case insensitive.
    assert!(is_audio_path(Path::new("song.FLAC")));
    assert!(is_audio_path(Path::new("song.Mp3")));
    // Non-audio extensions.
    assert!(!is_audio_path(Path::new("song.txt")));
    assert!(!is_audio_path(Path::new("song")));
}
