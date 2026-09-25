use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use crate::decode::{self, DecodeError, Decoder, StreamInfo};
use crate::coreaudio::error::CoreAudioError;
use crate::coreaudio::output::OutputUnit;
use rtrb::{Producer, RingBuffer};
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

struct Output {
    unit: OutputUnit,
    producer: Producer<f32>,
    info: StreamInfo,
    capacity: usize,
}

fn open_output(info: &StreamInfo) -> Result<Output, PlaybackError> {
    let capacity = ((info.sample_rate * info.channels as f64 * 0.5) as usize).max(4096);
    let (producer, consumer) = RingBuffer::<f32>::new(capacity);
    let mut unit = OutputUnit::new(info, consumer, None)?;
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

fn stream(
    decoder: &mut dyn Decoder,
    producer: &mut Producer<f32>,
) -> Result<(), DecodeError> {
    let info = decoder.info();
    let mut buf = vec![0.0f32; 4096 * info.channels as usize];

    loop {
        let frames = decoder.read(&mut buf)?;
        if frames == 0 {
            break;
        }

        let mut pending = &buf[..frames * info.channels as usize];
        while !pending.is_empty() {
            let (_written, rest) = producer.push_partial_slice(pending);
            pending = rest;
            if !pending.is_empty() {
                thread::sleep(Duration::from_millis(5));
            }
        }
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
pub fn play_files_blocking(paths: &[PathBuf]) -> Result<Vec<TrackFailure>, PlaybackError> {
    let mut failures = Vec::new();
    let mut idx = 0;
    let mut output: Option<Output> = None;

    let mut next = open_next(paths, &mut idx, &mut failures);

    while let Some((path, mut decoder)) = next.take() {
        let info = decoder.info();
        let same = output.as_ref().is_some_and(|o| {
            o.info.sample_rate == info.sample_rate && o.info.channels == info.channels
        });

        if !same {
            if let Some(o) = output.take() {
                drain_and_stop(o)?;
            }
            output = Some(open_output(&info)?);
        }

        next = open_next(paths, &mut idx, &mut failures);

        if let Some(o) = output.as_mut()
            && let Err(e) = stream(decoder.as_mut(), &mut o.producer)
        {
            failures.push(TrackFailure {
                path,
                error: e.into(),
            });
        }
    }

    if let Some(o) = output {
        drain_and_stop(o)?;
    }

    Ok(failures)
}

/// Plays a single audio file in a blocking manner.
///
/// This function opens the file, sets up the audio output, and streams the audio
/// data from the file to the output unit until the end of the file is reached.
pub fn play_file_blocking(path: &Path) -> Result<(), PlaybackError> {
    let failures = play_files_blocking(&[path.to_path_buf()])?;
    if let Some(f) = failures.into_iter().next() {
        return Err(f.error);
    }
    Ok(())
}
