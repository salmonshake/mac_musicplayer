//! SQLite-backed library storage.
//!
//! The database is the authoritative store for track metadata.
//! Files can always be re-scanned, so the DB is rebuildable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use super::{LibraryError, TrackMeta};

/// Schema DDL for the library database.
///
/// Uses `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS`
/// so that `open` and `open_in_memory` can run it safely on every call.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS folders(
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS tracks(
    id             INTEGER PRIMARY KEY,
    path           TEXT NOT NULL UNIQUE,
    folder_id      INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    mtime_ns       INTEGER NOT NULL,
    size           INTEGER NOT NULL,
    title          TEXT NOT NULL,
    artist         TEXT NOT NULL,
    album          TEXT NOT NULL,
    album_artist   TEXT NOT NULL,
    album_key      TEXT NOT NULL,
    genre          TEXT NOT NULL,
    track_no       INTEGER,
    disc_no        INTEGER,
    year           INTEGER,
    duration_ms    INTEGER NOT NULL,
    sample_rate    INTEGER,
    bit_depth      INTEGER,
    channels       INTEGER,
    bitrate_kbps   INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tracks_folder_id ON tracks(folder_id);
CREATE INDEX IF NOT EXISTS idx_tracks_album_key ON tracks(album_key, disc_no, track_no);
CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
"#;

/// Handle to the library database.
pub struct Library {
    conn: Connection,
}

/// Immutable file state for change detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileState {
    pub mtime_ns: i64,
    pub size: i64,
}

