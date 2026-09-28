//! OS media keys / Now Playing bridge (souvlaki).

use eframe::egui;
use souvlaki::{MediaControlEvent, MediaControls, PlatformConfig, SeekDirection};
use std::sync::mpsc::{self, Receiver};

/// Seek step for Seek(Forward/Backward) events.
#[allow(dead_code)]
const SEEK_STEP_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Receives OS media-key / Now Playing events and publishes playback info.
#[allow(dead_code)]
pub struct MediaKeys {
    controls: MediaControls,
    rx: Receiver<MediaControlEvent>,
}

#[allow(dead_code)]
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
        Some(Self { controls, rx })
    }

    /// Events received since the last call (non-blocking).
    pub fn poll_events(&self) -> Vec<MediaControlEvent> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use souvlaki::MediaPosition;
    use std::time::Duration;

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
}
