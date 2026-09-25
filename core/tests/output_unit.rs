#[cfg(test)]
mod tests {
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

        let mut unit = OutputUnit::new(&info, c).expect("failed to create OutputUnit");

        unit.start().expect("failed to start OutputUnit");

        thread::sleep(Duration::from_millis(200));

        unit.stop().expect("stop failed");
    }
}
