use crate::coreaudio::ext_audio_file::{ExtAudioFile, StreamInfo};
use super::{Decoder, DecodeError};
use std::path::Path;

pub struct ExtDecoder {
    file: ExtAudioFile,
    info: StreamInfo,
    /// Set when a seek landed past the end; `read` then reports EOF.
    at_end: bool,
}

impl ExtDecoder {
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let mut file = ExtAudioFile::open(path)?;
        let info = file.file_info()?;
        file.set_client_f32_interleaved(&info)?;
        Ok(Self { file, info, at_end: false })
    }
}

impl Decoder for ExtDecoder {
    fn info(&self) -> StreamInfo {
        self.info
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<usize, DecodeError> {
        if self.at_end {
            return Ok(0);
        }
        Ok(self.file.read(buf)?)
    }

    fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
        // Don't clamp with length_frames(): it counts file-format frames, which
        // differ from client frames for HE-AAC (22.05 kHz base layer vs 44.1 kHz
        // output). A seek past the end is treated as end of stream instead.
        let target = i64::try_from(frame).unwrap_or(i64::MAX);
        self.at_end = self.file.seek(target).is_err();
        Ok(())
    }
}
