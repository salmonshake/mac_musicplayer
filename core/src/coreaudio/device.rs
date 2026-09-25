use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioObjectID, AudioObjectPropertyAddress, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, kAudioDevicePropertyDeviceIsAlive,
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertyStreamConfiguration, kAudioDevicePropertyTransportType,
    kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput,
    kAudioObjectPropertyName, kAudioObjectPropertyElementMain,
    kAudioObjectSystemObject,
};
use objc2_core_audio_types::AudioBufferList;
use objc2_core_foundation::{CFRetained, CFString};

use crate::coreaudio::error::{check, CoreAudioError, Result};

/// A device identifier (Core Audio `AudioObjectID`).
pub type DeviceId = u32;

/// Information about an output device.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// The Core Audio object ID for this device.
    pub id: DeviceId,
    /// Unique identifier string for the device.
    pub uid: String,
    /// Human-readable name of the device.
    pub name: String,
    /// Number of output channels.
    pub output_channels: u32,
    /// Nominal sample rate in Hz.
    pub nominal_sample_rate: f64,
    /// Transport type (raw u32 from Core Audio).
    pub transport_type: u32,
}

impl DeviceInfo {
    /// Returns `true` if the device is connected via Bluetooth or Bluetooth LE.
    pub fn is_bluetooth(&self) -> bool {
        self.transport_type == kAudioDeviceTransportTypeBluetooth
            || self.transport_type == kAudioDeviceTransportTypeBluetoothLE
    }
}

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

fn addr(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn get_u32(id: AudioObjectID, selector: u32, scope: u32) -> Result<u32> {
    let mut a = addr(selector, scope);
    let mut v: u32 = 0;
    let mut size: u32 = size_of::<u32>() as u32;
    // SAFETY: AudioObjectGetPropertyData is called with a valid property
    // address pointing to a u32 buffer of the correct size.
    unsafe {
        check(
            AudioObjectGetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut v).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (get_u32)",
        )?;
    }
    Ok(v)
}

fn get_f64(id: AudioObjectID, selector: u32, scope: u32) -> Result<f64> {
    let mut a = addr(selector, scope);
    let mut v: f64 = 0.0;
    let mut size: u32 = size_of::<f64>() as u32;
    // SAFETY: AudioObjectGetPropertyData is called with a valid property
    // address pointing to an f64 buffer of the correct size.
    unsafe {
        check(
            AudioObjectGetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut v).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (get_f64)",
        )?;
    }
    Ok(v)
}

fn get_string(id: AudioObjectID, selector: u32) -> Result<String> {
    let mut a = addr(selector, kAudioObjectPropertyScopeGlobal);
    let mut raw: *const CFString = ptr::null();
    let mut size: u32 = size_of::<*const CFString>() as u32;
    // SAFETY: AudioObjectGetPropertyData is called with a valid property
    // address pointing to a *const CFString buffer.
    unsafe {
        check(
            AudioObjectGetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut raw).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (get_string)",
        )?;
    }
    let nn = NonNull::new(raw as *mut CFString)
        .ok_or_else(|| CoreAudioError::Unsupported("null CFString".into()))?;
    // SAFETY: Core Audio returns a CFStringRef that the caller owns.
    // We take ownership with from_raw.
    let s = unsafe { CFRetained::from_raw(nn) };
    Ok(s.to_string())
}

