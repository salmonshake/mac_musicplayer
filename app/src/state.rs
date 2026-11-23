//! Application state and background scanning / watching.

use crate::media::MediaAction;
use eframe::egui;
use mmp_core::library::{
    AlbumRow, Library, LibraryWatcher, ScanEvent, ScanHandle, TrackFilter, TrackRow,
    scan_all_in_background,
};
use mmp_core::paths;
use mmp_core::player::{PlayState, Player, PlayerStatus};
use mmp_core::playlist::{PlaylistError, PlaylistStore};
use mmp_core::settings::{Settings, ThemeMode};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// egui theme preference for a saved theme mode.
pub fn theme_pref(m: ThemeMode) -> egui::ThemePreference {
    match m {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    }
}

/// Returns a playlist name that does not appear in `existing`.
///
/// Tries `"New Playlist"` first, then appends an incrementing number
/// (e.g. `"New Playlist 2"`, `"New Playlist 3"`) until a free slot is found.
pub fn unique_playlist_name(existing: &[String]) -> String {
    let base = "New Playlist";
    let taken = |name: &str| existing.iter().any(|e| e == name);
    if !taken(base) {
        return base.to_owned();
    }
    (2usize..)
        .map(|n| format!("{base} {n}"))
        .find(|c| !taken(c))
        .unwrap_or_default()
}

/// Returns (current_position, neighbor_position) in the playlist file for
/// moving display row `row` up or down; None at either end or out of range.
///
/// `positions[i]` is the playlist-file position of display row `i`
/// (tracks missing from the library are not displayed).
pub fn playlist_move_target(positions: &[usize], row: usize, up: bool) -> Option<(usize, usize)> {
    let neighbor = if up {
        row.checked_sub(1)?
    } else {
        row.checked_add(1)?
    };
    Some((*positions.get(row)?, *positions.get(neighbor)?))
}

/// Stores `result` in `dst`; on error clears `dst` and reports it on the status line.
fn load_into<T, E: std::fmt::Display>(
    status: &mut String,
    dst: &mut Vec<T>,
    result: Result<Vec<T>, E>,
) {
    match result {
        Ok(v) => *dst = v,
        Err(e) => {
            *status = format!("Load error: {e}");
            dst.clear();
        }
    }
}

/// Sidebar navigation target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    /// All tracks.
    AllTracks,
    /// Album grid.
    Albums,
    /// One album (album_key).
    Album(String),
    /// Artist list.
    Artists,
    /// One artist.
    Artist(String),
    /// Genre list.
    Genres,
    /// One genre.
    Genre(String),
    /// One playlist (name).
    Playlist(String),
}

/// Whole-application state shared by the UI.
pub struct AppState {
    /// Library connection used by the UI thread.
    pub lib: Library,
    /// Path to the library database.
    pub db_path: PathBuf,
    /// Current navigation target.
    pub nav: Nav,
    /// Current search text.
    pub search: String,
    search_changed_at: Option<Instant>,
    /// Tracks for the current view.
    pub tracks: Vec<TrackRow>,
    /// Albums for the album grid.
    pub albums: Vec<AlbumRow>,
    /// (artist, track count).
    pub artists: Vec<(String, u32)>,
    /// (genre, track count).
    pub genres: Vec<(String, u32)>,
    /// Registered folders.
    pub folders: Vec<(i64, PathBuf)>,
    /// Index into `tracks` of the selected row.
    pub selected_track: Option<usize>,
    /// True while a full scan is running.
    pub scanning: bool,
    /// Status line text.
    pub status: String,
    /// Playback engine.
    pub player: Player,
    /// Latest player status, refreshed every frame in `poll`.
    pub now: PlayerStatus,
    /// Tracks as they were when queued; `now.current_item` indexes this.
    pub queue_items: Vec<TrackRow>,
    /// Whether the Up Next panel is shown.
    pub show_queue: bool,
    /// Playlist names (sorted).
    pub playlists: Vec<String>,
    /// Playlist file store; None if its directory could not be created.
    pub playlist_store: Option<PlaylistStore>,
    /// For `Nav::Playlist`: the playlist-file position of each row in `tracks`.
    pub playlist_positions: Vec<usize>,
    /// Inline rename in progress: (old name, edit buffer).
    pub renaming_playlist: Option<(String, String)>,
    /// Focus the rename text field on the next frame.
    pub rename_focus: bool,
    scan: Option<ScanHandle>,
    watch: Option<(LibraryWatcher, Receiver<ScanEvent>)>,
    /// Album artwork textures (background-loaded, LRU).
    pub art: crate::artwork::ArtTextures,
    /// Persisted user settings.
    pub settings: Settings,
    settings_path: Option<PathBuf>,
}

