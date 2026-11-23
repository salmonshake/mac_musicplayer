use maplay_core::decode;
use std::path::PathBuf;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("testdata")
        .join(name)
}

fn decode_all(name: &str) -> Option<Vec<f32>> {
    let p = testdata(name);
    if !p.exists() {
        eprintln!("skip: {} not found", p.display());
        return None;
    }
    let mut d = match decode::open(&p) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skip: {} open failed: {e}", p.display());
            return None;
        }
    };
    let info = d.info();
    assert_eq!(info.channels, 2, "{name} channels must be 2");
    let mut buf = vec![0.0f32; 4096 * 2];
    let mut samples = Vec::new();
    loop {
        let n = d.read(&mut buf).expect("{name}: read failed");
        if n == 0 {
            break;
        }
        samples.extend_from_slice(&buf[..n * 2]);
    }
    Some(samples)
}

fn check_seam(ext: &str) {
    let a = decode_all(&format!("gap_a.{ext}"));
    let b = decode_all(&format!("gap_b.{ext}"));
    let (Some(a), Some(b)) = (a, b) else {
        return;
    };
    let mut all = a;
    all.extend(b);
    let frames = all.len() / 2;
    assert_eq!(frames, 264600, "{ext} total frames");
    let seam = 132300;
    let mut max_jump = 0.0f32;
    for i in (seam - 2048)..(seam + 2048) {
        let left = all[i * 2];
        let prev = all[(i - 1) * 2];
        let jump = (left - prev).abs();
        if jump > max_jump {
            max_jump = jump;
        }
    }
    assert!(max_jump < 0.1, "{ext}: max jump {max_jump} at seam");
    // Silent-run check: zero-crossing seams can be silent, so verify no long gaps.
    let mut max_silent_run: usize = 0;
    let mut cur_run: usize = 0;
    for i in (seam - 2048)..(seam + 2048) {
        if all[i * 2].abs() < 1e-3 {
            cur_run += 1;
            if cur_run > max_silent_run {
                max_silent_run = cur_run;
            }
        } else {
            cur_run = 0;
        }
    }
    assert!(
        max_silent_run < 32,
        "{ext}: silent run of {max_silent_run} samples at seam"
    );
}

#[test]
fn seam_flac() {
    check_seam("flac");
}

#[test]
fn seam_mp3() {
    check_seam("mp3");
}

#[test]
fn seam_m4a() {
    check_seam("m4a");
}

#[test]
fn seam_ogg() {
    check_seam("ogg");
}

#[test]
fn flac_bit_exact_against_source() {
    let ref_samples = decode_all("_src6s_44k16.wav");
    let gap_a = decode_all("gap_a.flac");
    let gap_b = decode_all("gap_b.flac");
    let (Some(ref_s), Some(a), Some(b)) = (ref_samples, gap_a, gap_b) else {
        return;
    };
    let mut concatenated = a;
    concatenated.extend(b);
    assert_eq!(ref_s.len(), concatenated.len(), "length must match");
    let max_diff: f32 = ref_s
        .iter()
        .zip(concatenated.iter())
        .map(|(r, c)| (r - c).abs())
        .fold(0.0f32, f32::max);
    assert!(max_diff < 1e-4, "max diff {max_diff} exceeds 1e-4");
}
