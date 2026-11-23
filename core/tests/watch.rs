//! Tests for the folder watcher (Task 5.6).

use std::fs;
use std::path::Path;
use std::time::Duration;

use mmp_core::library::LibraryWatcher;

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
fn setup() -> Option<(tempfile::TempDir, std::path::PathBuf)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/lib");
    if !src.is_dir() {
        eprintln!("testdata/lib not found — skipping watcher tests");
        return None;
    }
    let tmp = tempfile::tempdir().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    copy_recursive(&src, &root).expect("copy testdata/lib into tempdir");
    Some((tmp, root))
}

/// Watcher detects a new file copied into a watched folder.
#[test]
fn watcher_detects_new_file() {
    let (tmp, root) = match setup() {
        Some(d) => d,
        None => return,
    };

    // Create a separate temp directory for the DB file.
    let db_dir = tempfile::tempdir().expect("create db tempdir");
    let db_path = db_dir.path().join("lib.db");

    // Register the root folder and do an initial scan.
    {
        let mut lib = mmp_core::library::Library::open(&db_path).expect("open library");
        lib.add_folder(&root).expect("add folder");
        let _ = mmp_core::library::scan_folder(&mut lib, &root);
    }
    // lib dropped here — DB file persists on disk.

    // Re-open the library to force a WAL checkpoint so the initial 5 tracks
    // become visible to the watcher's connection (SQLite WAL mode issue).
    {
        let _lib = mmp_core::library::Library::open(&db_path).expect("reopen library");
        assert_eq!(
            _lib.track_count().expect("count"),
            4,
            "initial scan should have 4 tracks (broken.mp3 is an error)"
        );
    }

    // Start the watcher with a 300 ms debounce.
    let (watcher, events) =
        LibraryWatcher::start(db_path.clone(), Duration::from_millis(300)).expect("start watcher");

    // Wait for the watcher to initialise its OS watches.
    std::thread::sleep(Duration::from_millis(500));

    // Copy a new FLAC file into the watched folder.
    let new_flac = root.join("AlbumB").join("new.flac");
    fs::copy(root.join("notag.flac"), &new_flac).expect("copy notag.flac as new.flac");

    // Collect events until we receive a Finished event (skip Started).
    let stats = loop {
        let event = match events.recv_timeout(Duration::from_secs(10)) {
            Ok(e) => e,
            Err(_) => panic!("watcher timed out before Finished event"),
        };
        match event {
            mmp_core::library::ScanEvent::Finished { stats, .. } => break stats,
            mmp_core::library::ScanEvent::Failed { error, .. } => {
                panic!("watcher reported a Failed event: {error}");
            }
            mmp_core::library::ScanEvent::Started { .. }
            | mmp_core::library::ScanEvent::AllDone => {}
        }
    };
    assert_eq!(stats.added, 1, "one track should be added");

    // Stop the watcher before re-opening the database.
    drop(watcher);

    // Re-open the database and verify the track count is now 5.
    let lib = mmp_core::library::Library::open(&db_path).expect("reopen library");
    assert_eq!(
        lib.track_count().expect("count"),
        5,
        "five tracks in DB after watcher scan"
    );
    drop(lib);
    drop(db_dir);

    // Clean up the temp directory.
    drop(tmp);
}
