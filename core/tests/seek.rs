use std::path::PathBuf;

use maplay_core::decode::open;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("testdata")
        .join(name)
}

fn check_seek(name: &str) {
    let path = testdata(name);
    if !path.exists() {
        eprintln!("skip: {} not found", path.display());
        return;
    }
    let mut dec = open(&path).expect("open");
    let ch = dec.info().channels as usize;
    dec.seek(66150).expect("seek");
    let mut buf = vec![0.0f32; 4096 * ch];
    let mut total = 0usize;
    loop {
        let n = dec.read(&mut buf).expect("read");
        if n == 0 {
            break;
        }
        total += n;
    }
    assert_eq!(total, 66150, "{name}");

    dec.seek(441000).expect("seek");
    assert_eq!(dec.read(&mut buf).expect("read"), 0, "{name} past end");
}

#[test]
fn seek_flac() {
    check_seek("sine440_44k16_stereo.flac");
}

#[test]
fn seek_mp3() {
    check_seek("sine440.mp3");
}

#[test]
fn seek_aac() {
    check_seek("sine440_aac.m4a");
}

#[test]
fn seek_ogg() {
    check_seek("sine440.ogg");
}

#[test]
fn seek_heaac() {
    // length_frames() is in base-layer (22.05 kHz) frames for HE-AAC; seeking
    // must still use 44.1 kHz client frames.
    check_seek("sine440_heaac.m4a");
}
