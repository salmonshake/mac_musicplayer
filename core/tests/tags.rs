//! Tests for tag reading (Task 5.2).

use mmp_core::library::{LibraryError, read_meta};
use std::path::PathBuf;

fn lib_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/lib")
}

fn check_lib_exists() -> Option<PathBuf> {
    let p = lib_path();
    if !p.is_dir() {
        eprintln!("testdata/lib not found, skipping tag tests");
        return None;
    }
    Some(p)
}

#[test]
fn test_flac_with_tags() {
    let base = match check_lib_exists() {
        Some(p) => p,
        None => return,
    };
    let path = base.join("AlbumA").join("01.flac");
    if !path.is_file() {
        eprintln!("AlbumA/01.flac not found, skipping");
        return;
    }
    let meta = read_meta(&path).expect("read_meta should succeed for 01.flac");

    assert_eq!(meta.title, "一曲目");
    assert_eq!(meta.artist, "テスト歌手");
    assert_eq!(meta.album_artist, "テスト歌手");
    assert_eq!(meta.album, "テストアルバム");
    assert_eq!(meta.genre, "Rock");
    assert_eq!(meta.track_no, Some(1));
    assert_eq!(meta.disc_no, Some(1));
    assert_eq!(meta.year, Some(2024));
    assert_eq!(meta.sample_rate, Some(44100));
    assert_eq!(meta.channels, Some(2));
    assert!(
        meta.duration_ms >= 950,
        "duration_ms {} should be >= 950",
        meta.duration_ms
    );
    assert!(
        meta.duration_ms <= 1050,
        "duration_ms {} should be <= 1050",
        meta.duration_ms
    );
}

#[test]
fn test_mp3_no_album_artist() {
    let base = match check_lib_exists() {
        Some(p) => p,
        None => return,
    };
    let path = base.join("AlbumA").join("02.mp3");
    if !path.is_file() {
        eprintln!("AlbumA/02.mp3 not found, skipping");
        return;
    }
    let meta = read_meta(&path).expect("read_meta should succeed for 02.mp3");

    assert_eq!(meta.title, "二曲目");
    assert_eq!(meta.artist, "テスト歌手");
    assert_eq!(meta.album, "テストアルバム");
    assert_eq!(meta.album_artist, "");
    assert_eq!(meta.effective_album_artist(), "テスト歌手");
    assert_eq!(meta.track_no, Some(2));
    assert_eq!(meta.year, Some(2024));
}

#[test]
fn test_no_tag_file() {
    let base = match check_lib_exists() {
        Some(p) => p,
        None => return,
    };
    let path = base.join("notag.flac");
    if !path.is_file() {
        eprintln!("notag.flac not found, skipping");
        return;
    }
    let meta = read_meta(&path).expect("read_meta should succeed for notag.flac");

    assert_eq!(meta.title, "notag");
    assert_eq!(meta.artist, "");
}

#[test]
fn test_broken_file_returns_err() {
    let base = match check_lib_exists() {
        Some(p) => p,
        None => return,
    };
    let path = base.join("broken.mp3");
    if !path.is_file() {
        eprintln!("broken.mp3 not found, skipping");
        return;
    }
    let result = read_meta(&path);
    assert!(
        result.is_err(),
        "read_meta for broken.mp3 should return Err, not Ok"
    );
    // Verify it's a Tag error, not a panic
    match result {
        Err(LibraryError::Tag(msg)) => {
            assert!(!msg.is_empty(), "Tag error message should not be empty");
        }
        other => panic!("expected LibraryError::Tag, got {:?}", other),
    }
}