impl AppState {
    /// Opens the library, loads the current view and starts a background scan.
    pub fn open(ctx: &egui::Context) -> Result<Self, String> {
        let db_path = paths::library_db_path().map_err(|e| e.to_string())?;
        let lib = Library::open(&db_path).map_err(|e| e.to_string())?;
        let settings_path = paths::settings_path().ok();
        let settings = settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        let (playlist_store, store_err) = match paths::playlists_dir().and_then(PlaylistStore::new)
        {
            Ok(st) => (Some(st), None),
            Err(e) => (None, Some(format!("Playlists unavailable: {e}"))),
        };
        let mut s = Self {
            lib,
            db_path,
            nav: Nav::AllTracks,
            search: String::new(),
            search_changed_at: None,
            tracks: Vec::new(),
            albums: Vec::new(),
            artists: Vec::new(),
            genres: Vec::new(),
            folders: Vec::new(),
            selected_track: None,
            scanning: false,
            status: String::new(),
            player: Player::new(None),
            now: PlayerStatus::default(),
            queue_items: Vec::new(),
            show_queue: false,
            playlists: Vec::new(),
            playlist_store,
            playlist_positions: Vec::new(),
            renaming_playlist: None,
            rename_focus: false,
            scan: None,
            watch: None,
            art: crate::artwork::ArtTextures::new(ctx),
            settings,
            settings_path,
        };
        s.reload_playlists();
        s.reload();
        s.start_scan();
        if let Some(e) = store_err {
            s.status = e;
        }
        ctx.set_theme(theme_pref(s.settings.theme));
        if s.settings.output_device.is_some() {
            s.player.set_device(s.settings.output_device.clone());
        }
        s.player
            .set_output_mode(s.settings.bit_perfect, s.settings.hog_mode);
        Ok(s)
    }

    fn start_scan(&mut self) {
        self.watch = None; // stop watching while scanning
        self.scan = Some(scan_all_in_background(self.db_path.clone()));
        self.scanning = true;
        self.status = "Scanning…".into();
    }

    fn start_watch(&mut self) {
        match LibraryWatcher::start(self.db_path.clone(), Duration::from_millis(1000)) {
            Ok(w) => self.watch = Some(w),
            Err(e) => self.status = format!("Could not start watching: {e}"),
        }
    }

    fn search_opt(&self) -> Option<String> {
        (!self.search.is_empty()).then(|| self.search.clone())
    }

    /// Reloads folders, genres and the lists for the current `nav` / `search`.
    pub fn reload(&mut self) {
        self.selected_track = None;
        load_into(&mut self.status, &mut self.folders, self.lib.folders());
        load_into(&mut self.status, &mut self.genres, self.lib.list_genres());
        self.playlist_positions.clear();
        let search = self.search_opt();
        let filter = match self.nav.clone() {
            Nav::AllTracks => TrackFilter {
                search,
                ..Default::default()
            },
            Nav::Album(key) => TrackFilter {
                album_key: Some(key),
                ..Default::default()
            },
            Nav::Artist(a) => TrackFilter {
                artist: Some(a),
                search,
                ..Default::default()
            },
            Nav::Genre(g) => TrackFilter {
                genre: Some(g),
                search,
                ..Default::default()
            },
            Nav::Playlist(name) => return self.load_playlist_tracks(&name),
            Nav::Albums => {
                let albums = self.lib.list_albums(search.as_deref());
                return load_into(&mut self.status, &mut self.albums, albums);
            }
            Nav::Artists => {
                return load_into(&mut self.status, &mut self.artists, self.lib.list_artists());
            }
            Nav::Genres => return,
        };
        load_into(
            &mut self.status,
            &mut self.tracks,
            self.lib.list_tracks(&filter),
        );
    }

