//! Library metadata reading from audio files.

mod error;
mod tags;

pub use error::LibraryError;
pub use tags::{read_meta, TrackMeta};
