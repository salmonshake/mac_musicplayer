use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::queue::RepeatMode;

/// Internal state of the playback engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

/// Snapshot of the player state exposed to the UI.
#[derive(Debug, Clone)]
pub struct PlayerStatus {
    /// Current play state.
    pub state: PlayState,
    /// Index into the queue items (None when stopped).
    pub current_item: Option<usize>,
    /// File path of the current track.
    pub path: Option<PathBuf>,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Total duration of the current track in milliseconds (0 until known).
    pub duration_ms: u64,
    /// Linear volume 0.0 .. 1.0.
    pub volume: f32,
    /// Whether shuffle is enabled.
    pub shuffle: bool,
    /// Current repeat mode.
    pub repeat: RepeatMode,
    /// Last error message (cleared on next successful operation).
    pub error: Option<String>,
    /// Item indices of tracks after the current one.
    pub upcoming: Vec<usize>,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            state: PlayState::Stopped,
            current_item: None,
            path: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 1.0,
            shuffle: false,
            repeat: RepeatMode::default(),
            error: None,
            upcoming: Vec::new(),
        }
    }
}

/// Commands that the UI thread sends to the engine thread.
#[allow(dead_code)] // 7.4b onwards: most variants will be handled by real engine logic.
#[derive(Debug)]
enum Command {
    /// Start playing the queue from `start` index.
    PlayQueue { items: Vec<PathBuf>, start: usize },
    /// Toggle pause / resume.
    TogglePause,
    /// Stop playback and reset to stopped state.
    Stop,
    /// User-requested next track.
    Next,
    /// User-requested previous track.
    Prev,
    /// Seek to the given millisecond position.
    SeekMs(u64),
    /// Set linear volume (0.0 .. 1.0).
    SetVolume(f32),
    /// Enable or disable shuffle.
    SetShuffle(bool),
    /// Set the repeat mode.
    SetRepeat(RepeatMode),
    /// Jump to a specific item by its index into the queue.
    JumpTo(usize),
    /// Shut down the engine thread.
    Quit,
}

/// Handle to the background playback engine.
///
/// All public methods send a command to the engine thread and return
/// immediately.  The actual audio processing happens on the engine thread.
#[derive(Debug)]
pub struct Player {
    tx: Sender<Command>,
    status: Arc<Mutex<PlayerStatus>>,
    join: Option<JoinHandle<()>>,
}

impl Player {
    /// Create a new player.
    ///
    /// `device_uid` is an optional Core Audio device UID.  When `None`
    /// the system default output device is used.  This skeleton task
    /// only stores the value; actual device selection is implemented
    /// in a later task.
    pub fn new(device_uid: Option<String>) -> Self {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(PlayerStatus::default()));
        let st = Arc::clone(&status);
        let join = std::thread::Builder::new()
            .name("mmp-player".into())
            .spawn(move || engine_main(rx, st, device_uid))
            .ok();
        Self {
            tx,
            status,
            join,
        }
    }

    /// Start playing the given queue of files.
    pub fn play_queue(&self, items: Vec<PathBuf>, start: usize) {
        let _ = self.tx.send(Command::PlayQueue { items, start });
    }

    /// Toggle pause / resume.
    pub fn toggle_pause(&self) {
        let _ = self.tx.send(Command::TogglePause);
    }

    /// Stop playback.
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }

    /// Skip to the next track.
    pub fn next(&self) {
        let _ = self.tx.send(Command::Next);
    }

    /// Go to the previous track.
    pub fn prev(&self) {
        let _ = self.tx.send(Command::Prev);
    }

    /// Seek to the given millisecond position.
    pub fn seek_ms(&self, ms: u64) {
        let _ = self.tx.send(Command::SeekMs(ms));
    }

    /// Set the playback volume (0.0 .. 1.0).
    pub fn set_volume(&self, v: f32) {
        let _ = self.tx.send(Command::SetVolume(v));
    }

    /// Enable or disable shuffle.
    pub fn set_shuffle(&self, on: bool) {
        let _ = self.tx.send(Command::SetShuffle(on));
    }

    /// Set the repeat mode.
    pub fn set_repeat(&self, r: RepeatMode) {
        let _ = self.tx.send(Command::SetRepeat(r));
    }

    /// Jump to a specific item by its index into the queue.
    pub fn jump_to_item(&self, i: usize) {
        let _ = self.tx.send(Command::JumpTo(i));
    }

    /// Return a snapshot of the current player state.
    ///
    /// If the Mutex has been poisoned (engine thread panicked), returns
    /// the default (stopped) status.
    pub fn status(&self) -> PlayerStatus {
        match self.status.lock() {
            Ok(s) => s.clone(),
            Err(_) => PlayerStatus::default(),
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Engine main loop.
///
/// Receives commands from `rx` and processes them.  This skeleton task
/// only handles `Quit` (and disconnect) — all other commands are no-ops
/// to be implemented in the next task.
fn engine_main(
    rx: Receiver<Command>,
    status: Arc<Mutex<PlayerStatus>>,
    device_uid: Option<String>,
) {
    let _ = (&status, &device_uid);
    loop {
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(Command::Quit) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(_other) => {
                // 7.4b onwards: handle other commands here.
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{Player, PlayState};

    /// Helper: create a minimal queue of paths for testing.
    fn make_paths(n: usize) -> Vec<PathBuf> {
        (0..n).map(|i| PathBuf::from(format!("/tmp/{i}.flac"))).collect()
    }

    #[test]
    fn test_new_status_is_stopped_with_default_volume() {
        let player = Player::new(None);
        let status = player.status();
        assert_eq!(status.state, PlayState::Stopped);
        assert_eq!(status.volume, 1.0);
        assert!(status.current_item.is_none());
        assert!(status.path.is_none());
        assert_eq!(status.position_ms, 0);
        assert_eq!(status.duration_ms, 0);
        assert!(!status.shuffle);
        assert!(status.error.is_none());
        assert!(status.upcoming.is_empty());
    }

    #[test]
    fn test_commands_do_not_panic() {
        let player = Player::new(None);
        // These should all send a command and return without panicking.
        player.play_queue(make_paths(3), 0);
        player.toggle_pause();
        player.stop();
        player.next();
        player.prev();
        player.seek_ms(1000);
        player.set_volume(0.5);
        player.set_shuffle(true);
        player.set_repeat(super::RepeatMode::All);
        player.jump_to_item(1);
        // Status should still be accessible.
        let status = player.status();
        assert_eq!(status.state, PlayState::Stopped);
    }

    #[test]
    fn test_drop_completes_within_two_seconds() {
        let player = Player::new(None);
        // Send a few commands to exercise the channel.
        player.play_queue(make_paths(2), 0);
        player.next();
        // Drop should send Quit and join the engine thread.
        let start = Instant::now();
        drop(player);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "Player::drop took {:?}, expected < 2s",
            elapsed
        );
    }
}
