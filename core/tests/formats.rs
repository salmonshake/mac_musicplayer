use mmp_core::decode;
use std::path::PathBuf;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("testdata")
        .join(name)
}

fn check(name: &str, rate: f64, expected_frames: i64) {
    let p = testdata(name);
    if !p.exists() {
        eprintln!("skip: {} not found", p.display());
        return;
    }
    let mut d = decode::open(&p).unwrap_or_else(|e| panic!("{name}: open failed: {e}"));
    let info = d.info();
    assert_eq!(info.sample_rate, rate, "{name} rate");
    assert_eq!(info.channels, 2, "{name} channels");
    let mut buf = vec![0.0f32; 4096 * 2];
    let mut total: i64 = 0;
    loop {
        let n = d
            .read(&mut buf)
            .unwrap_or_else(|e| panic!("{name}: read failed: {e}"));
        if n == 0 {
            break;
        }
        total += n as i64;
    }
    assert_eq!(total, expected_frames, "{name} frames");
}

#[test]
fn test_flac_44k16() {
    check("sine440_44k16_stereo.flac", 44100.0, 132300)
}

#[test]
fn test_flac_96k24() {
    check("sine1k_96k24_stereo.flac", 96000.0, 192000)
}

#[test]
fn test_wav() {
    check("sine440_44k16.wav", 44100.0, 132300)
}

#[test]
fn test_aiff() {
    check("sine440_44k16.aiff", 44100.0, 132300)
}

#[test]
fn test_mp3() {
    check("sine440.mp3", 44100.0, 132300)
}

#[test]
fn test_ogg() {
    check("sine440.ogg", 44100.0, 132300)
}

#[test]
fn test_alac() {
    check("sine440_alac.m4a", 44100.0, 132300)
}

#[test]
fn test_aac() {
    check("sine440_aac.m4a", 44100.0, 132300)
}

#[test]
fn test_heaac() {
    check("sine440_heaac.m4a", 44100.0, 132300)
}
