use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use super::{Decoder, DecodeError, StreamInfo};

pub struct VorbisDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    info: StreamInfo,
    pending: Vec<f32>,
    pending_pos: usize,
}

impl VorbisDecoder {
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let format = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).map_err(|e| DecodeError::Codec(e.to_string()))?;
        let track = format.default_track(TrackType::Audio).ok_or(DecodeError::NoAudioTrack)?;
        let track_id = track.id;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(DecodeError::NoAudioTrack)?;
        let sample_rate = params.sample_rate.ok_or_else(|| DecodeError::Unsupported("unknown sample rate".into()))?;
        let channels = params.channels.as_ref().map(|c| c.count() as u32).filter(|&c| c > 0).ok_or_else(|| DecodeError::Unsupported("unknown channel count".into()))?;
        let decoder = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).map_err(|e| DecodeError::Codec(e.to_string()))?;

        Ok(Self {
            format,
            decoder,
            track_id,
            info: StreamInfo {
                sample_rate: sample_rate as f64,
                channels,
            },
            pending: Vec::new(),
            pending_pos: 0,
        })
    }
}

impl Decoder for VorbisDecoder {
    fn info(&self) -> StreamInfo {
        self.info
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<usize, DecodeError> {
        let ch = self.info.channels as usize;
        let want = (buf.len() / ch) * ch;
        let mut written = 0;

        while written < want {
            if self.pending_pos < self.pending.len() {
                let n = (self.pending.len() - self.pending_pos).min(want - written);
                buf[written..written + n].copy_from_slice(&self.pending[self.pending_pos..self.pending_pos + n]);
                self.pending_pos += n;
                written += n;
                continue;
            }
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(e) => return Err(DecodeError::Codec(e.to_string())),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    self.pending.resize(audio.samples_interleaved(), 0.0);
                    audio.copy_to_slice_interleaved(&mut self.pending);
                    self.pending_pos = 0;
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Codec(e.to_string())),
            }
        }
        Ok(written / ch)
    }
}