    /// Reloads the playlist name list from `playlist_store`.
    pub fn reload_playlists(&mut self) {
        let names = match &self.playlist_store {
            Some(st) => st.list(),
            None => Ok(Vec::new()),
        };
        load_into(&mut self.status, &mut self.playlists, names);
    }

    /// Changes the navigation target and reloads if it differs.
    pub fn set_nav(&mut self, nav: Nav) {
        if self.nav != nav {
            self.nav = nav;
            self.reload();
        }
    }

    /// Updates the search text; the reload happens 200 ms later in `poll`.
    pub fn set_search(&mut self, s: String) {
        if self.search != s {
            self.search = s;
            self.search_changed_at = Some(Instant::now());
        }
    }

    /// Loads a playlist's tracks, keeping only paths known to the library.
    fn load_playlist_tracks(&mut self, name: &str) {
        let rows = match &self.playlist_store {
            Some(st) => st
                .load(name)
                .map_err(|e| e.to_string())
                .and_then(|paths| self.lib.tracks_by_paths(&paths).map_err(|e| e.to_string())),
            None => Ok(Vec::new()),
        };
        let (positions, tracks) = match rows {
            Ok(rows) => rows.into_iter().unzip(),
            Err(e) => {
                self.status = format!("Load error: {e}");
                (Vec::new(), Vec::new())
            }
        };
        self.playlist_positions = positions;
        self.tracks = tracks;
    }

    /// Registers a folder, reloads and starts a rescan.
    pub fn add_folder(&mut self, p: PathBuf) {
        match self.lib.add_folder(&p) {
            Ok(_) => {
                self.reload();
                self.start_scan();
            }
            Err(e) => self.status = format!("Could not add folder: {e}"),
        }
    }

    /// Unregisters a folder, reloads and starts a rescan.
    pub fn remove_folder(&mut self, p: &Path) {
        match self.lib.remove_folder(p) {
            Ok(()) => {
                self.reload();
                self.start_scan();
            }
            Err(e) => self.status = format!("Could not remove folder: {e}"),
        }
    }

    /// Queues the tracks of the current view and plays from `index`.
    pub fn play_from(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.queue_items = self.tracks.clone();
        let paths: Vec<PathBuf> = self.queue_items.iter().map(|t| t.path.clone()).collect();
        self.player.play_queue(paths, index);
    }

    /// The playing track (as it was when queued).
    pub fn now_track(&self) -> Option<&TrackRow> {
        self.now.current_item.and_then(|i| self.queue_items.get(i))
    }

    // ---- playlists --------------------------------------------------------

