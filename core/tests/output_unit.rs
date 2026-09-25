use mmp_core::coreaudio::ext_audio_file::StreamInfo;
use mmp_core::coreaudio::output::OutputUnit;
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
    let id = match mmp_core::coreaudio::device::default_output_device() {
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
