//! OS media keys / Now Playing bridge (souvlaki).

use eframe::egui;
use mmp_core::library::TrackRow;
use mmp_core::player::{PlayState, PlayerStatus};
use souvlaki::{MediaControlEvent, MediaMetadata, MediaPlayback, MediaPosition, MediaControls, PlatformConfig, SeekDirection};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Seek step for Seek(Forward/Backward) events.
const SEEK_STEP_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
    SeekTo(u64),
}

/// Convert a souvlaki `MediaControlEvent` into a `MediaAction`.
///
/// `position_ms` is the current playback position in milliseconds.
/// `duration_ms` is the total track duration in milliseconds (0 if unknown).
pub fn media_action(
    ev: &MediaControlEvent,
    position_ms: u64,
    duration_ms: u64,
) -> Option<MediaAction> {
    use MediaAction as M;
    Some(match ev {
        MediaControlEvent::Play => M::Play,
        MediaControlEvent::Pause => M::Pause,
        MediaControlEvent::Toggle => M::Toggle,
        MediaControlEvent::Next => M::Next,
        MediaControlEvent::Previous => M::Prev,
        MediaControlEvent::Stop => M::Stop,
        MediaControlEvent::Seek(dir) => match dir {
            SeekDirection::Forward => {
                M::SeekTo(position_ms.saturating_add(SEEK_STEP_MS).min(duration_ms))
            }
            SeekDirection::Backward => {
                M::SeekTo(position_ms.saturating_sub(SEEK_STEP_MS))
            }
        },
        MediaControlEvent::SeekBy(dir, d) => {
            let ms = d.as_millis() as u64;
            match dir {
                SeekDirection::Forward => {
                    M::SeekTo(position_ms.saturating_add(ms).min(duration_ms))
                }
                SeekDirection::Backward => {
                    M::SeekTo(position_ms.saturating_sub(ms))
                }
            }
        }
        MediaControlEvent::SetPosition(p) => {
            M::SeekTo((p.0.as_millis() as u64).min(duration_ms))
        }
        // These events are handled by souvlaki internally or not mapped.
        MediaControlEvent::SetVolume(_)
        | MediaControlEvent::OpenUri(_)
        | MediaControlEvent::Raise
        | MediaControlEvent::Quit => return None,
    })
}

/// Converts a `PlayState` and position into a `MediaPlayback` for Now Playing.
pub fn playback_for(state: PlayState, position_ms: u64) -> MediaPlayback {
    let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
    match state {
        PlayState::Playing => MediaPlayback::Playing { progress },
        PlayState::Paused => MediaPlayback::Paused { progress },
        PlayState::Stopped => MediaPlayback::Stopped,
    }
}

/// Receives OS media-key / Now Playing events and publishes playback info.
pub struct MediaKeys {
    controls: MediaControls,
    rx: Receiver<MediaControlEvent>,
    last_path: Option<PathBuf>,
    last_playback: Option<(PlayState, u64)>,
}

impl MediaKeys {
    /// Registers with the OS. Returns None if the platform refuses.
    pub fn new(ctx: egui::Context) -> Option<Self> {
        let config = PlatformConfig {
            display_name: "MacMusicPlay",
            dbus_name: "macmusicplay",
            hwnd: None,
        };
        let mut controls = MediaControls::new(config).ok()?;
        let (tx, rx) = mpsc::channel();
        controls.attach(move |ev| {
            let _ = tx.send(ev);
            ctx.request_repaint();
        }).ok()?;
        Some(Self { controls, rx, last_path: None, last_playback: None })
    }

    /// Events received since the last call (non-blocking).
    pub fn poll_events(&self) -> Vec<MediaControlEvent> {
        self.rx.try_iter().collect()
    }

    /// Publishes the current track and play state to Now Playing, only when they change.
    pub fn update(&mut self, now: &PlayerStatus, track: Option<&TrackRow>) {
        if now.path != self.last_path {
            let meta = match track {
                Some(t) => MediaMetadata {
                    title: Some(&t.title),
                    artist: Some(&t.artist),
                    album: Some(&t.album),
                    duration: Some(Duration::from_millis(t.duration_ms)),
                    cover_url: None,
                },
                None => MediaMetadata::default(),
            };
            let _ = self.controls.set_metadata(meta);
            self.last_path = now.path.clone();
        }
        let key = (now.state, now.position_ms / 1000);
        if self.last_playback != Some(key) {
            let _ = self.controls.set_playback(playback_for(now.state, now.position_ms));
            self.last_playback = Some(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_action_basic() {
        // Toggle → Toggle
        assert_eq!(
            media_action(&MediaControlEvent::Toggle, 0, 0),
            Some(MediaAction::Toggle)
        );
        // Previous → Prev
        assert_eq!(
            media_action(&MediaControlEvent::Previous, 0, 0),
            Some(MediaAction::Prev)
        );
        // Raise → None
        assert_eq!(
            media_action(&MediaControlEvent::Raise, 0, 0),
            None
        );
    }

    #[test]
    fn test_media_action_seek() {
        let position: u64 = 5_000;
        let duration: u64 = 60_000;

        // Seek(Forward) → SeekTo(15000)
        assert_eq!(
            media_action(&MediaControlEvent::Seek(SeekDirection::Forward), position, duration),
            Some(MediaAction::SeekTo(15_000))
        );

        // Seek(Backward) → SeekTo(0)
        assert_eq!(
            media_action(&MediaControlEvent::Seek(SeekDirection::Backward), position, duration),
            Some(MediaAction::SeekTo(0))
        );

        // SeekBy(Forward, 100s) → capped at duration (60000)
        assert_eq!(
            media_action(
                &MediaControlEvent::SeekBy(SeekDirection::Forward, Duration::from_secs(100)),
                position,
                duration
            ),
            Some(MediaAction::SeekTo(60_000))
        );

        // SetPosition(30s) → SeekTo(30000)
        assert_eq!(
            media_action(
                &MediaControlEvent::SetPosition(MediaPosition(Duration::from_secs(30))),
                position,
                duration
            ),
            Some(MediaAction::SeekTo(30_000))
        );
    }

    #[test]
    fn test_playback_for() {
        // Playing + 1500ms → Playing { progress: Some(MediaPosition(1500ms)) }
        assert_eq!(
            playback_for(PlayState::Playing, 1500),
            MediaPlayback::Playing { progress: Some(MediaPosition(Duration::from_millis(1500))) }
        );
        // Paused + 0ms → Paused { progress: Some(MediaPosition(0ms)) }
        assert_eq!(
            playback_for(PlayState::Paused, 0),
            MediaPlayback::Paused { progress: Some(MediaPosition(Duration::ZERO)) }
        );
        // Stopped + 999ms → Stopped (no progress)
        assert_eq!(
            playback_for(PlayState::Stopped, 999),
            MediaPlayback::Stopped
        );
    }
}
