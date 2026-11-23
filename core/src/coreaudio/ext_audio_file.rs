use crate::coreaudio::error::{CoreAudioError, Result, check};
use objc2_audio_toolbox::{
    AudioFileGetProperty, AudioFileGetPropertyInfo, AudioFileID, ExtAudioFileDispose,
    ExtAudioFileGetProperty, ExtAudioFileOpenURL, ExtAudioFileRead, ExtAudioFileRef,
    ExtAudioFileSeek, ExtAudioFileSetProperty, kAudioFilePropertyFormatList,
    kExtAudioFileProperty_AudioFile, kExtAudioFileProperty_ClientDataFormat,
    kExtAudioFileProperty_FileDataFormat, kExtAudioFileProperty_FileLengthFrames,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioFormatListItem, AudioStreamBasicDescription,
    kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM,
};
use objc2_core_foundation::CFURL;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::{NonNull, null_mut};

/// Information about the audio file's format.
#[derive(Debug, Clone, Copy)]
pub struct StreamInfo {
    pub sample_rate: f64,
    pub channels: u32,
}

/// A safe wrapper around `ExtAudioFileRef`.
#[derive(Debug)]
pub struct ExtAudioFile {
    raw: ExtAudioFileRef,
    client_channels: u32,
}

impl ExtAudioFile {
    /// Opens an audio file from the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let url = CFURL::from_file_path(path)
            .ok_or_else(|| CoreAudioError::InvalidPath(path.to_path_buf()))?;
        let mut raw = null_mut();
        // SAFETY: ExtAudioFileOpenURL is the standard way to open an audio file.
        unsafe {
            check(
                ExtAudioFileOpenURL(&url, NonNull::from(&mut raw)),
                "ExtAudioFileOpenURL",
            )?;
        }
        if raw.is_null() {
            return Err(CoreAudioError::Unsupported(
                "ExtAudioFileOpenURL returned null".into(),
            ));
        }

