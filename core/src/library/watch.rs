//! Folder monitoring with debounce-driven re-scanning (Task 5.6).
//!
//! Spawns a dedicated thread which registers every registered library folder
//! with the OS file-system watcher.  When a change is detected, the folder
//! is re-scanned after a configurable *debounce* period, and the result is
//! streamed through an `mpsc` channel.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, RecommendedWatcher, Watcher};

use super::db::Library;
use super::scan::scan_folder;
use super::{LibraryError, ScanEvent};

/// Handle to a running library folder watcher.
///
/// Drops the underlying watcher and waits for the processing thread to exit.
pub struct LibraryWatcher {
    /// The OS file-system watcher.  The underscore prefix signals that the
    /// struct holds it only for its side-effect (keeping watches alive).
    _watcher: RecommendedWatcher,
    /// Flag set by [`Drop`] to signal the worker thread to stop.
    stop: Arc<AtomicBool>,
    /// Handle to the worker thread, taken by [`Drop`].
    join: Option<JoinHandle<()>>,
}

impl LibraryWatcher {
    /// Start watching all registered library folders.
    ///
    /// 1. Opens the library at *db_path* and collects every registered folder.
    /// 2. Creates a file-system watcher that sends events to an internal channel.
    /// 3. Registers each existing folder for recursive watching (failures are
    ///    silently ignored so that missing / unmounted folders don't prevent
    ///    watching the rest).
    /// 4. Spawns a worker thread that debounces file-system events and
    ///    dispatches [`scan_folder`] calls.
    ///
    /// Returns the watcher and a receiver for [`ScanEvent`]s.
    ///
    /// # Errors
    /// - [`LibraryError::Watch`] if the OS file-system watcher cannot be
    ///   created.
    /// - [`LibraryError::Db`] if the library database cannot be opened
    ///   (the worker thread will emit a [`ScanEvent::Failed`] event instead).
    pub fn start(
        db_path: PathBuf,
        debounce: Duration,
    ) -> Result<(Self, Receiver<ScanEvent>), LibraryError> {
        // 1. Read registered folders.
        let lib = Library::open(&db_path)?;
        let folders: Vec<PathBuf> = lib
            .folders()
            .map(|f| f.into_iter().map(|(_, p)| p).collect())?;
        drop(lib);

        // 2. Create the OS file-system watcher.
        let (fs_tx, fs_rx) = channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(fs_tx).map_err(|e| LibraryError::Watch(e.to_string()))?;

        // 3. Register each existing folder for recursive watching.
        //    Keep (registered path, canonical path) pairs: FSEvents reports
        //    canonical paths (`/private/var/...`), but scans must use the
        //    registered path so they match the rows already in the DB.
        let folders: Vec<(PathBuf, PathBuf)> = folders
            .into_iter()
            .map(|f| {
                let canon = fs::canonicalize(&f).unwrap_or_else(|_| f.clone());
                (f, canon)
            })
            .collect();

        for (f, _) in &folders {
            if f.is_dir() {
                let _ = watcher.watch(f, RecursiveMode::Recursive);
            }
        }

        // 4. Spawn the worker thread.
        let (ev_tx, ev_rx) = channel::<ScanEvent>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let join = thread::spawn(move || {
            worker(db_path, folders, fs_rx, ev_tx, debounce, stop_clone);
        });

        Ok((
            Self {
                _watcher: watcher,
                stop,
                join: Some(join),
            },
            ev_rx,
        ))
    }
}

impl Drop for LibraryWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Worker loop that processes file-system events and dispatches scans.
///
/// * `db_path` — path to the library database (re-opened per scan).
/// * `folders` — (registered path, canonical path) of each registered folder.
/// * `fs_rx` — receiver for file-system events.
/// * `ev_tx` — sender for scan progress events.
/// * `debounce` — minimum time between the last event and the scan.
/// * `stop` — flag set by [`Drop`] to request termination.
fn worker(
    db_path: PathBuf,
    folders: Vec<(PathBuf, PathBuf)>,
    fs_rx: Receiver<notify::Result<notify::Event>>,
    ev_tx: Sender<ScanEvent>,
    debounce: Duration,
    stop: Arc<AtomicBool>,
) {
    // Open the library.  If we cannot open it, report the failure and exit.
    let mut lib = match Library::open(&db_path) {
        Ok(l) => l,
        Err(e) => {
            let _ = ev_tx.send(ScanEvent::Failed {
                folder: db_path.clone(),
                error: e.to_string(),
            });
            return;
        }
    };

    // Pending: maps folder → last event time.
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        match fs_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                for p in &event.paths {
                    // Try canonicalising the event path.  On macOS, FSEvents
                    // may return `/private/var/...` instead of `/var/...`.
                    let canon_p = fs::canonicalize(p).unwrap_or_else(|_| p.clone());

                    // Find the first folder that owns this path (either via
                    // the original or canonicalised path).
                    if let Some((f, _)) = folders.iter().find(|(f, canon)| {
                        p.starts_with(f)
                            || p.starts_with(canon)
                            || canon_p.starts_with(f)
                            || canon_p.starts_with(canon)
                    }) {
                        pending.insert(f.clone(), Instant::now());
                    }
                }
            }
            Ok(Err(_)) => {
                // Watcher error — ignore and keep going.
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Check for folders that have been idle long enough to scan.
        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, t)| now.duration_since(**t) >= debounce)
            .map(|(f, _)| f.clone())
            .collect();

        for f in ready {
            pending.remove(&f);

            let _ = ev_tx.send(ScanEvent::Started { folder: f.clone() });

            match scan_folder(&mut lib, &f) {
                Ok(stats) => {
                    let _ = ev_tx.send(ScanEvent::Finished { folder: f, stats });
                }
                Err(e) => {
                    let _ = ev_tx.send(ScanEvent::Failed {
                        folder: f,
                        error: e.to_string(),
                    });
                }
            }
        }
    }
}
