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
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

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
    #[expect(dead_code, reason = "constructed by the sidebar in 8.7")]
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
    scan: Option<ScanHandle>,
    watch: Option<(LibraryWatcher, Receiver<ScanEvent>)>,
}

impl AppState {
    /// Opens the library, loads the current view and starts a background scan.
    pub fn open() -> Result<Self, String> {
        let db_path = paths::library_db_path().map_err(|e| e.to_string())?;
        let lib = Library::open(&db_path).map_err(|e| e.to_string())?;
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
            scan: None,
            watch: None,
        };
        s.reload_playlists();
        s.reload();
        s.start_scan();
        if let Some(e) = store_err {
            s.status = e;
        }
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
    }
}