        Ok(Self {
            raw,
            client_channels: 0,
        })
    }

    fn first_format_from_list(&self) -> Option<AudioStreamBasicDescription> {
        let mut af: AudioFileID = null_mut();
        let mut size = std::mem::size_of::<AudioFileID>() as u32;
        // SAFETY: ExtAudioFileGetProperty is safe with valid raw.
        unsafe {
            if check(
                ExtAudioFileGetProperty(
                    self.raw,
                    kExtAudioFileProperty_AudioFile,
                    NonNull::from(&mut size),
                    NonNull::from(&mut af).cast::<c_void>(),
                ),
                "ExtAudioFileGetProperty (AudioFileID)",
            )
            .is_err()
                || af.is_null()
            {
                return None;
            }
        }

        let mut list_size: u32 = 0;
        // SAFETY: AudioFileGetPropertyInfo is safe with valid af and property.
        unsafe {
            if check(
                AudioFileGetPropertyInfo(
                    af,
                    kAudioFilePropertyFormatList,
                    &mut list_size,
                    null_mut(),
                ),
                "AudioFileGetPropertyInfo",
            )
            .is_err()
            {
                return None;
            }
        }

        let n = list_size as usize / std::mem::size_of::<AudioFormatListItem>();
        if n == 0 {
            return None;
        }

        // SAFETY: We pre-allocate and initialize with zeros.
        let mut items: Vec<AudioFormatListItem> =
            (0..n).map(|_| unsafe { std::mem::zeroed() }).collect();

        let mut io_size = list_size;
        let ptr = NonNull::new(items.as_mut_ptr().cast::<c_void>())?;
        // SAFETY: `items` holds `n` AudioFormatListItem slots, i.e. `io_size` bytes.
        let status = unsafe {
            AudioFileGetProperty(
                af,
                kAudioFilePropertyFormatList,
                NonNull::from(&mut io_size),
                ptr,
            )
        };
        if check(status, "AudioFileGetProperty (FormatList)").is_err() {
            return None;
        }

        items.first().map(|it| it.mASBD)
    }

    /// Returns the input file format information.
    pub fn file_info(&self) -> Result<StreamInfo> {
        // Prefer the first (highest-quality) entry of the format list so HE-AAC
        // reports its SBR output rate rather than the half-rate base layer.
        if let Some(asbd) = self.first_format_from_list()
            && asbd.mChannelsPerFrame > 0
            && asbd.mSampleRate > 0.0
        {
            return Ok(StreamInfo {
                sample_rate: asbd.mSampleRate,
                channels: asbd.mChannelsPerFrame,
            });
        }

        // SAFETY: AudioStreamBasicDescription is a plain C struct; all-zero is a valid value.
        let mut format_asb: AudioStreamBasicDescription = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
        unsafe {
            // SAFETY: We use the correct property ID and buffer size.
            check(
                ExtAudioFileGetProperty(
                    self.raw,
                    kExtAudioFileProperty_FileDataFormat,
                    NonNull::from(&mut size),
                    NonNull::from(&mut format_asb).cast::<c_void>(),
                ),
                "ExtAudioFileGetProperty (FileDataFormat)",
            )?;
        }

        let sample_rate = format_asb.mSampleRate as f64;
        let channels = format_asb.mChannelsPerFrame;

        if channels == 0 || sample_rate <= 0.0 {
            return Err(CoreAudioError::Unsupported(format!(
                "Invalid file format: sample_rate={} channels={}",
                sample_rate, channels
            )));
        }

        Ok(StreamInfo {
            sample_rate,
            channels,
        })
    }

    /// Returns the total number of frames in the audio file.
    pub fn length_frames(&self) -> Result<i64> {
        let mut length = 0i64;
        let mut size = std::mem::size_of::<i64>() as u32;
        unsafe {
            // SAFETY: We use the correct property ID and buffer size.
            check(
                ExtAudioFileGetProperty(
                    self.raw,
                    kExtAudioFileProperty_FileLengthFrames,
                    NonNull::from(&mut size),
                    NonNull::from(&mut length).cast::<c_void>(),
                ),
                "ExtAudioFileGetProperty (FileLengthFrames)",
            )?;
        }
        Ok(length)
    }

    /// Sets the client data format to 32-bit interleaved float.
    pub fn set_client_f32_interleaved(&mut self, info: &StreamInfo) -> Result<()> {
        // SAFETY: AudioStreamBasicDescription is a plain C struct; all-zero is a valid value.
        let mut asb: AudioStreamBasicDescription = unsafe { std::mem::zeroed() };
        asb.mSampleRate = info.sample_rate;
        asb.mFormatID = kAudioFormatLinearPCM;
        asb.mFormatFlags = kAudioFormatFlagsNativeFloatPacked;
        asb.mBitsPerChannel = 32;
        asb.mChannelsPerFrame = info.channels;
        asb.mBytesPerFrame = 4 * info.channels;
        asb.mFramesPerPacket = 1;
        asb.mBytesPerPacket = 4 * info.channels;

        let size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
        unsafe {
            // SAFETY: We use the correct property ID and buffer size.
            check(
                ExtAudioFileSetProperty(
                    self.raw,
                    kExtAudioFileProperty_ClientDataFormat,
                    size,
                    NonNull::from(&mut asb).cast::<c_void>(),
                ),
                "ExtAudioFileSetProperty (ClientDataFormat)",
            )?;
        }
        self.client_channels = info.channels;
        Ok(())
    }

    /// Reads up to `buf.len() / client_channels` frames from the audio file.
    pub fn read(&mut self, buf: &mut [f32]) -> Result<usize> {
        if self.client_channels == 0 {
            return Err(CoreAudioError::Unsupported("client format not set".into()));
        }

        let ch = self.client_channels;
        let frames_to_read = (buf.len() as u32) / ch;
        if frames_to_read == 0 {
            return Ok(0);
        }

        let mut frames_read = frames_to_read;
        let mut abl = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: ch,
                mDataByteSize: frames_to_read * ch * 4,
                mData: buf.as_mut_ptr().cast(),
            }],
        };

        unsafe {
            // SAFETY: We ensure the buffer is large enough for the requested frames.
            check(
                ExtAudioFileRead(
                    self.raw,
                    NonNull::from(&mut frames_read),
                    NonNull::from(&mut abl),
                ),
                "ExtAudioFileRead",
            )?;
        }

        Ok(frames_read as usize)
    }

    /// Seeks to the given client-format frame.
    pub fn seek(&mut self, frame: i64) -> Result<()> {
        unsafe {
            // SAFETY: ExtAudioFileSeek is safe with a valid raw pointer.
            check(ExtAudioFileSeek(self.raw, frame), "ExtAudioFileSeek")?;
        }
        Ok(())
    }
}

impl Drop for ExtAudioFile {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: raw is checked for null in open.
            let _ = ExtAudioFileDispose(self.raw);
        }
    }
}

unsafe impl Send for ExtAudioFile {}
