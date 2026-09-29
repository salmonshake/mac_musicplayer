//! Query API for the library.
//!
//! Provides `list_tracks`, `list_albums`, `list_artists`, and `list_genres`
//! backed by the SQLite database.  Dynamic WHERE fragments are assembled
//! from `TrackFilter` and passed through `rusqlite::params_from_iter`.

use std::path::PathBuf;

use rusqlite::{params_from_iter, OptionalExtension};

use super::LibraryError;
use crate::library::Library;

/// Column list for SELECT queries on the tracks table.
pub(crate) const TRACK_COLUMNS: &str =
    "id, path, title, artist, album, album_artist, album_key, genre, \
     track_no, disc_no, year, duration_ms, sample_rate, bit_depth, bitrate_kbps";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single track row returned by `list_tracks`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub album_key: String,
    pub genre: String,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u32>,
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}

/// A single album row returned by `list_albums`.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumRow {
    pub album_key: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u32>,
    pub track_count: u32,
    pub duration_ms: u64,
}

/// Filters used by `list_tracks`.
///
/// All fields are optional.  When present they narrow the result set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackFilter {
    pub album_key: Option<String>,
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub search: Option<String>,
}

/// Build a `TrackRow` from a SQLite row.
///
/// Column indices (0-14) match `TRACK_COLUMNS`.  Integral columns are read
/// as `i64` first and then narrowed to `u32` / `u64` / `u8` with
/// `and_then` / `as` casts so that `unwrap` / `expect` never appear in
/// product code.
fn track_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRow> {
    let id: i64 = row.get(0)?;
    let path: String = row.get(1)?;
    let title: String = row.get(2)?;
    let artist: String = row.get(3)?;
    let album: String = row.get(4)?;
    let album_artist: String = row.get(5)?;
    let album_key: String = row.get(6)?;
    let genre: String = row.get(7)?;
    let track_no: Option<i64> = row.get(8)?;
    let disc_no: Option<i64> = row.get(9)?;
    let year: Option<i64> = row.get(10)?;
    let duration_ms: i64 = row.get(11)?;
    let sample_rate: Option<i64> = row.get(12)?;
    let bit_depth: Option<i64> = row.get(13)?;
    let bitrate_kbps: Option<i64> = row.get(14)?;

    Ok(TrackRow {
        id,
        path: PathBuf::from(path),
        title,
        artist,
        album,
        album_artist,
        album_key,
        genre,
        track_no: track_no.and_then(|v| u32::try_from(v).ok()),
        disc_no: disc_no.and_then(|v| u32::try_from(v).ok()),
        year: year.and_then(|v| u32::try_from(v).ok()),
        duration_ms: duration_ms as u64,
        sample_rate: sample_rate.and_then(|v| u32::try_from(v).ok()),
        bit_depth: bit_depth.and_then(|v| u8::try_from(v).ok()),
        bitrate_kbps: bitrate_kbps.and_then(|v| u32::try_from(v).ok()),
    })
}

// ---------------------------------------------------------------------------
// Query building
// ---------------------------------------------------------------------------

/// WHERE conditions and their positional parameters, built up piece by piece.
#[derive(Default)]
struct Conditions {
    clauses: Vec<String>,
    params: Vec<String>,
}

impl Conditions {
    /// `column = ?` when `value` is set.
    fn eq(&mut self, column: &str, value: &Option<String>) {
        if let Some(v) = value {
            self.clauses.push(format!("{column} = ?"));
            self.params.push(v.clone());
        }
    }

    /// Every whitespace-separated word must appear (substring, case-insensitive
    /// for ASCII) in the title, artist, album or album artist.
    fn search(&mut self, search: Option<&str>) {
        const PER_WORD: &str = "(title LIKE ? ESCAPE '\\' OR artist LIKE ? ESCAPE '\\' \
                                OR album LIKE ? ESCAPE '\\' OR album_artist LIKE ? ESCAPE '\\')";
        for word in search.unwrap_or("").split_whitespace() {
            self.clauses.push(PER_WORD.to_string());
            let pattern = like_pattern(word);
            self.params.extend(std::iter::repeat_n(pattern, 4));
        }
    }

    /// `WHERE a AND b ...`, or an empty string when there are no conditions.
    fn where_sql(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.clauses.join(" AND "))
        }
    }
}

