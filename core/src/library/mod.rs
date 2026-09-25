//! Library metadata reading from audio files.

mod db;
mod error;
mod tags;

pub use db::{Library, FileState};
pub use error::LibraryError;
pub use tags::{read_meta, TrackMeta};
