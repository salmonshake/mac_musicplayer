//! Read metadata from audio files using lofty.

use lofty::prelude::*;
use lofty::read_from_path;
use std::path::Path;

use super::error::LibraryError;

/// Metadata extracted from an audio file's tags and properties.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u32>,
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}

impl TrackMeta {
    /// Returns `album_artist` if set, otherwise falls back to `artist`.
    pub fn effective_album_artist(&self) -> &str {
        if !self.album_artist.is_empty() {
            &self.album_artist
        } else {
            &self.artist
        }
    }
}

/// Read metadata from an audio file.
///
/// If the file cannot be parsed, returns `Err(LibraryError::Tag(..))`.
/// A file with no tags is still Ok — `title` is set to the file stem.
pub fn read_meta(path: &Path) -> Result<TrackMeta, LibraryError> {
    let tagged = read_from_path(path).map_err(|e| LibraryError::Tag(e.to_string()))?;
    let props = tagged.properties();

    let duration_ms = props.duration().as_millis() as u64;
    let sample_rate = props.sample_rate();
    let bit_depth = props.bit_depth();
    let channels = props.channels();
    let bitrate_kbps = props.audio_bitrate();

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let mut meta = TrackMeta {
        duration_ms,
        sample_rate,
        bit_depth,
        channels,
        bitrate_kbps,
        ..Default::default()
    };

    if let Some(t) = tag {
        meta.title = t.title().map(|s| s.to_string()).unwrap_or_default();
        meta.artist = t.artist().map(|s| s.to_string()).unwrap_or_default();
        meta.album = t.album().map(|s| s.to_string()).unwrap_or_default();
        meta.genre = t.genre().map(|s| s.to_string()).unwrap_or_default();
        meta.album_artist = t
            .get_string(ItemKey::AlbumArtist)
            .unwrap_or_default()
            .to_string();
        meta.track_no = t.track();
        meta.disc_no = t.disk();
        meta.year = t.date().map(|d| u32::from(d.year));
    }

    // Fallback: if title is empty, use the file stem (without extension).
    if meta.title.trim().is_empty() {
        meta.title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }

    Ok(meta)
}
