use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectSetPropertyData,
    kAudioDevicePropertyAvailableNominalSampleRates, kAudioDevicePropertyDeviceIsAlive,
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyHogMode,
    kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertyStreamConfiguration,
    kAudioDevicePropertyTransportType, kAudioDeviceTransportTypeAirPlay,
    kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
};
use objc2_core_audio_types::{AudioBufferList, AudioValueRange};
use objc2_core_foundation::{CFRetained, CFString};
use std::time::{Duration, Instant};

use crate::coreaudio::error::{CoreAudioError, Result, check};

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

    /// Returns `true` if bit-perfect playback can work on this device.
    pub fn is_bit_perfect_capable(&self) -> bool {
        bit_perfect_capable(self.transport_type)
    }
}

/// Bit-perfect output needs a wired/local device whose rate we can set;
/// Bluetooth and AirPlay re-encode the stream, so they are excluded.
pub fn bit_perfect_capable(transport_type: u32) -> bool {
    transport_type != kAudioDeviceTransportTypeBluetooth
        && transport_type != kAudioDeviceTransportTypeBluetoothLE
        && transport_type != kAudioDeviceTransportTypeAirPlay
}

/// Whether `rate` lies in one of the device's supported (min, max) ranges.
pub fn supports_rate(ranges: &[(f64, f64)], rate: f64) -> bool {
    ranges
        .iter()
        .any(|&(lo, hi)| rate >= lo - 0.5 && rate <= hi + 0.5)
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
    let mut a = addr(
        kAudioDevicePropertyStreamConfiguration,
        kAudioObjectPropertyScopeOutput,
    );
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
    let mut a = addr(
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
    );
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
    let nominal_sample_rate = get_f64(
        id,
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
    )
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
    Ok(device_ids()?.into_iter().filter_map(device_info).collect())
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

/// Writes a plain-data property value.
fn set_value<T>(id: AudioObjectID, selector: u32, v: &mut T, context: &'static str) -> Result<()> {
    let mut a = addr(selector, kAudioObjectPropertyScopeGlobal);
    // SAFETY: `v` points to a live `T` of `size_of::<T>()` bytes, which is the
    // data layout the caller's selector expects.
    unsafe {
        check(
            AudioObjectSetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                size_of::<T>() as u32,
                NonNull::from(v).cast::<c_void>(),
            ),
            context,
        )
    }
}

/// Returns the device's transport type (built-in, USB, Bluetooth, ...).
pub fn transport_type(id: DeviceId) -> Result<u32> {
    get_u32(
        id,
        kAudioDevicePropertyTransportType,
        kAudioObjectPropertyScopeGlobal,
    )
}

/// Returns the device's current nominal sample rate.
pub fn nominal_sample_rate(id: DeviceId) -> Result<f64> {
    get_f64(
        id,
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
    )
}

/// Returns the nominal sample rates the device supports as (min, max) ranges
/// (discrete rates have min == max).
pub fn available_sample_rates(id: DeviceId) -> Result<Vec<(f64, f64)>> {
    let mut a = addr(
        kAudioDevicePropertyAvailableNominalSampleRates,
        kAudioObjectPropertyScopeGlobal,
    );
    let mut size: u32 = 0;
    // SAFETY: valid property address on a device object.
    unsafe {
        check(
            AudioObjectGetPropertyDataSize(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
            ),
            "AudioObjectGetPropertyDataSize (available rates)",
        )?;
    }
    let n = size as usize / size_of::<AudioValueRange>();
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut ranges = vec![
        AudioValueRange {
            mMinimum: 0.0,
            mMaximum: 0.0
        };
        n
    ];
    // SAFETY: `ranges` holds `n * sizeof(AudioValueRange)` = `size` bytes.
    unsafe {
        check(
            AudioObjectGetPropertyData(
                id,
                NonNull::from(&mut a),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(ranges.as_mut_slice()).cast::<c_void>(),
            ),
            "AudioObjectGetPropertyData (available rates)",
        )?;
    }
    ranges.truncate(size as usize / size_of::<AudioValueRange>());
    Ok(ranges.iter().map(|r| (r.mMinimum, r.mMaximum)).collect())
}

/// How long to wait for the HAL to apply a nominal rate change.
const RATE_CHANGE_TIMEOUT: Duration = Duration::from_secs(1);

/// Sets the device's nominal sample rate and waits until it takes effect
/// (the change is asynchronous). No-op if the rate is already current.
pub fn set_nominal_sample_rate(id: DeviceId, rate: f64) -> Result<()> {
    if (nominal_sample_rate(id)? - rate).abs() < 0.5 {
        return Ok(());
    }
    let mut v = rate;
    set_value(
        id,
        kAudioDevicePropertyNominalSampleRate,
        &mut v,
        "AudioObjectSetPropertyData (nominal rate)",
    )?;
    let deadline = Instant::now() + RATE_CHANGE_TIMEOUT;
    loop {
        if (nominal_sample_rate(id)? - rate).abs() < 0.5 {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(CoreAudioError::Unsupported(format!(
                "device did not switch to {rate} Hz"
            )));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The pid that holds exclusive (hog) access to the device, or -1 if none.
pub fn hog_owner(id: DeviceId) -> Result<i32> {
    // pid_t is a 32-bit signed integer; read the raw bits.
    get_u32(
        id,
        kAudioDevicePropertyHogMode,
        kAudioObjectPropertyScopeGlobal,
    )
    .map(|v| v as i32)
}

/// Takes (`on`) or releases exclusive access to the device for this process.
///
/// The HAL property is a toggle — writing it acquires when free and releases
/// when we own it — so the current owner is checked first.
pub fn set_hog(id: DeviceId, on: bool) -> Result<()> {
    let me = std::process::id() as i32;
    let owner = hog_owner(id)?;
    if (owner == me) == on {
        return Ok(());
    }
    if on && owner != -1 {
        return Err(CoreAudioError::Unsupported(format!(
            "device is in exclusive use by process {owner}"
        )));
    }
    let mut v: i32 = me;
    set_value(
        id,
        kAudioDevicePropertyHogMode,
        &mut v,
        "AudioObjectSetPropertyData (hog mode)",
    )?;
    if (hog_owner(id)? == me) == on {
        Ok(())
    } else {
        Err(CoreAudioError::Unsupported(
            "hog mode change was not applied".into(),
        ))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_rate_discrete_and_range() {
        let discrete = [(44_100.0, 44_100.0), (48_000.0, 48_000.0)];
        assert!(supports_rate(&discrete, 44_100.0));
        assert!(supports_rate(&discrete, 48_000.0));
        assert!(!supports_rate(&discrete, 96_000.0));
        let range = [(8_000.0, 192_000.0)];
        assert!(supports_rate(&range, 88_200.0));
        assert!(!supports_rate(&range, 384_000.0));
        assert!(!supports_rate(&[], 44_100.0));
    }

    #[test]
    fn bit_perfect_excludes_wireless() {
        assert!(!bit_perfect_capable(kAudioDeviceTransportTypeBluetooth));
        assert!(!bit_perfect_capable(kAudioDeviceTransportTypeBluetoothLE));
        assert!(!bit_perfect_capable(kAudioDeviceTransportTypeAirPlay));
        // 'bltn' (built-in) and 'usb ' are fine.
        assert!(bit_perfect_capable(0x626c_746e));
        assert!(bit_perfect_capable(0x7573_6220));
    }
}
