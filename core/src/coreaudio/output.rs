use crate::coreaudio::device::DeviceId;
use crate::coreaudio::error::{CoreAudioError, Result, check};
use crate::coreaudio::ext_audio_file::StreamInfo;
use objc2_audio_toolbox::{
    AURenderCallbackStruct, AudioComponentDescription, AudioComponentFindNext,
    AudioComponentInstanceDispose, AudioComponentInstanceNew, AudioOutputUnitStart,
    AudioOutputUnitStop, AudioUnit, AudioUnitInitialize, AudioUnitRenderActionFlags,
    AudioUnitSetProperty, AudioUnitUninitialize, kAudioOutputUnitProperty_CurrentDevice,
    kAudioUnitManufacturer_Apple, kAudioUnitProperty_SetRenderCallback,
    kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global, kAudioUnitScope_Input,
    kAudioUnitSubType_DefaultOutput, kAudioUnitSubType_HALOutput, kAudioUnitType_Output,
};
use objc2_core_audio_types::{
    AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
    kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM,
};
use rtrb::Consumer;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// オーディオコールバックと共有するアトミック値。
///
/// 音量、flush 状態、再生フレーム数をロック・アロケーションなしで更新できる。
pub struct RenderShared {
    volume_bits: AtomicU32,
    flush: AtomicBool,
    frames_played: AtomicU64,
}

impl RenderShared {
    /// 新規作成。音量は 1.0、flush なし、フレーム数 0。
    pub fn new() -> Self {
        Self {
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            flush: AtomicBool::new(false),
            frames_played: AtomicU64::new(0),
        }
    }

    /// 現在の音量 (0.0 〜 1.0)。
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    /// 音量を設定。0.0 未満は 0.0、1.0 超は 1.0 に丸める。
    pub fn set_volume(&self, v: f32) {
        self.volume_bits
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// フラッシュを要求する。次のコールバックでリングの残りデータを捨てて無音を出力し、
    /// 捨て終わると `flush_pending` は false になる。
    pub fn request_flush(&self) {
        self.flush.store(true, Ordering::Release);
    }

    /// フラッシュが保留中か（リングにデータが残っている間 true）。
    pub fn flush_pending(&self) -> bool {
        self.flush.load(Ordering::Acquire)
    }

    /// リングから取り出して出力したフレーム数の累計。
    pub fn frames_played(&self) -> u64 {
        self.frames_played.load(Ordering::Relaxed)
    }
}

impl Default for RenderShared {
    fn default() -> Self {
        Self::new()
    }
}

struct RenderState {
    consumer: Consumer<f32>,
    shared: Arc<RenderShared>,
    channels: usize,
}

pub struct OutputUnit {
    unit: AudioUnit,
    state: *mut RenderState,
    shared: Arc<RenderShared>,
}

impl OutputUnit {
    /// システムのデフォルト出力デバイスに追従する OutputUnit を作成するか、
    /// 明示的なデバイス ID が指定された場合はそのデバイスに固定して作成する。
    ///
    /// `device` が `None` の場合は `kAudioUnitSubType_DefaultOutput` を使用し、
    /// システムのデフォルト出力デバイスに追従する。
    /// `device` が `Some(id)` の場合は `kAudioUnitSubType_HALOutput` を使用し、
    /// 指定されたデバイスに固定する。
    ///
    /// レンダーコールバックを登録して Initialize する（Start はしない）。
    pub fn new(
        info: &StreamInfo,
        consumer: Consumer<f32>,
        device: Option<DeviceId>,
    ) -> Result<Self> {
        let mut desc = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: if device.is_some() {
                kAudioUnitSubType_HALOutput
            } else {
                kAudioUnitSubType_DefaultOutput
            },
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };

        // SAFETY: desc is stack-allocated and remains valid for the duration of the call.
        let comp =
            unsafe { AudioComponentFindNext(std::ptr::null_mut(), NonNull::from(&mut desc)) };

