use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rtrb::{Producer, RingBuffer};

use super::queue::{Queue, RepeatMode};
use crate::coreaudio::device::{self, find_output_device_by_uid, DeviceId};
use crate::coreaudio::device_watch::{DeviceEvent, DeviceWatcher};
use crate::coreaudio::output::{OutputUnit, RenderShared};
use crate::decode::{self, Decoder, StreamInfo};

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

/// Engine main loop: handle commands and keep the ring buffer fed.
fn engine_main(
    rx: Receiver<Command>,
    status: Arc<Mutex<PlayerStatus>>,
    device_uid: Option<String>,
) {
    let mut engine = Engine::new(status, device_uid);
    loop {
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => engine.handle(cmd),
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Drain any further queued commands before doing audio work.
        while let Ok(cmd) = rx.try_recv() {
            if matches!(cmd, Command::Quit) {
                return;
            }
            engine.handle(cmd);
        }
        engine.pump();
    }
}

/// Frames decoded per read.
const DECODE_BLOCK_FRAMES: usize = 4096;
/// How long to wait for the render callback to acknowledge a flush.
const FLUSH_TIMEOUT: Duration = Duration::from_millis(200);
/// Minimum interval between status position updates.
const STATUS_INTERVAL: Duration = Duration::from_millis(50);
/// "Previous" restarts the current track when past this position.
const PREV_RESTART_MS: u64 = 3000;

/// An open output: CoreAudio unit plus the producer side of its ring.
struct Output {
    unit: OutputUnit,
    producer: Producer<f32>,
    shared: Arc<RenderShared>,
    info: StreamInfo,
    capacity: usize,
    /// Whether the unit is currently started.
    running: bool,
}

/// One track's worth of audio written into the ring.
struct Segment {
    item: usize,
    path: PathBuf,
    /// Frame index (in the output's frames_played timeline) where it starts.
    start_frame: u64,
    /// Position within the track (frames) at `start_frame` — non-zero after a seek.
    offset_frames: u64,
}

struct Engine {
    status: Arc<Mutex<PlayerStatus>>,
    /// Resolved output device (None = system default). Cleared on unplug.
    device: Option<DeviceId>,
    events: Option<Receiver<DeviceEvent>>,
    _watcher: Option<DeviceWatcher>,
    queue: Option<Queue>,
    state: PlayState,
    volume: f32,
    out: Option<Output>,
    /// Decoder for the track currently being written into the ring.
    decoder: Option<Box<dyn Decoder>>,
    buf: Vec<f32>,
    pending: Vec<f32>,
    pending_pos: usize,
    /// Samples pushed into the current output's ring.
    samples_written: u64,
    /// Tracks present in the ring; front is the one being heard.
    segments: VecDeque<Segment>,
    /// The track after the current one cannot be appended gaplessly
    /// (open failed or format differs); handle it after the ring drains.
    no_prefetch: bool,
    last_status: Instant,
}

impl Engine {
    fn new(status: Arc<Mutex<PlayerStatus>>, device_uid: Option<String>) -> Self {
        let mut error = None;
        let device = match device_uid {
            Some(uid) => match find_output_device_by_uid(&uid) {
                Ok(Some(d)) => Some(d.id),
                Ok(None) => {
                    error = Some(format!("output device {uid} not found; using the default"));
                    None
                }
                Err(e) => {
                    error = Some(e.to_string());
                    None
                }
            },
            None => None,
        };
        let (watcher, events) = match DeviceWatcher::new() {
            Ok((w, rx)) => (Some(w), Some(rx)),
            Err(_) => (None, None),
        };
        let volume = status.lock().map(|s| s.volume).unwrap_or(1.0);
        let engine = Self {
            status,
            device,
            events,
            _watcher: watcher,
            queue: None,
            state: PlayState::Stopped,
            volume,
            out: None,
            decoder: None,
            buf: Vec::new(),
            pending: Vec::new(),
            pending_pos: 0,
            samples_written: 0,
            segments: VecDeque::new(),
            no_prefetch: false,
            last_status: Instant::now(),
        };
        if error.is_some() {
            engine.set_status(|s| s.error = error);
        }
        engine
    }

