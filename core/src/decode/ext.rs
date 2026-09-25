use crate::coreaudio::ext_audio_file::{ExtAudioFile, StreamInfo};
use super::{Decoder, DecodeError};
use std::path::Path;

pub struct ExtDecoder {
    file: ExtAudioFile,
    info: StreamInfo,
}

impl ExtDecoder {
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let mut file = ExtAudioFile::open(path)?;
        let info = file.file_info()?;
        file.set_client_f32_interleaved(&info)?;
        Ok(Self { file, info })
    }
}

impl Decoder for ExtDecoder {
    fn info(&self) -> StreamInfo {
        self.info
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<usize, DecodeError> {
        Ok(self.file.read(buf)?)
    }
}
