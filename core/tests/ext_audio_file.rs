use mmp_core::coreaudio::ext_audio_file::ExtAudioFile;

#[test]
fn test_open_nonexistent_path() {
    let path = std::path::Path::new("/tmp/nonexistent_file_for_mmp_core_test_12345");
    let result = ExtAudioFile::open(path);
    assert!(result.is_err());
}

#[test]
fn test_open_empty_file() {
    let p = std::env::temp_dir().join("mmp_core_empty_test.flac");
    std::fs::write(&p, b"").unwrap();
    let result = ExtAudioFile::open(&p);

    if let Ok(file) = result {
        assert_eq!(file.length_frames().unwrap(), 0);
    } else {
        // If it returns Err, it's also acceptable according to instructions
    }
    std::fs::remove_file(&p).ok();
}

#[test]
fn test_ext_audio_file_full_flow() {
    let testdata_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(|s| std::path::Path::new(&s).parent().unwrap().join("testdata"))
        .unwrap_or_else(|_| std::path::Path::new("/tmp").to_path_buf());

    let file_path = testdata_dir.join("sine440_44k16_stereo.flac");
    if !file_path.exists() {
        eprintln!("Skipping test: {} does not exist", file_path.display());
        return;
    }

    let mut file = ExtAudioFile::open(&file_path).expect("Failed to open test file");
    let info = file.file_info().expect("Failed to get file info");
    assert_eq!(info.sample_rate, 44100.0);
    assert_eq!(info.channels, 2);

    let total_frames = file.length_frames().expect("Failed to get length frames");
    assert_eq!(total_frames, 132300);

    file.set_client_f32_interleaved(&info).expect("Failed to set client format");

    let mut total_read_frames = 0;
    let mut buf = vec![0.0f32; 4096 * 2];

    loop {
        let n = file.read(&mut buf).expect("read failed");
        if n == 0 {
            break;
        }
        total_read_frames += n as i64;
    }

    assert_eq!(total_read_frames, total_frames);
}
