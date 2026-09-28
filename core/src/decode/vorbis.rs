use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Timestamp;
use super::{Decoder, DecodeError, StreamInfo};

pub struct VorbisDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    info: StreamInfo,
    ts_per_frame: f64,
    seek_target: Option<i64>, // シーク先（フレーム単位）。到達するまで手前を捨てる
    eof: bool,
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

        // Compute ts_per_frame: how many timestamp units correspond to 1 frame (1/sample_rate seconds).
        let ts_per_frame: f64 = match track.time_base {
            Some(tb) => tb.denom.get() as f64 / (tb.numer.get() as f64 * sample_rate as f64),
            None => 1.0,
        };

        Ok(Self {
            format,
            decoder,
            track_id,
            info: StreamInfo {
                sample_rate: sample_rate as f64,
                channels,
            },
            ts_per_frame,
            seek_target: None,
            eof: false,
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

        if self.eof {
            return Ok(0);
        }

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
                Ok(None) => {
                    self.eof = true;
                    break;
                }
                Err(e) => return Err(DecodeError::Codec(e.to_string())),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let pts = packet.pts.get();
            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    self.pending.resize(audio.samples_interleaved(), 0.0);
                    audio.copy_to_slice_interleaved(&mut self.pending);
                    self.pending_pos = 0;
                    if let Some(target) = self.seek_target {
                        let frames = (self.pending.len() / ch) as i64;
                        let start = (pts as f64 / self.ts_per_frame).round() as i64;
                        let drop = (target - start).clamp(0, frames);
                        self.pending_pos = drop as usize * ch;
                        if frames > 0 && start + frames > target {
                            self.seek_target = None;
                        }
                    }
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Codec(e.to_string())),
            }
        }
        Ok(written / ch)
    }

    fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
        self.pending.clear();
        self.pending_pos = 0;
        self.eof = false;
        let ts = Timestamp::new((frame as f64 * self.ts_per_frame).round() as i64);
        let seek_to = SeekTo::Timestamp { ts, track_id: self.track_id };
        match self.format.seek(SeekMode::Accurate, seek_to) {
            Ok(_) => {
                self.decoder.reset();
                self.seek_target = Some(frame as i64);
                Ok(())
            }
            // 範囲外は末尾扱い（以後 read は 0）
            Err(SymError::SeekError(_)) => {
                self.seek_target = None;
                self.eof = true;
                Ok(())
            }
            Err(e) => Err(DecodeError::Codec(e.to_string())),
        }
    }
}