fn output_channels(id: AudioObjectID) -> Result<u32> {
    let mut a =
        addr(kAudioDevicePropertyStreamConfiguration, kAudioObjectPropertyScopeOutput);
    // SAFETY: AudioObjectGetPropertyDataSize is called with a valid property
    // address for a device object.
    let mut size: u32 = 0;
    unsafe {
        check(
            AudioObjectGetPropertyDataSize(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
            ),
            "AudioObjectGetPropertyDataSize (stream_config)",
        )?;
    }
    if size == 0 {
        return Ok(0);
    }
    // AudioBufferList is 8-byte aligned, so we round up.
    let n = (size as usize).div_ceil(8);
    let mut storage: Vec<u64> = vec![0; n];
    // SAFETY: `storage` holds `size` bytes, 8-byte aligned (Vec<u64>).
    unsafe {
        check(
            AudioObjectGetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                // Point at the buffer contents, not at the Vec header.
                NonNull::from(storage.as_mut_slice()).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (stream_config)",
        )?;
    }
    let abl = storage.as_ptr() as *const AudioBufferList;
    // SAFETY: `storage` holds `size` bytes starting with a valid AudioBufferList
    // whose mNumberBuffers field gives the number of buffers.
    let n = unsafe { (*abl).mNumberBuffers as usize };
    // SAFETY: `storage` holds at least `size` bytes; the first `size` bytes
    // contain an AudioBufferList with `mNumberBuffers` entries, each
    // being an AudioBuffer (12 bytes). The slice is valid for `n` items.
    let bufs = unsafe { std::slice::from_raw_parts((*abl).mBuffers.as_ptr(), n) };
    Ok(bufs.iter().map(|b| b.mNumberChannels).sum())
}

/// Returns all device IDs known to the system.
fn device_ids() -> Result<Vec<AudioObjectID>> {
    let mut a = addr(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal);
    // SAFETY: AudioObjectGetPropertyDataSize is called on the system object
    // with a valid property address.
    let mut size: u32 = 0;
    unsafe {
        check(
            AudioObjectGetPropertyDataSize(
                SYSTEM,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
            ),
            "AudioObjectGetPropertyDataSize (devices)",
        )?;
    }
    let n = size as usize / size_of::<AudioObjectID>();
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut ids: Vec<u32> = vec![0; n];
    // SAFETY: `ids` holds `n * sizeof(AudioObjectID)` bytes = `size` bytes.
    unsafe {
        check(
            AudioObjectGetPropertyData(
                SYSTEM,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                // Point at the buffer contents, not at the Vec header.
                NonNull::from(ids.as_mut_slice()).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (devices)",
        )?;
    }
    // The device list can shrink between the size query and the fetch.
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    Ok(ids)
}

/// Retrieves information for a single device. Returns `None` if the device
/// has no output channels or an empty UID.
fn device_info(id: AudioObjectID) -> Option<DeviceInfo> {
    let uid = get_string(id, kAudioDevicePropertyDeviceUID).ok()?;
    if uid.is_empty() {
        return None;
    }
    let name = get_string(id, kAudioObjectPropertyName).unwrap_or_default();
    let output_channels = output_channels(id).ok()?;
    if output_channels == 0 {
        return None;
    }
    let nominal_sample_rate =
        get_f64(id, kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal)
            .ok();
    let transport_type = get_u32(
        id,
        kAudioDevicePropertyTransportType,
        kAudioObjectPropertyScopeGlobal,
    )
    .ok();

    Some(DeviceInfo {
        id,
        uid,
        name,
        output_channels,
        nominal_sample_rate: nominal_sample_rate.unwrap_or(0.0),
        transport_type: transport_type.unwrap_or(0),
    })
}

/// Returns all output devices (those with at least one output channel).
pub fn output_devices() -> Result<Vec<DeviceInfo>> {
    Ok(device_ids()?
        .into_iter()
        .filter_map(device_info)
        .collect())
}

/// Returns the default output device ID.
pub fn default_output_device() -> Result<DeviceId> {
    get_u32(
        SYSTEM,
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
    )
}

/// Finds an output device by its UID string.
pub fn find_output_device_by_uid(uid: &str) -> Result<Option<DeviceInfo>> {
    Ok(output_devices()?.into_iter().find(|d| d.uid == uid))
}

/// Returns `true` if the device is alive. Returns `false` on any error.
pub fn is_alive(id: DeviceId) -> bool {
    get_u32(
        id,
        kAudioDevicePropertyDeviceIsAlive,
        kAudioObjectPropertyScopeGlobal,
    )
    .map(|v| v != 0)
    .unwrap_or(false)
}