        if comp.is_null() {
            return Err(CoreAudioError::ComponentNotFound(if device.is_some() {
                "HALOutput"
            } else {
                "DefaultOutput"
            }));
        }

        let mut unit = std::ptr::null_mut();
        // SAFETY: unit is a valid pointer to a stack variable.
        unsafe {
            check(
                AudioComponentInstanceNew(comp, NonNull::from(&mut unit)),
                "AudioComponentInstanceNew",
            )?;
        }

        // SAFETY: If a device ID is provided, we bind the output unit to that device.
        // DeviceId (AudioObjectID) is a u32 that identifies a Core Audio device.
        // The pointer remains valid for the duration of this call, and the unit
        // takes ownership of the device reference.
        if let Some(id) = device {
            let status = unsafe {
                AudioUnitSetProperty(
                    unit,
                    kAudioOutputUnitProperty_CurrentDevice,
                    kAudioUnitScope_Global,
                    0,
                    (&id as *const DeviceId).cast::<c_void>(),
                    std::mem::size_of::<DeviceId>() as u32,
                )
            };
            if let Err(e) = check(status, "AudioUnitSetProperty (CurrentDevice)") {
                // SAFETY: unit is a valid AudioUnit that was just created.
                unsafe { AudioComponentInstanceDispose(unit) };
                return Err(e);
            }
        }

        let shared = Arc::new(RenderShared::new());
        let channels = info.channels as usize;
        let state = Box::into_raw(Box::new(RenderState {
            consumer,
            shared: Arc::clone(&shared),
            channels,
        }));

        if let Err(e) = configure(unit, info, state) {
            // SAFETY: unit and state are valid pointers.
            unsafe {
                AudioComponentInstanceDispose(unit);
                drop(Box::from_raw(state));
            }
            return Err(e);
        }

        Ok(Self {
            unit,
            state,
            shared,
        })
    }

    /// コールバックと共有する `RenderShared` のコピーを返す。
    pub fn shared(&self) -> Arc<RenderShared> {
        Arc::clone(&self.shared)
    }

