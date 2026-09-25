use std::path::PathBuf;

fn main() {
    let paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: play <audio file>...");
        std::process::exit(2);
    }
    match mmp_core::playback::play_files_blocking(&paths, &mmp_core::playback::PlaybackOptions::default()) {
        Ok(report) => {
            for f in &report.failures {
                eprintln!("skipped {}: {}", f.path.display(), f.error);
            }
            if report.failures.len() == paths.len() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
