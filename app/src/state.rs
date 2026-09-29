//! Application state and background scanning / watching.
// Fields and methods are consumed by the UI in Tasks 6.5+.

use eframe::egui;
use mmp_core::library::{
    scan_all_in_background, AlbumRow, Library, LibraryWatcher, ScanEvent, ScanHandle, TrackFilter,
    TrackRow,
};
use mmp_core::paths;
use mmp_core::playlist::PlaylistStore;
use mmp_core::player::{Player, PlayerStatus, PlayState};
use mmp_core::settings::{Settings, ThemeMode};
use crate::media::MediaAction;
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
    if !existing.iter().any(|e| e == base) {
        return base.to_owned();
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{base} {n}");
        if !existing.iter().any(|e| e == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Returns (current_position, neighbor_position) for a playlist move.
///
/// `positions` is the list of positions on the m3u8 file.
/// - If `up` is true, the neighbor is `row - 1` (None when `row == 0`).
/// - If `up` is false, the neighbor is `row + 1` (None when out of bounds).
///   When `row` itself is out of bounds, returns None.
pub fn playlist_move_target(positions: &[usize], row: usize, up: bool) -> Option<(usize, usize)> {
    let neighbor = if up {
        row.checked_sub(1)?
    } else {
        row.checked_add(1)?
    };
    let &cur = positions.get(row)?;
    let &nbr = positions.get(neighbor)?;
    Some((cur, nbr))
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
    /// 再生エンジン (7.5)。
    pub player: Player,
    /// poll ごとに取得する最新の再生状態 (7.5)。
    pub now: PlayerStatus,
    /// キューに入れた時点の曲一覧。now.current_item はこの Vec の添字 (7.5)。
    pub queue_items: Vec<TrackRow>,
    /// Up Next パネルの表示切替フラグ (7.8)。
    pub show_queue: bool,
    /// プレイリスト名の一覧（ソート済み）。
    pub playlists: Vec<String>,
    /// プレイリストファイルのストア。ディレクトリ作成に失敗した場合は None。
    pub playlist_store: Option<PlaylistStore>,
    /// Nav::Playlist 用: `tracks` の各行がプレイリストファイル上の何番目か。
    pub playlist_positions: Vec<usize>,
    /// インライン名前変更進行中: (旧名, 編集バッファ)。
    pub renaming_playlist: Option<(String, String)>,
    /// 次フレームで Rename TextEdit にフォーカスを当てるフラグ。
    pub rename_focus: bool,
    scan: Option<ScanHandle>,
    watch: Option<(LibraryWatcher, Receiver<ScanEvent>)>,
    /// In-memory LRU artwork texture cache.
    pub art: crate::artwork::ArtTextures,
    /// Persisted user settings (11.3).
    pub settings: Settings,
    settings_path: Option<PathBuf>,
}

impl AppState {
    /// Opens the library, loads the current view and starts a background scan.
    pub fn open(ctx: &egui::Context) -> Result<Self, String> {
        let db_path = paths::library_db_path().map_err(|e| e.to_string())?;
        let lib = Library::open(&db_path).map_err(|e| e.to_string())?;
        let settings_path = paths::settings_path().ok();
        let settings = settings_path.as_deref().map(Settings::load).unwrap_or_default();
        let (playlist_store, store_err) =
            match paths::playlists_dir().and_then(PlaylistStore::new) {
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
        let theme = std::env::var("MMP_THEME").ok().and_then(|v| ThemeMode::from_str_opt(&v)).unwrap_or(s.settings.theme);
        ctx.set_theme(theme_pref(theme));
        if s.settings.output_device.is_some() {
            s.player.set_device(s.settings.output_device.clone());
        }
        s.player.set_output_mode(s.settings.bit_perfect, s.settings.hog_mode);
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
        if self.search.is_empty() {
            None
        } else {
            Some(self.search.clone())
        }
    }

    /// Reloads folders, genres and the lists for the current `nav` / `search`.
    pub fn reload(&mut self) {
        self.selected_track = None;
        match self.lib.folders() {
            Ok(v) => self.folders = v,
            Err(e) => {
                self.status = format!("Load error: {e}");
                self.folders.clear();
            }
        }
        match self.lib.list_genres() {
            Ok(v) => self.genres = v,
            Err(e) => {
                self.status = format!("Load error: {e}");
                self.genres.clear();
            }
        }
        let search = self.search_opt();
        self.playlist_positions.clear();
        let filter = match self.nav.clone() {
            Nav::AllTracks => Some(TrackFilter {
                search,
                ..Default::default()
            }),
            Nav::Album(key) => Some(TrackFilter {
                album_key: Some(key),
                ..Default::default()
            }),
            Nav::Artist(a) => Some(TrackFilter {
                artist: Some(a),
                search,
                ..Default::default()
            }),
            Nav::Genre(g) => Some(TrackFilter {
                genre: Some(g),
                search,
                ..Default::default()
            }),
            Nav::Playlist(name) => {
                self.load_playlist_tracks(&name);
                return;
            }
            Nav::Albums => {
                match self.lib.list_albums(search.as_deref()) {
                    Ok(v) => self.albums = v,
                    Err(e) => {
                        self.status = format!("Load error: {e}");
                        self.albums.clear();
                    }
                }
                None
            }
            Nav::Artists => {
                match self.lib.list_artists() {
                    Ok(v) => self.artists = v,
                    Err(e) => {
                        self.status = format!("Load error: {e}");
                        self.artists.clear();
                    }
                }
                None
            }
            Nav::Genres => None,
        };
        if let Some(f) = filter {
            match self.lib.list_tracks(&f) {
                Ok(v) => self.tracks = v,
                Err(e) => {
                    self.status = format!("Load error: {e}");
                    self.tracks.clear();
                }
            }
        }
    }

    /// Reloads the playlist name list from `playlist_store`.
    pub fn reload_playlists(&mut self) {
        match self.playlist_store.as_ref().map(|st| st.list()) {
            Some(Ok(names)) => self.playlists = names,
            Some(Err(e)) => {
                self.status = format!("Load error: {e}");
                self.playlists.clear();
            }
            None => self.playlists.clear(),
        }
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

    /// Loads playlist tracks from `playlist_store` and resolves paths to `TrackRow`.
    fn load_playlist_tracks(&mut self, name: &str) {
        let paths = match self
            .playlist_store
            .as_ref()
            .map(|st| st.load(name))
        {
            Some(Ok(paths)) => paths,
            Some(Err(e)) => {
                self.status = format!("Load error: {e}");
                self.tracks.clear();
                return;
            }
            None => {
                self.tracks.clear();
                return;
            }
        };
        match self.lib.tracks_by_paths(&paths) {
            Ok(rows) => {
                self.playlist_positions = rows.iter().map(|(i, _)| *i).collect();
                self.tracks = rows.into_iter().map(|(_, t)| t).collect();
            }
            Err(e) => {
                self.status = format!("Load error: {e}");
                self.tracks.clear();
            }
        }
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

    /// 表示中の一覧をキューに入れ、index の曲から再生する。
    // 7.6 以降の UI で使う
    pub fn play_from(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.queue_items = self.tracks.clone();
        let paths: Vec<PathBuf> = self.queue_items.iter().map(|t| t.path.clone()).collect();
        self.player.play_queue(paths, index);
    }

    /// 再生中の曲（キューに入れた時点の TrackRow）を返す。
    // 7.6 以降の UI で使う
    pub fn now_track(&self) -> Option<&TrackRow> {
        self.now.current_item.and_then(|i| self.queue_items.get(i))
    }

    /// Creates a new empty playlist, navigates to it.
    ///
    /// Returns `Some(name)` on success, `None` when `playlist_store` is unavailable.
    pub fn create_playlist(&mut self) -> Option<String> {
        let store = self.playlist_store.as_ref()?;
        let name = unique_playlist_name(&self.playlists);
        match store.create(&name) {
            Ok(()) => {
                self.reload_playlists();
                self.set_nav(Nav::Playlist(name.clone()));
                Some(name)
            }
            Err(e) => {
                self.status = format!("Could not create playlist: {e}");
                None
            }
        }
    }

    /// Renames an existing playlist.
    ///
    /// Returns `true` on success, `false` on failure.
    pub fn rename_playlist(&mut self, old: &str, new: &str) -> bool {
        let store = match &self.playlist_store {
            Some(st) => st,
            None => {
                self.status = "Playlists unavailable".to_owned();
                return false;
            }
        };
        let result = store.rename(old, new);
        match result {
            Ok(()) => {
                self.reload_playlists();
                if self.nav == Nav::Playlist(old.to_string()) {
                    self.nav = Nav::Playlist(new.to_string());
                }
                true
            }
            Err(e) => {
                self.status = format!("Could not rename playlist: {e}");
                false
            }
        }
    }

    /// Deletes a playlist by name.
    pub fn delete_playlist(&mut self, name: &str) {
        let store = match &self.playlist_store {
            Some(st) => st,
            None => {
                self.status = "Playlists unavailable".to_owned();
                return;
            }
        };
        let result = store.delete(name);
        match result {
            Ok(()) => {
                self.reload_playlists();
                if self.nav == Nav::Playlist(name.to_string()) {
                    self.set_nav(Nav::AllTracks);
                }
            }
            Err(e) => {
                self.status = format!("Could not delete playlist: {e}");
            }
        }
    }

    /// Adds tracks to an existing playlist.
    pub fn add_to_playlist(&mut self, name: &str, paths: &[PathBuf]) {
        let store = match &self.playlist_store {
            Some(st) => st,
            None => {
                self.status = "Playlists unavailable".to_owned();
                return;
            }
        };
        let result = store.add(name, paths);
        match result {
            Ok(()) => {
                self.reload_playlists();
                if self.nav == Nav::Playlist(name.to_string()) {
                    self.reload();
                }
            }
            Err(e) => {
                self.status = format!("Could not add to playlist: {e}");
            }
        }
    }

    /// Removes a track from its playlist at the given display index.
    pub fn remove_from_playlist(&mut self, track_index: usize) {
        let name = match &self.nav {
            Nav::Playlist(name) => name.clone(),
            _ => return,
        };
        let pos = match self.playlist_positions.get(track_index) {
            Some(&pos) => pos,
            None => return,
        };
        let store = match &self.playlist_store {
            Some(st) => st,
            None => {
                self.status = "Playlists unavailable".to_owned();
                return;
            }
        };
        let result = store.remove_at(&name, pos);
        match result {
            Ok(()) => {
                self.reload_playlists();
                self.reload();
            }
            Err(e) => {
                self.status = format!("Could not remove from playlist: {e}");
            }
        }
    }

    /// Moves a track up or down within its playlist.
    pub fn move_in_playlist(&mut self, track_index: usize, up: bool) {
        let name = match &self.nav {
            Nav::Playlist(name) => name.clone(),
            _ => return,
        };
        let (from, to) = match playlist_move_target(&self.playlist_positions, track_index, up) {
            Some(pair) => pair,
            None => return,
        };
        let store = match &self.playlist_store {
            Some(st) => st,
            None => {
                self.status = "Playlists unavailable".to_owned();
                return;
            }
        };
        let result = store.move_item(&name, from, to);
        match result {
            Ok(()) => {
                self.reload_playlists();
                self.reload();
                self.selected_track = Some(if up {
                    track_index.saturating_sub(1)
                } else {
                    (track_index + 1).min(self.tracks.len().saturating_sub(1))
                });
            }
            Err(e) => {
                self.status = format!("Could not move in playlist: {e}");
            }
        }
    }

    /// Handles scan/watch events and the delayed search; call every frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut changed = false;

        // a) full scan
        let scan_events: Vec<ScanEvent> = match &self.scan {
            Some(h) => h.events.try_iter().collect(),
            None => Vec::new(),
        };
        let mut done = false;
        for ev in scan_events {
            match ev {
                ScanEvent::Started { folder } => {
                    self.status = format!("Scanning: {}", folder.display());
                }
                ScanEvent::Finished { stats, .. } => {
                    if stats.added + stats.updated + stats.removed > 0 {
                        changed = true;
                    }
                }
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

        // b) watcher
        let watch_events: Vec<ScanEvent> = match &self.watch {
            Some((_, rx)) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for ev in watch_events {
            if let ScanEvent::Finished { stats, .. } = ev
                && stats.added + stats.updated + stats.removed > 0
            {
                changed = true;
            }
        }

        // c) delayed search
        if let Some(t) = self.search_changed_at {
            if t.elapsed() >= Duration::from_millis(200) {
                self.search_changed_at = None;
                changed = true;
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }

        // d)
        if changed {
            self.reload();
        }
        if self.scanning || self.watch.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        // e) player status
        self.now = self.player.status();
        if self.now.state == PlayState::Playing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        // f) artwork cache
        self.art.poll(ctx);
    }

    /// Applies a play-control action (from media keys or the keyboard).
    pub fn apply_media_action(&mut self, a: MediaAction) {
        let has_track = self.now_track().is_some();
        match a {
            MediaAction::Toggle => {
                if has_track {
                    self.player.toggle_pause();
                } else if let Some(i) = self.selected_track {
                    self.play_from(i);
                }
            }
            MediaAction::Play => {
                if self.now.state == PlayState::Paused {
                    self.player.toggle_pause();
                } else if !has_track
                    && let Some(i) = self.selected_track {
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

    /// Writes settings.txt; failures go to the status line.
    fn save_settings(&mut self) {
        if let Some(p) = &self.settings_path
            && let Err(e) = self.settings.save(p)
        {
            self.status = format!("Could not save settings: {e}");
        }
    }

    /// Turns bit-perfect output on or off and saves the choice.
    pub fn set_bit_perfect(&mut self, on: bool) {
        self.settings.bit_perfect = on;
        self.player.set_output_mode(on, self.settings.hog_mode);
        self.save_settings();
    }

    /// Turns exclusive (hog mode) access on or off and saves the choice.
    pub fn set_hog_mode(&mut self, on: bool) {
        self.settings.hog_mode = on;
        self.player.set_output_mode(self.settings.bit_perfect, on);
        self.save_settings();
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
    use mmp_core::settings::ThemeMode;
    use eframe::egui;

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
        let result = unique_playlist_name(&[
            "New Playlist".to_owned(),
            "New Playlist 2".to_owned(),
        ]);
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
