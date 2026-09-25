use std::ptr::NonNull;
use std::ffi::c_void;
use objc2_audio_toolbox::{
    AudioComponentDescription, AudioComponentFindNext, AudioComponentInstanceNew, AudioComponentInstanceDispose,
    AudioUnit, AudioUnitInitialize, AudioUnitUninitialize, AudioUnitSetProperty, AudioOutputUnitStart,
    AudioOutputUnitStop, AURenderCallbackStruct, AudioUnitRenderActionFlags, kAudioUnitType_Output,
    kAudioUnitSubType_DefaultOutput, kAudioUnitManufacturer_Apple, kAudioUnitProperty_StreamFormat,
    kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input,
};
use objc2_core_audio_types::{
    AudioBufferList, AudioTimeStamp, kAudioFormatFlagsNativeFloatPacked, kAudioFormatLinearPCM,
    AudioStreamBasicDescription,
};
use crate::coreaudio::error::{check, CoreAudioError, Result};
use crate::coreaudio::ext_audio_file::StreamInfo;
use rtrb::Consumer;

struct RenderState {
    consumer: Consumer<f32>,
}

pub struct OutputUnit {
    unit: AudioUnit,
    state: *mut RenderState,
}

impl OutputUnit {
    /// DefaultOutput を作成し、入力スコープ element 0 に f32 インターリーブ形式を設定、
    /// レンダーコールbackを登録して Initialize する（Start はしない）
    pub fn new(info: &StreamInfo, consumer: Consumer<f32>) -> Result<Self> {
        let mut desc = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_DefaultOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };

        // SAFETY: desc is stack-allocated and remains valid for the duration of the call.
        let comp = unsafe {
            AudioComponentFindNext(std::ptr::null_mut(), NonNull::from(&mut desc))
        };

        if comp.is_null() {
            return Err(CoreAudioError::ComponentNotFound("DefaultOutput"));
        }

        let mut unit = std::ptr::null_mut();
        // SAFETY: unit is a valid pointer to a stack variable.
        unsafe {
            check(
                AudioComponentInstanceNew(comp, NonNull::from(&mut unit)),
                "AudioComponentInstanceNew",
            )?;
        }

        let state = Box::into_raw(Box::new(RenderState { consumer }));

        if let Err(e) = configure(unit, info, state) {
            // SAFETY: unit and state are valid pointers.
            unsafe {
                AudioComponentInstanceDispose(unit);
                drop(Box::from_raw(state));
            }
            return Err(e);
        }

        Ok(Self { unit, state })
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
        check(
            AudioUnitInitialize(unit),
            "AudioUnitInitialize",
        )?;
    }

    Ok(())
}

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
        let slice: &mut [f32] = unsafe {
            std::slice::from_raw_parts_mut(buffer.mData.cast::<f32>(), sample_len)
        };

        let (_filled, rest) = state.consumer.pop_partial_slice(slice);
        rest.fill(0.0);
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