/// Build a SQL LIKE pattern matching *word* as a substring.
///
/// Characters `%`, `_`, and `\` are escaped with a backslash.
fn like_pattern(word: &str) -> String {
    let mut s = String::from("%");
    for c in word.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

// ---------------------------------------------------------------------------
// Library methods (query.rs)
// ---------------------------------------------------------------------------

impl Library {
    /// Return all tracks matching *f*.
    ///
    /// Ordering:
    /// - When `album_key` is specified:
    ///   `disc_no IS NULL, disc_no, track_no IS NULL, track_no, title COLLATE NOCASE`
    /// - Otherwise:
    ///   `CASE WHEN album_artist = '' THEN artist ELSE album_artist END COLLATE NOCASE,
    ///    album COLLATE NOCASE, disc_no IS NULL, disc_no, track_no IS NULL, track_no`
    pub fn list_tracks(&self, f: &TrackFilter) -> Result<Vec<TrackRow>, LibraryError> {
        let mut c = Conditions::default();
        c.eq("album_key", &f.album_key);
        c.eq("artist", &f.artist);
        c.eq("genre", &f.genre);
        c.search(f.search.as_deref());

        let order_by = if f.album_key.is_some() {
            "disc_no IS NULL, disc_no, track_no IS NULL, track_no, title COLLATE NOCASE"
        } else {
            "CASE WHEN album_artist = '' THEN artist ELSE album_artist END COLLATE NOCASE, \
             album COLLATE NOCASE, disc_no IS NULL, disc_no, track_no IS NULL, track_no"
        };
        let sql = format!("SELECT {TRACK_COLUMNS} FROM tracks {} ORDER BY {order_by}", c.where_sql());

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(c.params.iter()), track_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Return all albums, optionally filtered by *search*.
    ///
    /// Groups by `album_key`.  `album_artist` uses the effective value.
    pub fn list_albums(&self, search: Option<&str>) -> Result<Vec<AlbumRow>, LibraryError> {
        let mut c = Conditions::default();
        c.search(search);
        let sql = format!(
            "SELECT album_key, MAX(album), \
             MAX(CASE WHEN album_artist = '' THEN artist ELSE album_artist END), \
             MAX(year), COUNT(*), SUM(duration_ms) \
             FROM tracks {} GROUP BY album_key \
             ORDER BY 3 COLLATE NOCASE, 2 COLLATE NOCASE",
            c.where_sql()
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(c.params.iter()), |row| {
            let year: Option<i64> = row.get(3)?;
            let track_count: i64 = row.get(4)?;
            let duration_ms: i64 = row.get(5)?;
            Ok(AlbumRow {
                album_key: row.get(0)?,
                album: row.get(1)?,
                album_artist: row.get(2)?,
                year: year.and_then(|v| u32::try_from(v).ok()),
                track_count: track_count as u32,
                duration_ms: duration_ms as u64,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Return all artists ordered alphabetically (case-insensitive).
    ///
    /// Excludes empty artist strings.
    pub fn list_artists(&self) -> Result<Vec<(String, u32)>, LibraryError> {
        self.counts_by(Grouping::Artist)
    }

    /// Return all genres ordered alphabetically (case-insensitive).
    ///
    /// Excludes empty genre strings.
    pub fn list_genres(&self) -> Result<Vec<(String, u32)>, LibraryError> {
        self.counts_by(Grouping::Genre)
    }

    /// (value, track count) for each non-empty value of a column.
    fn counts_by(&self, g: Grouping) -> Result<Vec<(String, u32)>, LibraryError> {
        let col = g.column();
        let sql = format!(
            "SELECT {col}, COUNT(*) FROM tracks WHERE {col} <> '' \
             GROUP BY {col} ORDER BY {col} COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let count: i64 = row.get(1)?;
            Ok((row.get(0)?, count as u32))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Resolve playlist paths to track rows in the DB.
    ///
    /// The returned `usize` is the index into the input *paths* slice.
    /// Paths that are not found in the DB are silently skipped, preserving
    /// the original order.
    pub fn tracks_by_paths(&self, paths: &[PathBuf]) -> Result<Vec<(usize, TrackRow)>, LibraryError> {
        let sql = format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE path = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut out = Vec::new();
        for (i, p) in paths.iter().enumerate() {
            let key = p.to_string_lossy();
            if let Some(t) = stmt.query_row([key.as_ref()], track_from_row).optional()? {
                out.push((i, t));
            }
        }
        Ok(out)
    }
}

/// Columns `counts_by` may group on (kept closed so no SQL is built from input).
#[derive(Clone, Copy)]
enum Grouping {
    Artist,
    Genre,
}

impl Grouping {
    fn column(self) -> &'static str {
        match self {
            Grouping::Artist => "artist",
            Grouping::Genre => "genre",
        }
    }
}

// ---------------------------------------------------------------------------
// TrackRow helpers
// ---------------------------------------------------------------------------

impl TrackRow {
    /// Returns a human-readable format label for this track.
    ///
    /// Extension (lowercase) determines the base format name:
    /// - `flac` → FLAC, `mp3` → MP3, `ogg`/`oga` → Vorbis, `wav` → WAV,
    ///   `aif`/`aiff` → AIFF, `aac` → AAC, `m4a` → ALAC (if `bit_depth` is Some)
    ///   or AAC (if None).
    ///
    /// Lossless formats (FLAC / ALAC / WAV / AIFF) with both `bit_depth` and
    /// `sample_rate` produce `"${bit}-bit/${khz}kHz ${fmt}"`.
    /// Otherwise, if `bitrate_kbps` is available: `"${kbps}kbps ${fmt}"`.
    /// Otherwise, if only `sample_rate`: `"${khz}kHz ${fmt}"`.
    /// Fallback: just `${fmt}`.
    pub fn format_label(&self) -> String {
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let ext = ext.to_lowercase();

        let (fmt, is_lossless) = match ext.as_str() {
            "flac" => ("FLAC", true),
            "mp3" => ("MP3", false),
            "ogg" | "oga" => ("Vorbis", false),
            "wav" => ("WAV", true),
            "aif" | "aiff" => ("AIFF", true),
            "aac" => ("AAC", false),
            "m4a" => {
                if self.bit_depth.is_some() {
                    ("ALAC", true)
                } else {
                    ("AAC", false)
                }
            }
            other => {
                let upper = other.to_uppercase();
                return upper;
            }
        };

        let sample_rate = self.sample_rate;
        let bitrate_kbps = self.bitrate_kbps;
        let bit_depth = self.bit_depth;

        // Build kHz string from sample_rate.
        let khz = sample_rate.map(|sr| {
            if sr % 1000 == 0 {
                format!("{}", sr / 1000)
            } else {
                format!("{:.1}", sr as f64 / 1000.0)
            }
        });

        if is_lossless && let (Some(bd), Some(k)) = (bit_depth, &khz) {
            return format!("{}-bit/{}kHz {}", bd, k, fmt);
        }

        if let Some(kbps) = bitrate_kbps {
            return format!("{}kbps {}", kbps, fmt);
        }

        if let Some(k) = &khz {
            return format!("{}kHz {}", k, fmt);
        }

        fmt.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::library::{FileState, TrackMeta};

    /// Create an in-memory library with 6 test tracks.
    ///
    /// Tracks:
    /// 1. Japanese title 「一曲目」, normal album_artist
    /// 2. Empty album_artist (artist is fallback)
    /// 3. Same album, disc 1, track 1 (for grouping test)
    /// 4. Same album, disc 2, track 2 (multi-disc)
    /// 5. Empty genre
    /// 6. Title containing `%` ("100% Pure")
    fn setup_tracks(lib: &Library, dir: &TempDir) {
        let folder_id = lib.add_folder(dir.path()).unwrap();

        let write_file = |name: &str| {
            let p = dir.path().join(name);
            fs::write(&p, "fake audio data").unwrap();
            p
        };

        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };

        // 1. Japanese title
        let p1 = write_file("01.flac");
        lib.upsert_track(
            folder_id, &p1, state,
            &TrackMeta {
                title: "一曲目".into(),
                artist: "テスト歌手".into(),
                album: "テストアルバム".into(),
                album_artist: "テスト歌手".into(),
                genre: "Rock".into(),
                track_no: Some(1),
                disc_no: Some(1),
                year: Some(2024),
                duration_ms: 180_000,
                sample_rate: Some(44_100),
                bit_depth: Some(24),
                channels: Some(2),
                bitrate_kbps: Some(1_411),
            },
        ).unwrap();

        // 2. Empty album_artist
        let p2 = write_file("02.mp3");
        lib.upsert_track(
            folder_id, &p2, state,
            &TrackMeta {
                title: "二曲目".into(),
                artist: "別アーティスト".into(),
                album: "テストアルバム".into(),
                album_artist: "".into(),
                genre: "Rock".into(),
                track_no: Some(2),
                disc_no: Some(1),
                year: Some(2024),
                duration_ms: 120_000,
                sample_rate: None,
                bit_depth: None,
                channels: Some(2),
                bitrate_kbps: Some(320),
            },
        ).unwrap();

        // 3. Same album, disc 1, track 1 (different artist)
        let p3 = write_file("03.flac");
        lib.upsert_track(
            folder_id, &p3, state,
            &TrackMeta {
                title: "三曲目".into(),
                artist: "別の歌手".into(),
                album: "テストアルバム".into(),
                album_artist: "テスト歌手".into(),
                genre: "Rock".into(),
                track_no: Some(1),
                disc_no: Some(1),
                year: Some(2024),
                duration_ms: 200_000,
                sample_rate: Some(44_100),
                bit_depth: Some(16),
                channels: Some(2),
                bitrate_kbps: Some(1_411),
            },
        ).unwrap();

        // 4. Same album, disc 2, track 2 (multi-disc)
        let p4 = write_file("04.ogg");
        lib.upsert_track(
            folder_id, &p4, state,
            &TrackMeta {
                title: "四曲目".into(),
                artist: "別の歌手".into(),
                album: "テストアルバム".into(),
                album_artist: "テスト歌手".into(),
                genre: "Rock".into(),
                track_no: Some(2),
                disc_no: Some(2),
                year: Some(2024),
                duration_ms: 150_000,
                sample_rate: None,
                bit_depth: None,
                channels: Some(2),
                bitrate_kbps: Some(128),
            },
        ).unwrap();

        // 5. Empty genre
        let p5 = write_file("notag.flac");
        lib.upsert_track(
            folder_id, &p5, state,
            &TrackMeta {
                title: "五曲目".into(),
                artist: "無ジャンル".into(),
                album: "別アルバム".into(),
                album_artist: "無ジャンル".into(),
                genre: "".into(),
                track_no: None,
                disc_no: None,
                year: None,
                duration_ms: 90_000,
                sample_rate: Some(48_000),
                bit_depth: Some(24),
                channels: Some(2),
                bitrate_kbps: None,
            },
        ).unwrap();

        // 6. Title containing %
        let p6 = write_file("pure.m4a");
        lib.upsert_track(
            folder_id, &p6, state,
            &TrackMeta {
                title: "100% Pure".into(),
                artist: "%アーティスト".into(),
                album: "%アルバム".into(),
                album_artist: "%アーティスト".into(),
                genre: "%ジャンル".into(),
                track_no: None,
                disc_no: None,
                year: None,
                duration_ms: 60_000,
                sample_rate: Some(44_100),
                bit_depth: Some(24),
                channels: Some(2),
                bitrate_kbps: Some(256),
            },
        ).unwrap();
    }

    #[test]
    fn test_list_tracks_order() {
        let dir = TempDir::with_prefix("query_order").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        assert_eq!(tracks.len(), 6);

        // Without album_key filter, order is by:
        // effective album_artist, album, disc_no, track_no, title
        // Track 1 (テスト歌手, テストアルバム, disc 1, track 1)
        // Track 2 (別アーティスト, テストアルバム, disc 1, track 2) — effective album_artist = "別アーティスト"
        // Track 3 (テスト歌手, テストアルバム, disc 1, track 1) — same album_artist but different title
        // Track 4 (テスト歌手, テストアルバム, disc 2, track 2)
        // Track 5 (無ジャンル, 別アルバム)
        // Track 6 (%アーティスト, %アルバム)

        // Verify ordering: % comes before ASCII in SQLite COLLATE NOCASE
        // Actually SQLite sorts ASCII before Japanese in COLLATE NOCASE
        // Let's just verify the relative order is deterministic
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        // 5曲目 (無ジャンル) should come after テスト歌手 tracks
        let fifth_idx = titles.iter().position(|t| *t == "五曲目").unwrap();
        let first_idx = titles.iter().position(|t| *t == "一曲目").unwrap();
        assert!(fifth_idx > first_idx, "五曲目 should come after 一曲目");
    }

    #[test]
    fn test_list_tracks_with_album_key() {
        let dir = TempDir::with_prefix("query_album").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Filter to テスト歌手's tracks
        let key = "テスト歌手\u{1f}テストアルバム";
        let f = TrackFilter {
            album_key: Some(key.to_string()),
            ..Default::default()
        };
        let tracks = lib.list_tracks(&f).unwrap();
        assert_eq!(tracks.len(), 3); // tracks 1, 3, 4 (track 2 has empty album_artist → different album_key)

        // Verify ordering: disc_no IS NULL first, then disc_no, track_no IS NULL, track_no, title
        let ordering: Vec<(Option<u32>, Option<u32>, &str)> = tracks
            .iter()
            .map(|t| (t.disc_no, t.track_no, t.title.as_str()))
            .collect();

        // Expected: disc 1 track 1 (一曲目), disc 1 track 1 (三曲目), disc 2 track 2 (四曲目)
        // Wait, track 2 has empty album_artist, so it's excluded
        // Track 1: disc=Some(1), track=Some(1)
        // Track 3: disc=Some(1), track=Some(1)
        // Track 4: disc=Some(2), track=Some(2)
        assert_eq!(ordering[0].0, Some(1));
        assert_eq!(ordering[0].1, Some(1));
        assert_eq!(ordering[2].0, Some(2));
        assert_eq!(ordering[2].1, Some(2));
    }

    #[test]
    fn test_list_tracks_search_japanese() {
        let dir = TempDir::with_prefix("query_search_jp").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Search for "テスト" — should match tracks where title/artist/album/album_artist contains "テスト"
        let f = TrackFilter {
            search: Some("テスト".into()),
            ..Default::default()
        };
        let tracks = lib.list_tracks(&f).unwrap();
        assert!(!tracks.is_empty(), "Should find tracks matching 'テスト'");
    }

    #[test]
    fn test_list_tracks_search_percent() {
        let dir = TempDir::with_prefix("query_search_pct").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Search for "100% Pure" — the `%` in the search term should be escaped
        let f = TrackFilter {
            search: Some("100% Pure".into()),
            ..Default::default()
        };
        let tracks = lib.list_tracks(&f).unwrap();
        assert_eq!(tracks.len(), 1, "Should find exactly '100% Pure'");
        assert_eq!(tracks[0].title, "100% Pure");
    }

    #[test]
    fn test_list_tracks_search_case_insensitive() {
        let dir = TempDir::with_prefix("query_search_ci").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Search "pure" (lowercase) — should match "100% Pure" (case-insensitive)
        let f = TrackFilter {
            search: Some("pure".into()),
            ..Default::default()
        };
        let tracks = lib.list_tracks(&f).unwrap();
        assert!(!tracks.is_empty(), "Should find '100% Pure' searching 'pure'");
    }

    #[test]
    fn test_list_tracks_multi_word_search() {
        let dir = TempDir::with_prefix("query_multi").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Search "テスト 歌手" — both words must match (AND)
        let f = TrackFilter {
            search: Some("テスト 歌手".into()),
            ..Default::default()
        };
        let tracks = lib.list_tracks(&f).unwrap();
        // Should match tracks where both "テスト" and "歌手" appear
        assert!(!tracks.is_empty(), "Should find tracks matching both words");
    }

    #[test]
    fn test_list_albums() {
        let dir = TempDir::with_prefix("query_albums").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        let albums = lib.list_albums(None).unwrap();
        assert_eq!(albums.len(), 4); // テスト歌手/テストアルバム, 別アーティスト/テストアルバム, 無ジャンル/別アルバム, %アーティスト/%アルバム

        // Find テスト歌手's テストアルバム
        let test_album = albums.iter().find(|a| a.album == "テストアルバム" && a.album_artist == "テスト歌手").unwrap();
        assert_eq!(test_album.track_count, 3); // tracks 1, 3, 4 (track 2 has empty album_artist → excluded)
        assert_eq!(test_album.album_artist, "テスト歌手"); // effective (track 2 uses artist)
    }

    #[test]
    fn test_list_albums_search() {
        let dir = TempDir::with_prefix("query_albums_search").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        let albums = lib.list_albums(Some("テスト")).unwrap();
        assert!(albums.len() >= 2); // テスト歌手's テストアルバム + 別の歌手's テストアルバム (4-column search on title matches both)
        assert!(albums.iter().any(|a| a.album == "テストアルバム"));
    }

    #[test]
    fn test_list_artists() {
        let dir = TempDir::with_prefix("query_artists").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        let artists = lib.list_artists().unwrap();
        assert!(!artists.is_empty());
        // Should exclude empty artist strings
        assert!(artists.iter().all(|(a, _)| !a.is_empty()));
    }

    #[test]
    fn test_list_genres_excludes_empty() {
        let dir = TempDir::with_prefix("query_genres").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        let genres = lib.list_genres().unwrap();
        assert!(!genres.is_empty());
        // Genre 5 has empty genre, should be excluded
        assert!(genres.iter().all(|(g, _)| !g.is_empty()));
    }

    #[test]
    fn test_format_label_flac() {
        let dir = TempDir::with_prefix("fmt_flac").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.flac");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000, sample_rate: Some(48000), bit_depth: Some(24),
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "24-bit/48kHz FLAC");
    }

    #[test]
    fn test_format_label_mp3() {
        let dir = TempDir::with_prefix("fmt_mp3").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.mp3");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000, bitrate_kbps: Some(320),
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "320kbps MP3");
    }

    #[test]
    fn test_format_label_m4a_alac() {
        let dir = TempDir::with_prefix("fmt_alac").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.m4a");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000, sample_rate: Some(44100), bit_depth: Some(24),
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "24-bit/44.1kHz ALAC");
    }

    #[test]
    fn test_format_label_m4a_aac() {
        let dir = TempDir::with_prefix("fmt_aac").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.m4a");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000, bitrate_kbps: Some(256),
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "256kbps AAC");
    }

    #[test]
    fn test_format_label_44_1khz() {
        let dir = TempDir::with_prefix("fmt_khz").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.wav");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000, sample_rate: Some(44100),
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "44.1kHz WAV");
    }

    #[test]
    fn test_format_label_no_metadata() {
        let dir = TempDir::with_prefix("fmt_none").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let p = dir.path().join("test.ogg");
        fs::write(&p, "fake").unwrap();
        let state = FileState { mtime_ns: 1_000_000_000, size: 100 };
        lib.upsert_track(
            folder_id, &p, state,
            &TrackMeta {
                title: "t".into(), artist: "a".into(), album: "a".into(),
                duration_ms: 1000,
                ..Default::default()
            },
        ).unwrap();

        let tracks = lib.list_tracks(&TrackFilter::default()).unwrap();
        let label = &tracks[0].format_label();
        assert_eq!(label, "Vorbis");
    }

    #[test]
    fn test_like_pattern_escapes() {
        // The "100% Pure" search test (test_list_tracks_search_percent)
        // already exercises LIKE with escaped `%`.
        let lib = Library::open_in_memory().unwrap();
        let patterns = [
            ("hello", "%hello%"),
            ("100%", "%100\\%%"),
            ("what_", "%what\\_%"),
            ("a\\b", "%a\\\\b%"),
        ];
        for (input, expected) in patterns {
            assert_eq!(like_pattern(input), expected);
        }
        // Verify list_tracks works with empty filter (no query errors).
        assert!(lib.list_tracks(&TrackFilter::default()).is_ok());
    }

    #[test]
    fn test_tracks_by_paths() {
        let dir = TempDir::with_prefix("query_paths").unwrap();
        let lib = Library::open_in_memory().unwrap();
        setup_tracks(&lib, &dir);

        // Grab paths for track 1 (01.flac) and track 2 (02.mp3) from DB.
        let all = lib.list_tracks(&TrackFilter::default()).unwrap();
        let track1_path = all.iter().find(|t| t.path.to_string_lossy().ends_with("01.flac")).map(|t| t.path.clone()).unwrap();
        let track2_path = all.iter().find(|t| t.path.to_string_lossy().ends_with("02.mp3")).map(|t| t.path.clone()).unwrap();

        // Non-existent path.
        let nonexistent = dir.path().join("does_not_exist.flac");

        // [b, x, a, b]  →  positions [0, 2, 3], paths [b, a, b]
        let paths = vec![track2_path.clone(), nonexistent, track1_path.clone(), track2_path.clone()];
        let result = lib.tracks_by_paths(&paths).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[1].0, 2);
        assert_eq!(result[2].0, 3);
        assert_eq!(result[0].1.path, track2_path);
        assert_eq!(result[1].1.path, track1_path);
        assert_eq!(result[2].1.path, track2_path);

        // Empty paths → empty result.
        let empty: Vec<PathBuf> = Vec::new();
        assert!(lib.tracks_by_paths(&empty).unwrap().is_empty());
    }
}
