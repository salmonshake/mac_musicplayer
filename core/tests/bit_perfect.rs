//! Bit-perfect mode against the real default output device.
//!
//! Lives in its own test binary so the device rate changes don't disturb the
//! timing-sensitive engine tests. Plays generated silence (volume is fixed at
//! 100% in this mode) and restores the device rate afterwards.

use std::path::Path;
use std::time::{Duration, Instant};

use mmp_core::coreaudio::device;
use mmp_core::player::{PlayState, Player, PlayerStatus};

/// Writes `secs` seconds of 16-bit stereo silence at `rate` Hz.
fn write_silent_wav(path: &Path, rate: u32, secs: u32) {
    let frames = rate * secs;
    let data_len = frames * 4;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // channels
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 4).to_le_bytes()); // byte rate
    b.extend_from_slice(&4u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    b.resize(44 + data_len as usize, 0);
    std::fs::write(path, b).unwrap();
}

fn wait_for(
    p: &Player,
    timeout: Duration,
    pred: impl Fn(&PlayerStatus) -> bool,
) -> Option<PlayerStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        let s = p.status();
        if pred(&s) {
            return Some(s);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_rate(id: u32, rate: f64, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if device::nominal_sample_rate(id).is_ok_and(|r| (r - rate).abs() < 0.5) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn device_rate_follows_track_and_is_restored() {
    let Ok(id) = device::default_output_device() else {
        eprintln!("skip: no default output device");
        return;
    };
    if !device::transport_type(id).is_ok_and(device::bit_perfect_capable) {
        eprintln!("skip: default output is Bluetooth/AirPlay");
        return;
    }
    let ranges = device::available_sample_rates(id).unwrap();
    if !(device::supports_rate(&ranges, 44_100.0) && device::supports_rate(&ranges, 48_000.0)) {
        eprintln!("skip: device lacks 44.1/48 kHz ({ranges:?})");
        return;
    }
    let original = device::nominal_sample_rate(id).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("silence_44k.wav");
    write_silent_wav(&wav, 44_100, 3);

    // Start from 48 kHz so the switch is observable.
    device::set_nominal_sample_rate(id, 48_000.0).unwrap();

    let p = Player::new(None);
    p.set_volume(0.3);
    p.set_output_mode(true, false);
    p.play_queue(vec![wav], 0);
    let s = wait_for(&p, Duration::from_secs(3), |s| {
        s.state == PlayState::Playing && s.device_rate == Some(44_100.0)
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let s = s.expect("device should switch to the track rate (44.1 kHz)");
        assert!(s.bit_perfect);
        assert!(s.bit_perfect_available);
        // The user's volume is remembered, not applied.
        assert!((s.volume - 0.3).abs() < 1e-6, "volume {}", s.volume);
        assert!(wait_rate(id, 44_100.0, Duration::from_millis(500)));

        // Leaving bit-perfect puts the device back to the rate it had.
        p.set_output_mode(false, false);
        assert!(
            wait_rate(id, 48_000.0, Duration::from_secs(2)),
            "rate should be restored to 48 kHz, is {:?}",
            device::nominal_sample_rate(id)
        );
        let s = wait_for(&p, Duration::from_secs(1), |s| !s.bit_perfect).expect("mode off");
        assert_eq!(
            s.state,
            PlayState::Playing,
            "playback continues after the switch"
        );
    }));
    drop(p);
    let _ = device::set_nominal_sample_rate(id, original);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
