//! Tests for the background scanner (Task 5.5).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mmp_core::library::{ScanEvent, ScanStats, scan_all_in_background};

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
fn setup() -> Option<(tempfile::TempDir, PathBuf)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testdata/lib");
    if !src.is_dir() {
        eprintln!("testdata/lib not found — skipping scanner tests");
        return None;
    }
    let tmp = tempfile::tempdir().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    copy_recursive(&src, &root).expect("copy testdata/lib into tempdir");
    Some((tmp, root))
}

/// Full scan cycle through the background scanner.
#[test]
fn background_scan_completes() {
    let (tmp, root) = match setup() {
        Some(d) => d,
        None => return,
    };

    // Create a separate temp directory for the DB file.
    let db_dir = tempfile::tempdir().expect("create db tempdir");
    let db_path = db_dir.path().join("lib.db");

    // Open the DB, add the root folder, then drop it so the scanner can open
    // a fresh connection.
    {
        let lib = mmp_core::library::Library::open(&db_path).expect("open library");
        lib.add_folder(&root).expect("add folder");
        // lib dropped here — DB file persists on disk.
    }

    // Launch the background scanner.
    let handle = scan_all_in_background(db_path.clone());

    // Collect events until AllDone (timeout = 30 s).
    let mut started = 0u32;
    let mut finished = 0u32;
    let mut failed = 0u32;
    let mut last_stats: Option<ScanStats> = None;

    loop {
        let event = match handle.events.recv_timeout(Duration::from_secs(30)) {
            Ok(e) => e,
            Err(_) => panic!("scanner timed out before AllDone"),
        };
        match &event {
            ScanEvent::Started { .. } => started += 1,
            ScanEvent::Finished { stats, .. } => {
                finished += 1;
                last_stats = Some(stats.clone());
            }
            ScanEvent::Failed { .. } => failed += 1,
            ScanEvent::AllDone => break,
        }
    }

    assert_eq!(started, 1, "exactly one Started event");
    assert_eq!(finished, 1, "exactly one Finished event");
    assert_eq!(failed, 0, "no Failed events");

    let stats = last_stats.expect("should have received Finished");
    assert_eq!(stats.added, 4, "four tracks added");
    assert_eq!(stats.errors, 1, "one error (broken.mp3)");

    // Wait for the thread to fully exit.
    handle.wait();

    // Re-open the database and verify track count.
    let lib = mmp_core::library::Library::open(&db_path).expect("reopen library");
    assert_eq!(lib.track_count().expect("count"), 4, "four tracks in DB");

    // Drop db_dir so the DB file is cleaned up.
    drop(db_dir);
    drop(tmp);
}