    fn set_status(&self, f: impl FnOnce(&mut PlayerStatus)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    // ---- commands -------------------------------------------------------

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::PlayQueue { items, start } => {
                let (shuffle, repeat) = self
                    .status
                    .lock()
                    .map(|s| (s.shuffle, s.repeat))
                    .unwrap_or((false, RepeatMode::Off));
                self.queue = Some(Queue::new(items, start, shuffle, repeat, seed()));
                self.state = PlayState::Playing;
                self.restart_current(0);
            }
            Command::TogglePause => self.toggle_pause(),
            Command::Stop => self.stop(),
            Command::Next => {
                let moved = self.queue.as_mut().and_then(|q| q.next()).is_some();
                if moved {
                    self.resume_if_stopped();
                    self.restart_current(0);
                }
            }
            Command::Prev => {
                if self.position_ms() > PREV_RESTART_MS {
                    self.restart_current(0);
                } else if self.queue.as_mut().and_then(|q| q.prev()).is_some() {
                    self.resume_if_stopped();
                    self.restart_current(0);
                }
            }
            Command::JumpTo(item) => {
                if self.queue.as_mut().and_then(|q| q.jump_to_item(item)).is_some() {
                    self.resume_if_stopped();
                    self.restart_current(0);
                }
            }
            Command::SeekMs(ms) => {
                if self.state != PlayState::Stopped
                    && let Some(rate) = self.out.as_ref().map(|o| o.info.sample_rate)
                {
                    let frames = (ms as f64 * rate / 1000.0) as u64;
                    self.restart_current(frames);
                }
            }
            Command::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                if let Some(o) = &self.out {
                    o.shared.set_volume(self.volume);
                }
                let v = self.volume;
                self.set_status(|s| s.volume = v);
            }
            Command::SetShuffle(on) => {
                if let Some(q) = self.queue.as_mut() {
                    q.set_shuffle(on);
                }
                self.set_status(|s| s.shuffle = on);
                self.publish_track();
            }
            Command::SetRepeat(r) => {
                if let Some(q) = self.queue.as_mut() {
                    q.set_repeat(r);
                }
                self.set_status(|s| s.repeat = r);
                self.publish_track();
            }
            Command::Quit => {}
        }
    }

    fn resume_if_stopped(&mut self) {
        if self.state == PlayState::Stopped {
            self.state = PlayState::Playing;
        }
    }

    fn toggle_pause(&mut self) {
        match self.state {
            PlayState::Playing => {
                if let Some(o) = self.out.as_mut()
                    && o.running
                {
                    let _ = o.unit.stop();
                    o.running = false;
                }
                self.state = PlayState::Paused;
            }
            PlayState::Paused => {
                self.state = PlayState::Playing;
                if let Some(o) = self.out.as_mut()
                    && !o.running
                {
                    match o.unit.start() {
                        Ok(()) => o.running = true,
                        Err(e) => {
                            let msg = e.to_string();
                            self.set_status(|s| s.error = Some(msg));
                        }
                    }
                }
            }
            PlayState::Stopped => {
                if self.queue.as_ref().and_then(|q| q.current()).is_some() {
                    self.state = PlayState::Playing;
                    self.restart_current(0);
                }
            }
        }
        let st = self.state;
        self.set_status(|s| s.state = st);
    }

    fn stop(&mut self) {
        self.out = None;
        self.decoder = None;
        self.clear_pending();
        self.segments.clear();
        self.state = PlayState::Stopped;
        self.set_status(|s| {
            s.state = PlayState::Stopped;
            s.position_ms = 0;
        });
    }

    // ---- track start / seek ---------------------------------------------

    /// (Re)start the queue's current track at `offset_frames`, discarding
    /// anything already in the ring. Tracks that fail to open are skipped.
    fn restart_current(&mut self, offset_frames: u64) {
        let Some(queue) = self.queue.as_mut() else {
            self.stop();
            return;
        };
        let attempts = queue.items().len().max(1);
        let mut last_error = None;
        for _ in 0..attempts {
            let Some((item, path)) = self.queue.as_ref().and_then(|q| q.current()) else {
                break;
            };
            let path = path.to_path_buf();
            match decode::open(&path) {
                Ok(mut dec) => {
                    if offset_frames > 0
                        && let Err(e) = dec.seek(offset_frames)
                    {
                        last_error = Some(format!("{}: {e}", path.display()));
                    }
                    let info = dec.info();
                    if let Err(e) = self.prepare_output(&info) {
                        self.set_status(|s| s.error = Some(e));
                        self.stop();
                        return;
                    }
                    self.decoder = Some(dec);
                    self.clear_pending();
                    self.no_prefetch = false;
                    self.segments.clear();
                    self.segments.push_back(Segment {
                        item,
                        path,
                        start_frame: self.frames_written(),
                        offset_frames,
                    });
                    self.set_status(|s| s.error = last_error);
                    self.publish_track();
                    return;
                }
                Err(e) => {
                    last_error = Some(format!("{}: {e}", path.display()));
                    // Skip the broken track (respecting repeat) and try the next one.
                    if self.queue.as_mut().and_then(|q| q.next()).is_none() {
                        break;
                    }
                }
            }
        }
        self.set_status(|s| s.error = last_error);
        self.stop();
    }

    /// Make sure an empty output matching `info` exists, started or not
    /// according to the current state.
    fn prepare_output(&mut self, info: &StreamInfo) -> Result<(), String> {
        let same_format = self.out.as_ref().is_some_and(|o| {
            o.info.sample_rate == info.sample_rate && o.info.channels == info.channels
        });
        let flushed = same_format && self.state == PlayState::Playing && self.flush_and_wait();
        if !flushed {
            self.open_output(info)?;
        }
        Ok(())
    }

    /// Ask the render callback to discard the ring and wait for it.
    fn flush_and_wait(&mut self) -> bool {
        let Some(o) = self.out.as_ref() else {
            return false;
        };
        if !o.running {
            return false;
        }
        o.shared.request_flush();
        let deadline = Instant::now() + FLUSH_TIMEOUT;
        while o.shared.flush_pending() {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // Everything written is now either played or discarded.
        let ch = u64::from(o.info.channels.max(1));
        self.samples_written = o.shared.frames_played() * ch;
        true
    }

    fn open_output(&mut self, info: &StreamInfo) -> Result<(), String> {
        self.out = None; // stop the old unit before creating a new one
        self.samples_written = 0;
        let capacity = ((info.sample_rate * f64::from(info.channels) * 0.5) as usize).max(4096);
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);
        let mut unit = OutputUnit::new(info, consumer, self.device).map_err(|e| e.to_string())?;
        let shared = unit.shared();
        shared.set_volume(self.volume);
        let running = self.state == PlayState::Playing;
        if running {
            unit.start().map_err(|e| e.to_string())?;
        }
        self.out = Some(Output {
            unit,
            producer,
            shared,
            info: *info,
            capacity,
            running,
        });
        Ok(())
    }

    // ---- pumping ----------------------------------------------------------

    fn pump(&mut self) {
        self.check_devices();
        self.advance_segments();
        if self.out.is_none() {
            return;
        }
        // Keep writing until the ring is full or there is nothing to write.
        loop {
            if !self.write_pending() {
                break; // ring full
            }
            if self.decoder.is_none() && !self.try_prefetch() {
                break;
            }
            let Some(dec) = self.decoder.as_mut() else {
                break;
            };
            let ch = dec.info().channels.max(1) as usize;
            self.buf.resize(DECODE_BLOCK_FRAMES * ch, 0.0);
            match dec.read(&mut self.buf) {
                Ok(0) => self.decoder = None,
                Ok(n) => {
                    self.pending.clear();
                    self.pending.extend_from_slice(&self.buf[..n * ch]);
                    self.pending_pos = 0;
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.set_status(|s| s.error = Some(msg));
                    self.decoder = None;
                }
            }
        }
        self.finish_if_drained();
        self.maybe_publish_position();
    }

    /// Push pending samples. Returns false when the ring is full.
    fn write_pending(&mut self) -> bool {
        let Some(o) = self.out.as_mut() else {
            return false;
        };
        let rest = &self.pending[self.pending_pos..];
        if rest.is_empty() {
            return true;
        }
        let (written, left) = o.producer.push_partial_slice(rest);
        let n = written.len();
        let done = left.is_empty();
        self.pending_pos += n;
        self.samples_written += n as u64;
        done
    }

    /// Open the next track and append it to the ring if the format matches.
    fn try_prefetch(&mut self) -> bool {
        if self.no_prefetch || self.segments.len() != 1 {
            return false;
        }
        let Some((item, path)) = self.queue.as_ref().and_then(|q| q.peek_after_current()) else {
            self.no_prefetch = true;
            return false;
        };
        let path = path.to_path_buf();
        let dec = match decode::open(&path) {
            Ok(d) => d,
            Err(_) => {
                // Handled (skipped) when the current track has drained.
                self.no_prefetch = true;
                return false;
            }
        };
        let info = dec.info();
        let matches = self.out.as_ref().is_some_and(|o| {
            o.info.sample_rate == info.sample_rate && o.info.channels == info.channels
        });
        if !matches {
            self.no_prefetch = true;
            return false;
        }
        self.segments.push_back(Segment {
            item,
            path,
            start_frame: self.frames_written(),
            offset_frames: 0,
        });
        self.decoder = Some(dec);
        true
    }

    /// Move to the next segment once playback crosses its start.
    fn advance_segments(&mut self) {
        let Some(played) = self.out.as_ref().map(|o| o.shared.frames_played()) else {
            return;
        };
        let mut changed = false;
        while self.segments.len() >= 2 && self.segments.get(1).is_some_and(|s| played >= s.start_frame) {
            self.segments.pop_front();
            if let (Some(q), Some(seg)) = (self.queue.as_mut(), self.segments.front()) {
                q.jump_to_item(seg.item);
            }
            self.no_prefetch = false;
            changed = true;
        }
        if changed {
            self.publish_track();
        }
    }

    /// When the ring has fully played out and nothing follows gaplessly,
    /// move to the next track (new output if needed) or stop.
    fn finish_if_drained(&mut self) {
        if self.state != PlayState::Playing || self.decoder.is_some() || self.segments.len() > 1 {
            return;
        }
        if self.pending_pos < self.pending.len() {
            return;
        }
        let drained = self.out.as_ref().is_some_and(|o| o.producer.slots() >= o.capacity);
        if !drained {
            return;
        }
        let next = self.queue.as_mut().and_then(|q| q.advance_on_end()).is_some();
        if next {
            self.restart_current(0);
        } else {
            self.stop();
        }
    }

    // ---- devices ----------------------------------------------------------

    fn check_devices(&mut self) {
        let Some(rx) = &self.events else {
            return;
        };
        let mut changed = false;
        while let Ok(ev) = rx.try_recv() {
            if ev == DeviceEvent::DevicesChanged {
                changed = true;
            }
        }
        let Some(id) = self.device else {
            return;
        };
        if !changed || device::is_alive(id) {
            return;
        }
        // The selected device disappeared: fall back to the default output
        // and resume the current track where it was.
        self.device = None;
        self.set_status(|s| {
            s.error = Some("output device disconnected; using the default".to_owned())
        });
        if self.state != PlayState::Stopped {
            let frames = self.position_frames();
            self.out = None;
            self.restart_current(frames);
        }
    }

    // ---- position / status ------------------------------------------------

    fn frames_written(&self) -> u64 {
        let ch = self.out.as_ref().map_or(1, |o| u64::from(o.info.channels.max(1)));
        self.samples_written / ch
    }

    fn position_frames(&self) -> u64 {
        let (Some(o), Some(seg)) = (self.out.as_ref(), self.segments.front()) else {
            return 0;
        };
        let played = o.shared.frames_played();
        seg.offset_frames + played.saturating_sub(seg.start_frame)
    }

    fn position_ms(&self) -> u64 {
        let Some(rate) = self.out.as_ref().map(|o| o.info.sample_rate) else {
            return 0;
        };
        if rate <= 0.0 {
            return 0;
        }
        (self.position_frames() as f64 * 1000.0 / rate) as u64
    }

    fn maybe_publish_position(&mut self) {
        if self.last_status.elapsed() < STATUS_INTERVAL {
            return;
        }
        self.last_status = Instant::now();
        let pos = self.position_ms();
        let st = self.state;
        self.set_status(|s| {
            s.position_ms = pos;
            s.state = st;
        });
    }

    /// Publish the current track, queue view and state.
    fn publish_track(&mut self) {
        let current = self.segments.front().map(|s| (s.item, s.path.clone()));
        let upcoming = self.queue.as_ref().map(|q| q.upcoming()).unwrap_or_default();
        let pos = self.position_ms();
        let st = self.state;
        self.set_status(|s| {
            s.state = st;
            s.current_item = current.as_ref().map(|c| c.0);
            s.path = current.map(|c| c.1);
            s.position_ms = pos;
            s.upcoming = upcoming;
        });
        self.last_status = Instant::now();
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_pos = 0;
    }
}

