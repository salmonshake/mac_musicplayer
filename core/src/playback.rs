use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use crate::coreaudio::device;
use crate::coreaudio::device_watch::{DeviceEvent, DeviceWatcher};
use crate::coreaudio::output::OutputUnit;
use crate::decode::{self, DecodeError, Decoder, StreamInfo};
use crate::coreaudio::error::CoreAudioError;
use rtrb::Producer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error(transparent)]
    CoreAudio(#[from] CoreAudioError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// A track that failed to open or play.
#[derive(Debug)]
pub struct TrackFailure {
    /// Path of the track that failed.
    pub path: PathBuf,
    /// The error that caused the failure.
    pub error: PlaybackError,
}

/// Options for playback.
#[derive(Debug, Clone, Default)]
pub struct PlaybackOptions {
    /// UID of the output device to use, or `None` for the system default.
    pub device_uid: Option<String>,
}

/// Report after playback completes.
#[derive(Debug, Default)]
pub struct PlaybackReport {
    /// Tracks that failed to open or play.
    pub failures: Vec<TrackFailure>,
    /// Whether we fell back to the default output device.
    pub device_fallback: bool,
}

struct Output {
    unit: OutputUnit,
    producer: Producer<f32>,
    info: StreamInfo,
    capacity: usize,
}

fn open_output(info: &StreamInfo, device: Option<device::DeviceId>) -> Result<Output, PlaybackError> {
    let capacity = ((info.sample_rate * info.channels as f64 * 0.5) as usize).max(4096);
    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
    let mut unit = OutputUnit::new(info, consumer, device)?;
    unit.start()?;
    Ok(Output {
        unit,
        producer,
        info: *info,
        capacity,
    })
}

fn drain_and_stop(mut out: Output) -> Result<(), PlaybackError> {
    while out.producer.slots() < out.capacity {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(50));
    out.unit.stop()?;
    Ok(())
}

fn open_next(
    paths: &[PathBuf],
    idx: &mut usize,
    failures: &mut Vec<TrackFailure>,
) -> Option<(PathBuf, Box<dyn Decoder>)> {
    while *idx < paths.len() {
        let p = paths[*idx].clone();
        *idx += 1;
        match decode::open(&p) {
            Ok(d) => return Some((p, d)),
            Err(e) => failures.push(TrackFailure {
                path: p,
                error: PlaybackError::Decode(e),
            }),
        }
    }
    None
}

/// Internal session that manages output state and device monitoring.
struct Session {
    output: Option<Output>,
    device: Option<device::DeviceId>,
    fallback: bool,
    events: Option<Receiver<DeviceEvent>>,
    _watcher: Option<DeviceWatcher>,
}

impl Session {
    /// Check for device-change events and handle fallback if needed.
    fn check_devices(&mut self) -> Result<(), PlaybackError> {
        let Some(rx) = &self.events else {
            return Ok(());
        };
        let mut changed = false;
        while let Ok(ev) = rx.try_recv() {
            if ev == DeviceEvent::DevicesChanged {
                changed = true;
            }
        }
        if !changed {
            return Ok(());
        }
        let Some(id) = self.device else {
            return Ok(());
        };
        if device::is_alive(id) {
            return Ok(());
        }
        self.device = None;
        self.fallback = true;
        if let Some(old) = self.output.take() {
            let info = old.info;
            let mut old = old;
            let _ = old.unit.stop();
            drop(old);
            self.output = Some(open_output(&info, None)?);
        }
        Ok(())
    }

    /// Ensure an output is open for the given stream info.
    /// Reopens if the info differs (sample_rate or channels).
    fn ensure_output(&mut self, info: &StreamInfo) -> Result<(), PlaybackError> {
        let same = self.output.as_ref().is_some_and(|o| {
            o.info.sample_rate == info.sample_rate && o.info.channels == info.channels
        });
        if same {
            return Ok(());
        }
        if let Some(o) = self.output.take() {
            drain_and_stop(o)?;
        }
        self.output = Some(open_output(info, self.device)?);
        Ok(())
    }

    /// Wait until the ring buffer is full, then stop the unit.
    fn drain_and_stop_output(&mut self) -> Result<(), PlaybackError> {
        loop {
            self.check_devices()?;
            let Some(o) = self.output.as_ref() else {
                return Ok(());
            };
            if o.producer.slots() >= o.capacity {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(50));
        if let Some(mut o) = self.output.take() {
            let _ = o.unit.stop();
        }
        Ok(())
    }
}

fn stream(
    decoder: &mut dyn Decoder,
    session: &mut Session,
) -> Result<(), PlaybackError> {
    let info = decoder.info();
    let mut buf = vec![0.0f32; 4096 * info.channels as usize];

    loop {
        let frames = match decoder.read(&mut buf) {
            Ok(n) => n,
            Err(e) => return Err(PlaybackError::Decode(e)),
        };
        if frames == 0 {
            break;
        }

        let mut pending = &buf[..frames * info.channels as usize];
        while !pending.is_empty() {
            session.check_devices()?;
            let Some(o) = session.output.as_mut() else {
                return Ok(());
            };
            let (_written, rest) = o.producer.push_partial_slice(pending);
            pending = rest;
            if !pending.is_empty() {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    Ok(())
}

/// Plays a single audio file in a blocking manner.
///
/// This function opens the file, sets up the audio output, and streams the audio
/// data from the file to the output unit until the end of the file is reached.
pub fn play_file_blocking(path: &Path) -> Result<(), PlaybackError> {
    let report = play_files_blocking(&[path.to_path_buf()], &PlaybackOptions::default())?;
    if let Some(f) = report.failures.into_iter().next() {
        return Err(f.error);
    }
    Ok(())
}

/// Plays multiple audio files in sequence without gaps.
///
/// Opens each file in order, streaming audio data through the same ring buffer
/// and output unit. If the sample rate or channel count changes between files,
/// the output unit is reopened after the current file finishes draining.
/// Files that fail to open or play are skipped and collected as `TrackFailure`
/// results. Returns `Ok` with any track failures on successful playback,
/// or `Err` only for fatal errors like failing to open the output device.
///
/// If `opts.device_uid` is set, the specified device is used. If the device
/// disappears during playback, the session falls back to the default output
/// device and continues playing.
pub fn play_files_blocking(
    paths: &[PathBuf],
    opts: &PlaybackOptions,
) -> Result<PlaybackReport, PlaybackError> {
    let mut failures = Vec::new();
    let mut idx = 0;
    let mut device_fallback_used = false;

    // Resolve device UID to DeviceId (or None for default).
    let device = match &opts.device_uid {
        Some(uid) => match device::find_output_device_by_uid(uid) {
            Ok(Some(d)) => Some(d.id),
            Ok(None) => { device_fallback_used = true; None },
            Err(e) => return Err(PlaybackError::CoreAudio(e)),
        },
        None => None,
    };

    // Start device watching if possible.  Failure to watch does not prevent playback.
    let (watcher, events) = match DeviceWatcher::new() {
        Ok((w, rx)) => (Some(w), Some(rx)),
        Err(_) => (None, None),
    };

    let mut session = Session {
        output: None,
        device,
        fallback: device_fallback_used,
        events,
        _watcher: watcher,
    };

    let mut next = open_next(paths, &mut idx, &mut failures);

    while let Some((path, mut decoder)) = next.take() {
        let info = decoder.info();

        session.ensure_output(&info)?;

        next = open_next(paths, &mut idx, &mut failures);

        match stream(decoder.as_mut(), &mut session) {
            Ok(()) => {}
            Err(PlaybackError::CoreAudio(e)) => return Err(PlaybackError::CoreAudio(e)),
            Err(e) => failures.push(TrackFailure {
                path,
                error: e,
            }),
        }
    }

    session.drain_and_stop_output()?;

    Ok(PlaybackReport {
        failures,
        device_fallback: session.fallback,
    })
}