    pub fn start(&mut self) -> Result<()> {
        // SAFETY: self.unit was initialized and is valid.
        unsafe {
            check(AudioOutputUnitStart(self.unit), "AudioOutputUnitStart")?;
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        // SAFETY: self.unit is valid.
        unsafe {
            check(AudioOutputUnitStop(self.unit), "AudioOutputUnitStop")?;
        }
        Ok(())
    }
}

fn configure(unit: AudioUnit, info: &StreamInfo, state: *mut RenderState) -> Result<()> {
    // SAFETY: zeroing a plain-old-data struct is safe.
    let mut asbd: AudioStreamBasicDescription = unsafe { std::mem::zeroed() };
    asbd.mSampleRate = info.sample_rate;
    asbd.mFormatID = kAudioFormatLinearPCM;
    asbd.mFormatFlags = kAudioFormatFlagsNativeFloatPacked;
    asbd.mBitsPerChannel = 32;
    asbd.mChannelsPerFrame = info.channels;
    asbd.mBytesPerFrame = 4 * info.channels;
    asbd.mFramesPerPacket = 1;
    asbd.mBytesPerPacket = 4 * info.channels;

    // SAFETY: asbd is a valid stack variable.
    unsafe {
        check(
            AudioUnitSetProperty(
                unit,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Input,
                0,
                (&asbd as *const AudioStreamBasicDescription).cast::<c_void>(),
                std::mem::size_of::<AudioStreamBasicDescription>() as u32,
            ),
            "AudioUnitSetProperty (StreamFormat)",
        )?;
    }

    // SAFETY: cb is a valid stack variable.
    unsafe {
        let cb = AURenderCallbackStruct {
            inputProc: Some(render_callback),
            inputProcRefCon: state.cast::<c_void>(),
        };
        check(
            AudioUnitSetProperty(
                unit,
                kAudioUnitProperty_SetRenderCallback,
                kAudioUnitScope_Input,
                0,
                (&cb as *const AURenderCallbackStruct).cast::<c_void>(),
                std::mem::size_of::<AURenderCallbackStruct>() as u32,
            ),
            "AudioUnitSetProperty (SetRenderCallback)",
        )?;
    }

    // SAFETY: unit is valid.
    unsafe {
        check(AudioUnitInitialize(unit), "AudioUnitInitialize")?;
    }

    Ok(())
}

/// オーディオコールバックの処理本体（CoreAudio なしでテストできるよう分離）。
///
/// この関数内ではロック・アロケーション・I/O・print は行わない。
pub fn render_into(
    out: &mut [f32],
    channels: usize,
    consumer: &mut Consumer<f32>,
    shared: &RenderShared,
) {
    if shared.flush.load(Ordering::Acquire) {
        let n = consumer.slots();
        if let Ok(chunk) = consumer.read_chunk(n) {
            chunk.commit_all();
        }
        shared.flush.store(false, Ordering::Release);
        out.fill(0.0);
        return;
    }

    let (filled, rest) = consumer.pop_partial_slice(out);
    let got = filled.len();
    let vol = shared.volume();
    if vol != 1.0 {
        for s in filled.iter_mut() {
            *s *= vol;
        }
    }
    rest.fill(0.0);
    if let Some(frames) = got.checked_div(channels) {
        shared
            .frames_played
            .fetch_add(frames as u64, Ordering::Relaxed);
    }
}

/// Core Audio render callback; runs on the real-time audio thread.
///
/// # Safety
/// `in_ref_con` must point to the live `RenderState` registered with the unit,
/// and `io_data` must be null or a valid `AudioBufferList` supplied by Core Audio.
unsafe extern "C-unwind" fn render_callback(
    in_ref_con: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _ts: NonNull<AudioTimeStamp>,
    _bus: u32,
    _frames: u32,
    io_data: *mut AudioBufferList,
) -> i32 {
    if io_data.is_null() {
        return 0;
    }

    // SAFETY: io_data is a valid pointer from the OS.
    let abl = unsafe { &mut *io_data };
    // SAFETY: in_ref_con is the user data we passed to the callback.
    let state = unsafe { &mut *(in_ref_con.as_ptr() as *mut RenderState) };

    for i in 0..abl.mNumberBuffers as usize {
        // SAFETY: mBuffers is a valid array and mNumberBuffers is a valid count.
        let buffer_ptr = unsafe { abl.mBuffers.as_mut_ptr().add(i) };
        if buffer_ptr.is_null() {
            continue;
        }

        // SAFETY: buffer_ptr is not null and points to a valid buffer.
        let buffer = unsafe { &*buffer_ptr };
        if buffer.mData.is_null() {
            continue;
        }

        let sample_len = (buffer.mDataByteSize as usize) / 4;
        // SAFETY: buffer.mData is valid and sample_len is correct.
        let slice: &mut [f32] =
            unsafe { std::slice::from_raw_parts_mut(buffer.mData.cast::<f32>(), sample_len) };

        render_into(slice, state.channels, &mut state.consumer, &state.shared);
    }

    0
}

impl Drop for OutputUnit {
    fn drop(&mut self) {
        // SAFETY: unit and state are valid pointers.
        unsafe {
            let _ = AudioOutputUnitStop(self.unit);
            let _ = AudioUnitUninitialize(self.unit);
            let _ = AudioComponentInstanceDispose(self.unit);
            let _ = Box::from_raw(self.state);
        }
    }
}

// SAFETY: OutputUnit owns the AudioUnit and the RenderState.
// The callback uses the pointer to RenderState, but the lifetime of the unit
// is longer than the callback execution (it's stopped before drop).
unsafe impl Send for OutputUnit {}
