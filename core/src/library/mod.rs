//! Library metadata reading from audio files.

mod db;
mod error;
mod scan;
mod scanner;
mod tags;

pub use db::{Library, FileState};
pub use error::LibraryError;
pub use scan::{is_audio_path, scan_folder, ScanStats};
pub use scanner::{ScanEvent, ScanHandle, scan_all_in_background};
pub use tags::{read_meta, TrackMeta};