    /// Name of the playlist being viewed, if any.
    fn current_playlist(&self) -> Option<String> {
        match &self.nav {
            Nav::Playlist(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Runs a playlist-store operation and refreshes the name list. Failures
    /// (or a missing store) go to the status line as "Could not {what}: …".
    fn playlist_op(
        &mut self,
        what: &str,
        op: impl FnOnce(&PlaylistStore) -> Result<(), PlaylistError>,
    ) -> bool {
        let Some(store) = &self.playlist_store else {
            self.status = "Playlists unavailable".to_owned();
            return false;
        };
        match op(store) {
            Ok(()) => {
                self.reload_playlists();
                true
            }
            Err(e) => {
                self.status = format!("Could not {what}: {e}");
                false
            }
        }
    }

    /// Creates an empty playlist with a free "New Playlist" name and shows it.
    pub fn create_playlist(&mut self) -> Option<String> {
        let name = unique_playlist_name(&self.playlists);
        if !self.playlist_op("create playlist", |st| st.create(&name)) {
            return None;
        }
        self.set_nav(Nav::Playlist(name.clone()));
        Some(name)
    }

    /// Renames a playlist; returns whether it succeeded.
    pub fn rename_playlist(&mut self, old: &str, new: &str) -> bool {
        let ok = self.playlist_op("rename playlist", |st| st.rename(old, new));
        if ok && self.current_playlist().as_deref() == Some(old) {
            self.nav = Nav::Playlist(new.to_string());
        }
        ok
    }

    /// Deletes a playlist; leaves its view if it was shown.
    pub fn delete_playlist(&mut self, name: &str) {
        if self.playlist_op("delete playlist", |st| st.delete(name))
            && self.current_playlist().as_deref() == Some(name)
        {
            self.set_nav(Nav::AllTracks);
        }
    }

    /// Appends tracks to a playlist.
    pub fn add_to_playlist(&mut self, name: &str, paths: &[PathBuf]) {
        if self.playlist_op("add to playlist", |st| st.add(name, paths))
            && self.current_playlist().as_deref() == Some(name)
        {
            self.reload();
        }
    }

    /// Removes display row `track_index` from the playlist being viewed.
    pub fn remove_from_playlist(&mut self, track_index: usize) {
        let (Some(name), Some(&pos)) = (
            self.current_playlist(),
            self.playlist_positions.get(track_index),
        ) else {
            return;
        };
        if self.playlist_op("remove from playlist", |st| st.remove_at(&name, pos)) {
            self.reload();
        }
    }

    /// Moves display row `track_index` up or down in the playlist being viewed
    /// and keeps it selected.
    pub fn move_in_playlist(&mut self, track_index: usize, up: bool) {
        let Some(name) = self.current_playlist() else {
            return;
        };
        let Some((from, to)) = playlist_move_target(&self.playlist_positions, track_index, up)
        else {
            return;
        };
        if self.playlist_op("move in playlist", |st| st.move_item(&name, from, to)) {
            self.reload();
            let row = if up {
                track_index.saturating_sub(1)
            } else {
                track_index + 1
            };
            self.selected_track = Some(row.min(self.tracks.len().saturating_sub(1)));
        }
    }

    // ---- per-frame --------------------------------------------------------

    /// Handles scan/watch events, the delayed search, player status and
    /// artwork loading; call every frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut changed = false;

        // Full scan.
        let scan_events: Vec<ScanEvent> = self
            .scan
            .as_ref()
            .map(|h| h.events.try_iter().collect())
            .unwrap_or_default();
        let mut done = false;
        for ev in scan_events {
            match ev {
                ScanEvent::Started { folder } => {
                    self.status = format!("Scanning: {}", folder.display());
                }
                ScanEvent::Finished { stats, .. } => changed |= stats.changed(),
                ScanEvent::Failed { folder, error } => {
                    self.status = format!("{}: {error}", folder.display());
                }
                ScanEvent::AllDone => done = true,
            }
        }
        if done {
            self.scan = None;
            self.scanning = false;
            self.status.clear();
            self.start_watch();
            changed = true;
        }

        // Folder watcher.
        if let Some((_, rx)) = &self.watch {
            for ev in rx.try_iter() {
                if let ScanEvent::Finished { stats, .. } = ev {
                    changed |= stats.changed();
                }
            }
        }

        // Delayed search.
        if let Some(t) = self.search_changed_at {
            if t.elapsed() >= Duration::from_millis(200) {
                self.search_changed_at = None;
                changed = true;
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }

        if changed {
            self.reload();
        }
        if self.scanning || self.watch.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        self.now = self.player.status();
        if self.now.state == PlayState::Playing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        self.art.poll(ctx);
    }

    /// Applies a play-control action (from media keys or the keyboard).
    pub fn apply_media_action(&mut self, a: MediaAction) {
        let has_track = self.now_track().is_some();
        match a {
            MediaAction::Toggle if has_track => self.player.toggle_pause(),
            MediaAction::Play if self.now.state == PlayState::Paused => self.player.toggle_pause(),
            MediaAction::Toggle | MediaAction::Play => {
                if !has_track && let Some(i) = self.selected_track {
                    self.play_from(i);
                }
            }
            MediaAction::Pause => {
                if self.now.state == PlayState::Playing {
                    self.player.toggle_pause();
                }
            }
            MediaAction::Next => self.player.next(),
            MediaAction::Prev => self.player.prev(),
            MediaAction::Stop => self.player.stop(),
            MediaAction::SeekTo(ms) => {
                if has_track {
                    self.player.seek_ms(ms);
                }
            }
        }
    }

    // ---- settings ---------------------------------------------------------

    /// Writes settings.txt; failures go to the status line.
    fn save_settings(&mut self) {
        if let Some(p) = &self.settings_path
            && let Err(e) = self.settings.save(p)
        {
            self.status = format!("Could not save settings: {e}");
        }
    }

    /// Sends the saved output mode to the player and saves the settings.
    fn apply_output_mode(&mut self) {
        self.player
            .set_output_mode(self.settings.bit_perfect, self.settings.hog_mode);
        self.save_settings();
    }

    /// Turns bit-perfect output on or off and saves the choice.
    pub fn set_bit_perfect(&mut self, on: bool) {
        self.settings.bit_perfect = on;
        self.apply_output_mode();
    }

    /// Turns exclusive (hog mode) access on or off and saves the choice.
    pub fn set_hog_mode(&mut self, on: bool) {
        self.settings.hog_mode = on;
        self.apply_output_mode();
    }

    /// Switches the output device (None = system default) and saves the choice.
    pub fn set_output_device(&mut self, uid: Option<String>) {
        self.settings.output_device = uid.clone();
        self.player.set_device(uid);
        self.save_settings();
    }

    /// Applies a theme and saves it to settings.txt.
    pub fn set_theme(&mut self, ctx: &egui::Context, mode: ThemeMode) {
        self.settings.theme = mode;
        ctx.set_theme(theme_pref(mode));
        self.save_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::{playlist_move_target, theme_pref, unique_playlist_name};
    use eframe::egui;
    use mmp_core::settings::ThemeMode;

    #[test]
    fn test_unique_playlist_name_empty() {
        let result = unique_playlist_name(&[]);
        assert_eq!(result, "New Playlist");
    }

    #[test]
    fn test_unique_playlist_name_one_taken() {
        let result = unique_playlist_name(&["New Playlist".to_owned()]);
        assert_eq!(result, "New Playlist 2");
    }

    #[test]
    fn test_unique_playlist_name_two_taken() {
        let result =
            unique_playlist_name(&["New Playlist".to_owned(), "New Playlist 2".to_owned()]);
        assert_eq!(result, "New Playlist 3");
    }

    #[test]
    fn test_unique_playlist_name_number_taken() {
        let result = unique_playlist_name(&["New Playlist 2".to_owned()]);
        assert_eq!(result, "New Playlist");
    }

    #[test]
    fn test_playlist_move_target() {
        let positions = vec![0, 2, 3];
        // (1, true) → (2, 0)
        assert_eq!(playlist_move_target(&positions, 1, true), Some((2, 0)));
        // (1, false) → (2, 3)
        assert_eq!(playlist_move_target(&positions, 1, false), Some((2, 3)));
        // (0, true) → None (row == 0, no neighbor)
        assert_eq!(playlist_move_target(&positions, 0, true), None);
        // (2, false) → None (row == 2 is last)
        assert_eq!(playlist_move_target(&positions, 2, false), None);
        // (5, true) → None (row out of bounds)
        assert_eq!(playlist_move_target(&positions, 5, true), None);
    }

    #[test]
    fn test_theme_pref() {
        assert_eq!(theme_pref(ThemeMode::System), egui::ThemePreference::System);
        assert_eq!(theme_pref(ThemeMode::Light), egui::ThemePreference::Light);
        assert_eq!(theme_pref(ThemeMode::Dark), egui::ThemePreference::Dark);
    }
}