/// Shuffle seed from the clock (determinism is only needed in tests).
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
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

    // ---- playback tests (real output device, volume 0) --------------------

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata").join(name)
    }

    fn have(names: &[&str]) -> bool {
        let ok = names.iter().all(|n| testdata(n).exists());
        if !ok {
            eprintln!("skip: testdata missing");
        }
        ok
    }

    fn muted_player() -> Player {
        let p = Player::new(None);
        p.set_volume(0.0);
        p
    }

    /// Poll until `pred` holds or `timeout` passes; returns the last status.
    fn wait_for(
        p: &Player,
        timeout: Duration,
        pred: impl Fn(&super::PlayerStatus) -> bool,
    ) -> Option<super::PlayerStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            let s = p.status();
            if pred(&s) {
                return Some(s);
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn plays_one_track_to_the_end() {
        if !have(&["gap_a.flac"]) {
            return;
        }
        let p = muted_player();
        p.play_queue(vec![testdata("gap_a.flac")], 0);
        let s = wait_for(&p, Duration::from_secs(1), |s| s.state == PlayState::Playing)
            .expect("should start playing");
        assert_eq!(s.current_item, Some(0));
        std::thread::sleep(Duration::from_secs(1));
        let pos = p.status().position_ms;
        assert!((700..=1400).contains(&pos), "position after 1s: {pos}");
        wait_for(&p, Duration::from_secs(4), |s| s.state == PlayState::Stopped)
            .expect("should stop after the track ends");
    }

    #[test]
    fn seek_moves_position() {
        if !have(&["gap_a.flac"]) {
            return;
        }
        let p = muted_player();
        p.play_queue(vec![testdata("gap_a.flac")], 0);
        wait_for(&p, Duration::from_secs(1), |s| s.state == PlayState::Playing).expect("playing");
        p.seek_ms(2000);
        std::thread::sleep(Duration::from_millis(150));
        let pos = p.status().position_ms;
        assert!((1950..=2500).contains(&pos), "position after seek: {pos}");
    }

    #[test]
    fn next_moves_to_the_following_track() {
        if !have(&["gap_a.flac", "gap_b.flac"]) {
            return;
        }
        let p = muted_player();
        p.play_queue(vec![testdata("gap_a.flac"), testdata("gap_b.flac")], 0);
        wait_for(&p, Duration::from_secs(1), |s| s.state == PlayState::Playing).expect("playing");
        p.next();
        let s = wait_for(&p, Duration::from_millis(500), |s| s.current_item == Some(1))
            .expect("next should move to item 1");
        assert!(s.position_ms < 500, "next should start from the top: {}", s.position_ms);
    }

    #[test]
    fn pause_freezes_position() {
        if !have(&["gap_a.flac"]) {
            return;
        }
        let p = muted_player();
        p.play_queue(vec![testdata("gap_a.flac")], 0);
        wait_for(&p, Duration::from_secs(1), |s| s.state == PlayState::Playing).expect("playing");
        std::thread::sleep(Duration::from_millis(300));
        p.toggle_pause();
        wait_for(&p, Duration::from_millis(300), |s| s.state == PlayState::Paused).expect("paused");
        std::thread::sleep(Duration::from_millis(100));
        let a = p.status().position_ms;
        std::thread::sleep(Duration::from_millis(400));
        let b = p.status().position_ms;
        assert!(b.abs_diff(a) <= 60, "position moved while paused: {a} -> {b}");
        p.toggle_pause();
        std::thread::sleep(Duration::from_millis(400));
        let c = p.status().position_ms;
        assert!(c >= b + 200, "position should advance after resume: {b} -> {c}");
    }

    #[test]
    fn gapless_advances_between_tracks() {
        if !have(&["gap_a.flac", "gap_b.flac"]) {
            return;
        }
        let p = muted_player();
        let t0 = Instant::now();
        p.play_queue(vec![testdata("gap_a.flac"), testdata("gap_b.flac")], 0);
        wait_for(&p, Duration::from_secs(4), |s| s.current_item == Some(1))
            .expect("should advance to the second track");
        let switched = t0.elapsed();
        assert!(
            (Duration::from_millis(2700)..=Duration::from_millis(3600)).contains(&switched),
            "switched at {switched:?}"
        );
        wait_for(&p, Duration::from_secs(4), |s| s.state == PlayState::Stopped)
            .expect("should stop after both tracks");
        let total = t0.elapsed();
        assert!(
            (Duration::from_millis(5700)..=Duration::from_millis(7000)).contains(&total),
            "total {total:?}"
        );
    }

    #[test]
    fn skips_tracks_that_cannot_be_opened() {
        if !have(&["gap_a.flac"]) {
            return;
        }
        let p = muted_player();
        p.play_queue(vec![PathBuf::from("/nonexistent/x.flac"), testdata("gap_a.flac")], 0);
        let s = wait_for(&p, Duration::from_secs(1), |s| s.state == PlayState::Playing)
            .expect("should play the next openable track");
        assert_eq!(s.current_item, Some(1));
        assert!(s.error.is_some(), "the skipped track should be reported");
    }
}
