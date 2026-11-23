use maplay_core::coreaudio::ext_audio_file::StreamInfo;
use maplay_core::coreaudio::output::{OutputUnit, RenderShared, render_into};
use rtrb::RingBuffer;
use std::thread;
use std::time::Duration;

#[test]
fn test_output_unit_lifecycle() {
    let info = StreamInfo {
        sample_rate: 44100.0,
        channels: 2,
    };
    let (_p, c) = RingBuffer::<f32>::new(4096);

    let mut unit = match OutputUnit::new(&info, c, None) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("skipping: no output device: {e}");
            return;
        }
    };

    unit.start().expect("failed to start OutputUnit");

    thread::sleep(Duration::from_millis(200));

    unit.stop().expect("stop failed");
}

#[test]
fn test_output_unit_explicit_device() {
    let id = match maplay_core::coreaudio::device::default_output_device() {
        Ok(id) => id,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };

    let info = StreamInfo {
        sample_rate: 44100.0,
        channels: 2,
    };
    let (_p, c) = RingBuffer::<f32>::new(4096);

    let mut unit = match OutputUnit::new(&info, c, Some(id)) {
        Ok(u) => u,
        Err(e) => panic!("OutputUnit::new with explicit device should succeed: {e}"),
    };

    unit.start().expect("failed to start OutputUnit");

    thread::sleep(Duration::from_millis(200));

    unit.stop().expect("stop failed");
}

// ---- render_into unit tests (no CoreAudio) ----

#[test]
fn test_render_into_volume() {
    let (mut p, mut c) = RingBuffer::<f32>::new(8);
    let _ = p.push_partial_slice(&[1.0_f32; 8]);

    let shared = RenderShared::new();
    shared.set_volume(0.5);

    let mut out = [0.0_f32; 8];
    render_into(&mut out, 2, &mut c, &shared);

    // 4 frames × 2 channels = 8 samples, all 0.5
    for &v in &out {
        assert!((v - 0.5).abs() < 1e-6, "expected 0.5, got {v}");
    }
    assert_eq!(shared.frames_played(), 4);
}

#[test]
fn test_render_into_underrun() {
    let (mut p, mut c) = RingBuffer::<f32>::new(8);
    let _ = p.push_partial_slice(&[1.0_f32; 4]);

    let shared = RenderShared::new();
    let mut out = [0.0_f32; 8];
    render_into(&mut out, 2, &mut c, &shared);

    // 4 samples = 2 stereo frames: out[0..4] come from the ring, out[4..8] are silence
    assert!(out[..4].iter().all(|&s| (s - 1.0).abs() < 1e-6), "{out:?}");
    assert!(out[4..].iter().all(|&s| s == 0.0), "{out:?}");
    assert_eq!(shared.frames_played(), 2);
}

#[test]
fn test_render_into_flush() {
    let (mut p, mut c) = RingBuffer::<f32>::new(8);
    let _ = p.push_partial_slice(&[1.0_f32; 8]);

    let shared = RenderShared::new();
    shared.request_flush();

    let mut out = [0.0_f32; 8];
    render_into(&mut out, 2, &mut c, &shared);

    // All output should be zero (flushed)
    for &v in &out {
        assert!((v - 0.0).abs() < 1e-6, "expected 0.0, got {v}");
    }
    // After flush completes, flush_pending should be false
    assert!(!shared.flush_pending(), "flush should be complete");
    // Ring should be empty
    assert_eq!(c.slots(), 0, "ring should be empty after flush");
    // No frames counted (we discarded them)
    assert_eq!(shared.frames_played(), 0);
}

#[test]
fn test_render_into_set_volume_clamp() {
    let shared = RenderShared::new();
    // set_volume(2.0) should clamp to 1.0
    shared.set_volume(2.0);
    assert!(
        (shared.volume() - 1.0).abs() < 1e-6,
        "volume should be clamped to 1.0"
    );

    // set_volume(-1.0) should clamp to 0.0
    shared.set_volume(-1.0);
    assert!(
        (shared.volume() - 0.0).abs() < 1e-6,
        "volume should be clamped to 0.0"
    );
}

#[test]
fn test_render_into_empty_ring() {
    let (_p, mut c) = RingBuffer::<f32>::new(8);

    let shared = RenderShared::new();
    let mut out = [0.0_f32; 8];
    render_into(&mut out, 2, &mut c, &shared);

    // All output should be zero (underrun on empty ring)
    for &v in &out {
        assert!((v - 0.0).abs() < 1e-6);
    }
    assert_eq!(shared.frames_played(), 0);
}

#[test]
fn test_render_into_mono() {
    let (mut p, mut c) = RingBuffer::<f32>::new(4);
    let _ = p.push_partial_slice(&[2.0_f32, 3.0]);

    let shared = RenderShared::new();
    let mut out = [0.0_f32; 4];
    render_into(&mut out, 1, &mut c, &shared);

    assert!((out[0] - 2.0).abs() < 1e-6);
    assert!((out[1] - 3.0).abs() < 1e-6);
    assert!((out[2] - 0.0).abs() < 1e-6);
    assert!((out[3] - 0.0).abs() < 1e-6);
    assert_eq!(shared.frames_played(), 2);
}

#[test]
fn test_render_into_stereo_no_volume() {
    let (mut p, mut c) = RingBuffer::<f32>::new(8);
    let _ = p.push_partial_slice(&[1.0_f32, 2.0, 3.0, 4.0]);

    let shared = RenderShared::new();
    let mut out = [0.0_f32; 8];
    render_into(&mut out, 2, &mut c, &shared);

    assert!((out[0] - 1.0).abs() < 1e-6);
    assert!((out[1] - 2.0).abs() < 1e-6);
    assert!((out[2] - 3.0).abs() < 1e-6);
    assert!((out[3] - 4.0).abs() < 1e-6);
    assert!((out[4] - 0.0).abs() < 1e-6);
    assert!((out[5] - 0.0).abs() < 1e-6);
    assert!((out[6] - 0.0).abs() < 1e-6);
    assert!((out[7] - 0.0).abs() < 1e-6);
    assert_eq!(shared.frames_played(), 2);
}
