fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: play <audio file>");
        std::process::exit(2);
    }
    let path = std::path::Path::new(&args[1]);
    match mmp_core::playback::play_file_blocking(path) {
        Ok(()) => {},
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