impl Library {
    /// Open an on-disk library at *path*.
    ///
    /// Enables WAL mode and foreign keys, then creates the schema.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        let conn = Connection::open(path).map_err(LibraryError::Db)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(LibraryError::Db)?;
        Self::init(&conn)?;
        Ok(Library { conn })
    }

    /// Open an in-memory library (for tests).
    ///
    /// WAL mode is not needed for in-memory databases.
    pub fn open_in_memory() -> Result<Self, LibraryError> {
        let conn = Connection::open_in_memory().map_err(LibraryError::Db)?;
        Self::init(&conn)?;
        Ok(Library { conn })
    }

    /// Create tables and set `user_version = 1`.
    fn init(conn: &Connection) -> Result<(), LibraryError> {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(LibraryError::Db)?;
        conn.execute_batch(SCHEMA).map_err(LibraryError::Db)?;
        conn.pragma_update(None, "user_version", "1")
            .map_err(LibraryError::Db)?;
        Ok(())
    }

    /// Register a folder. Returns the folder id (new or existing).
    pub fn add_folder(&self, path: &Path) -> Result<i64, LibraryError> {
        let path_str = path.to_string_lossy().into_owned();
        self.conn
            .execute(
                "INSERT OR IGNORE INTO folders(path) VALUES (?1)",
                params![&path_str],
            )
            .map_err(LibraryError::Db)?;
        let id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM folders WHERE path = ?1",
                params![&path_str],
                |row| row.get(0),
            )
            .map_err(LibraryError::Db)?;
        Ok(id)
    }

    /// Remove a folder and all tracks inside it.
    ///
    /// Requires `foreign_keys=ON` so the cascade deletes tracks.
    pub fn remove_folder(&self, path: &Path) -> Result<(), LibraryError> {
        let path_str = path.to_string_lossy().into_owned();
        self.conn
            .execute(
                "DELETE FROM folders WHERE path = ?1",
                params![&path_str],
            )
            .map_err(LibraryError::Db)?;
        Ok(())
    }

    /// Return all registered folders ordered by path.
    pub fn folders(&self) -> Result<Vec<(i64, PathBuf)>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path FROM folders ORDER BY path")
            .map_err(LibraryError::Db)?;
        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let path: String = row.get(1)?;
                Ok((id, PathBuf::from(path)))
            })
            .map_err(LibraryError::Db)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(LibraryError::Db)
    }

    /// File states for every track in *folder_id*.
    pub fn file_states(
        &self,
        folder_id: i64,
    ) -> Result<HashMap<PathBuf, FileState>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT path, mtime_ns, size FROM tracks WHERE folder_id = ?1",
            )
            .map_err(LibraryError::Db)?;
        let rows = stmt.query_map(params![folder_id], |row| {
            let path: String = row.get(0)?;
            let mtime_ns: i64 = row.get(1)?;
            let size: i64 = row.get(2)?;
            Ok((PathBuf::from(path), FileState { mtime_ns, size }))
        }).map_err(LibraryError::Db)?;
        rows.collect::<std::result::Result<HashMap<_, _>, _>>()
            .map_err(LibraryError::Db)
    }

    /// Upsert a track row.
    ///
    /// `album_key` is `effective_album_artist + "\u{1f}" + album`.
    /// On conflict of `path`, all columns except `path` are updated.
    pub fn upsert_track(
        &self,
        folder_id: i64,
        path: &Path,
        state: FileState,
        meta: &TrackMeta,
    ) -> Result<(), LibraryError> {
        let path_str = path.to_string_lossy().into_owned();
        let album_key = format!(
            "{}\u{1f}{}",
            meta.effective_album_artist(),
            meta.album
        );
        self.conn.execute(
            "INSERT INTO tracks(path, folder_id, mtime_ns, size, title, artist, album, album_artist, album_key, genre, track_no, disc_no, year, duration_ms, sample_rate, bit_depth, channels, bitrate_kbps) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18) ON CONFLICT(path) DO UPDATE SET folder_id=excluded.folder_id, mtime_ns=excluded.mtime_ns, size=excluded.size, title=excluded.title, artist=excluded.artist, album=excluded.album, album_artist=excluded.album_artist, album_key=excluded.album_key, genre=excluded.genre, track_no=excluded.track_no, disc_no=excluded.disc_no, year=excluded.year, duration_ms=excluded.duration_ms, sample_rate=excluded.sample_rate, bit_depth=excluded.bit_depth, channels=excluded.channels, bitrate_kbps=excluded.bitrate_kbps",
            params![
                &path_str,
                folder_id,
                state.mtime_ns,
                state.size,
                &meta.title,
                &meta.artist,
                &meta.album,
                &meta.album_artist,
                &album_key,
                &meta.genre,
                meta.track_no.map(|v| v as i64),
                meta.disc_no.map(|v| v as i64),
                meta.year.map(|v| v as i64),
                meta.duration_ms as i64,
                meta.sample_rate.map(|v| v as i64),
                meta.bit_depth.map(|v| v as i64),
                meta.channels.map(|v| v as i64),
                meta.bitrate_kbps.map(|v| v as i64),
            ],
        ).map_err(LibraryError::Db)?;
        Ok(())
    }

    /// Remove a single track by path.
    pub fn remove_track(&self, path: &Path) -> Result<(), LibraryError> {
        let path_str = path.to_string_lossy().into_owned();
        self.conn
            .execute(
                "DELETE FROM tracks WHERE path = ?1",
                params![&path_str],
            )
            .map_err(LibraryError::Db)?;
        Ok(())
    }

    /// Total number of tracks in the database.
    pub fn track_count(&self) -> Result<u64, LibraryError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .map_err(LibraryError::Db)?;
        Ok(count as u64)
    }

    /// Execute a closure inside a manual transaction.
    ///
    /// On `Ok`, commits. On `Err`, rolls back (best-effort) and propagates the error.
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&Library) -> Result<T, LibraryError>,
    ) -> Result<T, LibraryError> {
        self.conn
            .execute_batch("BEGIN")
            .map_err(LibraryError::Db)?;
        let result = f(self);
        match result {
            Ok(v) => {
                self.conn
                    .execute_batch("COMMIT")
                    .map_err(LibraryError::Db)?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn sample_meta() -> TrackMeta {
        TrackMeta {
            title: "t".into(),
            artist: "a".into(),
            album: "b".into(),
            duration_ms: 1000,
            ..Default::default()
        }
    }

    #[test]
    fn test_add_folder_returns_id() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let id = lib.add_folder(dir.path()).unwrap();
        assert!(id >= 0);
    }

    #[test]
    fn test_add_folder_idempotent() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let id1 = lib.add_folder(dir.path()).unwrap();
        let id2 = lib.add_folder(dir.path()).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_folders_returns_registered() {
        let lib = Library::open_in_memory().unwrap();
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        lib.add_folder(dir_a.path()).unwrap();
        lib.add_folder(dir_b.path()).unwrap();
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 2);
        // Should be ordered by path.
        assert!(folders[0].1 < folders[1].1);
    }

    #[test]
    fn test_upsert_and_track_count() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);
    }

    #[test]
    fn test_upsert_twice_updates() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        // Update with different meta.
        let mut meta2 = sample_meta();
        meta2.title = "updated".into();
        let state2 = FileState {
            mtime_ns: 2_000_000_000,
            size: 200,
        };
        lib.upsert_track(folder_id, &file_path, state2, &meta2)
            .unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);
        // Verify it was updated, not duplicated.
        let mut stmt = lib
            .conn
            .prepare("SELECT title, mtime_ns, size FROM tracks WHERE path = ?")
            .unwrap();
        let path_str = file_path.to_string_lossy().into_owned();
        let (title, mtime_ns, size): (String, i64, i64) = stmt
            .query_row(params![&path_str], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!(title, "updated");
        assert_eq!(mtime_ns, 2_000_000_000);
        assert_eq!(size, 200);
    }

    #[test]
    fn test_file_states_roundtrip() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_234_567,
            size: 42,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        let states = lib.file_states(folder_id).unwrap();
        assert_eq!(states.len(), 1);
        let retrieved = states.get(&file_path).unwrap();
        assert_eq!(*retrieved, state);
    }

    #[test]
    fn test_remove_folder_deletes_tracks() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);
        lib.remove_folder(dir.path()).unwrap();
        assert_eq!(lib.track_count().unwrap(), 0);
    }

    #[test]
    fn test_remove_track() {
        let lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        lib.remove_track(&file_path).unwrap();
        assert_eq!(lib.track_count().unwrap(), 0);
    }

    #[test]
    fn test_transaction_commits() {
        let mut lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        let mut meta = sample_meta();
        meta.title = "tx_test".into();
        lib.transaction(|tx| {
            tx.upsert_track(folder_id, &file_path, state, &meta)
        })
        .unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);
    }

    #[test]
    fn test_transaction_rolls_back_on_error() {
        let mut lib = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib.add_folder(dir.path()).unwrap();
        // Insert one track outside transaction.
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        assert_eq!(lib.track_count().unwrap(), 1);

        // Transaction that fails — should not add a second track.
        let file_path2 = dir.path().join("song2.flac");
        fs::write(&file_path2, "fake").unwrap();
        let result: Result<(), LibraryError> = lib.transaction(|tx| {
            let meta = sample_meta();
            tx.upsert_track(folder_id, &file_path2, state, &meta)?;
            Err(LibraryError::Tag("rollback".into()))
        });
        assert!(result.is_err());
        // Count should still be 1.
        assert_eq!(lib.track_count().unwrap(), 1);
    }

    #[test]
    fn test_open_file_twice_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("lib.db");
        // First open.
        let _lib1 = Library::open(&db_path).unwrap();
        // Second open of the same file — should succeed (schema creation is idempotent).
        let lib2 = Library::open(&db_path).unwrap();
        assert_eq!(lib2.track_count().unwrap(), 0);
    }

    #[test]
    fn test_open_in_memory_isolated() {
        let lib1 = Library::open_in_memory().unwrap();
        let lib2 = Library::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder_id = lib1.add_folder(dir.path()).unwrap();
        let file_path = dir.path().join("song.flac");
        fs::write(&file_path, "fake").unwrap();
        let state = FileState {
            mtime_ns: 1_000_000_000,
            size: 100,
        };
        lib1.upsert_track(folder_id, &file_path, state, &sample_meta())
            .unwrap();
        assert_eq!(lib1.track_count().unwrap(), 1);
        assert_eq!(lib2.track_count().unwrap(), 0);
    }
}
