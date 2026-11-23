//! Library metadata reading from audio files.

mod db;
mod error;
mod query;
mod scan;
mod scanner;
mod tags;
mod watch;

pub use db::{FileState, Library};
pub use error::LibraryError;
pub use query::{AlbumRow, TrackFilter, TrackRow};
pub use scan::{ScanStats, is_audio_path, scan_folder};
pub use scanner::{ScanEvent, ScanHandle, scan_all_in_background};
pub use tags::{TrackMeta, read_meta};
pub use watch::LibraryWatcher;
