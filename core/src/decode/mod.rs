use std::path::Path;
use crate::coreaudio::error::CoreAudioError;
pub use crate::coreaudio::ext_audio_file::StreamInfo;

pub mod ext;

/// 音声ファイルをデコードするためのトレイト
pub trait Decoder: Send {
    /// ファイルのストリーム情報（サンプルレート、チャンネル数）を返す
    fn info(&self) -> StreamInfo;
    /// f32 インターリーブ形式で、最大 buf.len()/channels フレームを読み込む。
    /// 戻り値は読み込んだフレーム数。0 は EOF を示す。
    fn read(&mut self, buf: &mut [f32]) -> Result<usize, DecodeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error(transparent)]
    CoreAudio(#[from] CoreAudioError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Codec(String),
    #[error("no audio track")]
    NoAudioTrack,
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// 拡張子（小文字化）に基づいて適切なデコーダを返す
pub fn open(path: &Path) -> Result<Box<dyn Decoder>, DecodeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "ogg" | "oga" => Err(DecodeError::Unsupported(
            "ogg vorbis decoder not implemented yet".into(),
        )),
        _ => Ok(Box::new(ext::ExtDecoder::open(path)?)),
    }
}
