use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc::{Receiver, Sender, channel};

use objc2_core_audio::{
    AudioObjectAddPropertyListener, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectRemovePropertyListener, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};

use crate::coreaudio::error::{Result, check};

/// The Core Audio system object used to register property listeners.
const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

/// Events emitted when device topology changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    /// One or more devices were added or removed.
    DevicesChanged,
    /// The default output device changed.
    DefaultOutputChanged,
}

/// Builds a property address for a given selector on the global scope,
/// main element.
fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Core Audio property listener callback.
///
/// This callback runs on the Core Audio notification thread. It only calls
/// `Sender::send`, which does not block; a send error (the receiver was
/// dropped) is ignored.
///
/// # Safety
/// The `client_data` pointer must point to a live `Sender<DeviceEvent>`
/// allocated with `Box::into_raw` and not yet dropped.
unsafe extern "C-unwind" fn listener(
    _id: AudioObjectID,
    count: u32,
    addresses: NonNull<AudioObjectPropertyAddress>,
    client_data: *mut c_void,
) -> i32 {
    if client_data.is_null() {
        return 0;
    }
    // SAFETY: `client_data` is the `Box<Sender>` leaked by `DeviceWatcher::new`
    // (checked non-null above) and is freed only after the listener is removed.
    // Only a shared reference is taken; `Sender::send` needs just `&self`.
    let sender = unsafe { &*(client_data as *const Sender<DeviceEvent>) };
    // SAFETY: Core Audio passes `count` valid property addresses at `addresses`
    // for the duration of this callback.
    let addrs = unsafe { std::slice::from_raw_parts(addresses.as_ptr(), count as usize) };
    for a in addrs {
        let ev = if a.mSelector == kAudioHardwarePropertyDevices {
            Some(DeviceEvent::DevicesChanged)
        } else if a.mSelector == kAudioHardwarePropertyDefaultOutputDevice {
            Some(DeviceEvent::DefaultOutputChanged)
        } else {
            None
        };
        if let Some(ev) = ev {
            let _ = sender.send(ev);
        }
    }
    0
}

/// Watches Core Audio for device-add/remove and default-output-change events.
///
/// Call `new()` to register listeners; the returned `DeviceWatcher` must stay
/// alive for the duration of the watch. Dropping it removes all listeners.
pub struct DeviceWatcher {
    sender: *mut Sender<DeviceEvent>,
}

impl DeviceWatcher {
    /// Registers property listeners for device topology changes and default
    /// output device changes, then returns a watcher and a receiver channel.
    ///
    /// The caller should keep the `DeviceWatcher` alive for as long as
    /// notifications are desired. Dropping the watcher removes both listeners.
    pub fn new() -> Result<(Self, Receiver<DeviceEvent>)> {
        let (tx, rx) = channel();
        let sender = Box::into_raw(Box::new(tx));
        let mut a1 = address(kAudioHardwarePropertyDevices);
        // SAFETY: `a1` outlives the call, `listener` has the signature Core Audio
        // expects, and `sender` stays valid (leaked by `Box::into_raw`) until it
        // is freed after the listener has been removed.
        let s1 = unsafe {
            AudioObjectAddPropertyListener(
                SYSTEM,
                NonNull::from(&mut a1),
                Some(listener),
                sender.cast::<c_void>(),
            )
        };
        if let Err(e) = check(s1, "AudioObjectAddPropertyListener (Devices)") {
            // The first registration failed, so Core Audio holds no pointer to
            // `sender`; the remove call is only a precaution (its result is ignored).
            // SAFETY: same arguments as the add above. `sender` came from
            // `Box::into_raw` and is freed exactly once, on this early return.
            unsafe {
                AudioObjectRemovePropertyListener(
                    SYSTEM,
                    NonNull::from(&mut a1),
                    Some(listener),
                    sender.cast::<c_void>(),
                );
                drop(Box::from_raw(sender));
            }
            return Err(e);
        }
        let mut a2 = address(kAudioHardwarePropertyDefaultOutputDevice);
        // SAFETY: as for the first registration.
        let s2 = unsafe {
            AudioObjectAddPropertyListener(
                SYSTEM,
                NonNull::from(&mut a2),
                Some(listener),
                sender.cast::<c_void>(),
            )
        };
        if let Err(e) = check(s2, "AudioObjectAddPropertyListener (DefaultOutputDevice)") {
            // Second registration failed — remove the first listener too.
            // SAFETY: the first listener is registered with exactly these
            // arguments. `sender` is freed once, on this early return, after the
            // listener that referenced it has been removed.
            unsafe {
                AudioObjectRemovePropertyListener(
                    SYSTEM,
                    NonNull::from(&mut a1),
                    Some(listener),
                    sender.cast::<c_void>(),
                );
                drop(Box::from_raw(sender));
            }
            return Err(e);
        }
        Ok((Self { sender }, rx))
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        let mut a1 = address(kAudioHardwarePropertyDevices);
        let mut a2 = address(kAudioHardwarePropertyDefaultOutputDevice);
        // Ignore return values — we are tearing down.
        //
        // SAFETY: both listeners were registered in `new` with these arguments,
        // and `self.sender` came from `Box::into_raw`; it is freed here exactly
        // once, after the listeners are removed. This relies on Core Audio not
        // delivering a notification once `AudioObjectRemovePropertyListener` has
        // returned — Apple documents the unregistration but does not spell out
        // waiting for a callback that is already running.
        unsafe {
            AudioObjectRemovePropertyListener(
                SYSTEM,
                NonNull::from(&mut a1),
                Some(listener),
                self.sender.cast::<c_void>(),
            );
            AudioObjectRemovePropertyListener(
                SYSTEM,
                NonNull::from(&mut a2),
                Some(listener),
                self.sender.cast::<c_void>(),
            );
            drop(Box::from_raw(self.sender));
        }
    }
}

// SAFETY: The inner Sender<DeviceEvent> is Send, and the raw pointer
// is managed exclusively by DeviceWatcher (no aliasing).
unsafe impl Send for DeviceWatcher {}
