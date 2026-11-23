//! Background scanner that scans all registered folders in a library.
//!
//! Spawns a dedicated thread which opens its own `Library` connection,
//! iterates every registered folder, runs `scan_folder`, and streams
//! progress through an `mpsc` channel.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

use super::ScanStats;
use super::db::Library;
use super::scan::scan_folder;

/// Event emitted by the background scanner.
#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// Scanning has started for *folder*.
    Started { folder: PathBuf },
    /// Scanning *folder* finished with these statistics.
    Finished { folder: PathBuf, stats: ScanStats },
    /// Scanning *folder* failed with an error message.
    Failed { folder: PathBuf, error: String },
    /// All registered folders have been processed.
    AllDone,
}

/// Handle to a background scan operation.
pub struct ScanHandle {
    /// Receiver for scan events.
    pub events: Receiver<ScanEvent>,
    join: JoinHandle<()>,
}

impl ScanHandle {
    /// Wait for the background scan to complete.
    pub fn wait(self) {
        let _ = self.join.join();
    }
}

/// Scan all registered folders in the library in the background.
///
/// Opens a dedicated `Library` connection in a separate thread, iterates
/// every registered folder, and streams `ScanEvent`s back through an
/// `mpsc` channel.
///
/// If the database cannot be opened, sends a single `Failed` event
/// (folder = *db_path*) followed by `AllDone`.
pub fn scan_all_in_background(db_path: PathBuf) -> ScanHandle {
    let (tx, rx) = channel();
    let join = thread::spawn(move || run(db_path, tx));
    ScanHandle { events: rx, join }
}

fn run(db_path: PathBuf, tx: Sender<ScanEvent>) {
    let mut lib = match Library::open(&db_path) {
        Ok(l) => l,
        Err(e) => {
            let _ = tx.send(ScanEvent::Failed {
                folder: db_path.clone(),
                error: e.to_string(),
            });
            let _ = tx.send(ScanEvent::AllDone);
            return;
        }
    };

    let folders = match lib.folders() {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(ScanEvent::Failed {
                folder: db_path.clone(),
                error: e.to_string(),
            });
            let _ = tx.send(ScanEvent::AllDone);
            return;
        }
    };

    for (_, folder) in folders {
        let _ = tx.send(ScanEvent::Started {
            folder: folder.clone(),
        });
        match scan_folder(&mut lib, &folder) {
            Ok(stats) => {
                let _ = tx.send(ScanEvent::Finished { folder, stats });
            }
            Err(e) => {
                let _ = tx.send(ScanEvent::Failed {
                    folder,
                    error: e.to_string(),
                });
            }
        }
    }

    let _ = tx.send(ScanEvent::AllDone);
}
