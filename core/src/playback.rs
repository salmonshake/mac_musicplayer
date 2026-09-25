use std::path::Path;
use std::thread;
use std::time::Duration;
use crate::decode::{self, DecodeError};
use crate::coreaudio::output::OutputUnit;
use crate::coreaudio::error::CoreAudioError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error(transparent)]
    CoreAudio(#[from] CoreAudioError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// Plays a single audio file in a blocking manner.
///
/// This function opens the file, sets up the audio output, and streams the audio
/// data from the file to the output unit until the end of the file is reached.
pub fn play_file_blocking(path: &Path) -> Result<(), PlaybackError> {
    let mut decoder = decode::open(path)?;
    let info = decoder.info();

    let capacity = ((info.sample_rate * info.channels as f64 * 0.5) as usize).max(4096);
    let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);

    let mut unit = OutputUnit::new(&info, consumer)?;
    unit.start()?;

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

    while producer.slots() < capacity {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(50));
    unit.stop()?;

    Ok(())
}
