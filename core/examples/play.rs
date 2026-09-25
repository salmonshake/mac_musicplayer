use std::path::PathBuf;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<String>>();

    if args.is_empty() {
        eprintln!("usage: play [--device <uid>] <audio file>...");
        eprintln!("       play --list-devices");
        std::process::exit(2);
    }

    // --list-devices: print device list and exit.
    if args[0] == "--list-devices" {
        match mmp_core::coreaudio::device::output_devices() {
            Ok(devices) => {
                let default_id = mmp_core::coreaudio::device::default_output_device().ok();
                for d in &devices {
                    let prefix = if default_id == Some(d.id) { "* " } else { "  " };
                    let bt = if d.is_bluetooth() { "  [BT]" } else { "" };
                    println!(
                        "{}{}  {}  {}ch  {}Hz{}",
                        prefix,
                        d.uid,
                        d.name,
                        d.output_channels,
                        d.nominal_sample_rate as u32,
                        bt
                    );
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        std::process::exit(0);
    }

    // --device <uid>: parse device UID and remaining file args.
    let (device_uid, file_args) = if args[0] == "--device" {
        match args.get(1) {
            Some(uid) => (Some(uid.clone()), args[2..].to_vec()),
            None => {
                eprintln!("usage: play [--device <uid>] <audio file>...");
                eprintln!("       play --list-devices");
                std::process::exit(2);
            }
        }
    } else {
        (None, args)
    };

    if file_args.is_empty() {
        eprintln!("usage: play [--device <uid>] <audio file>...");
        eprintln!("       play --list-devices");
        std::process::exit(2);
    }

    let paths: Vec<PathBuf> = file_args.into_iter().map(PathBuf::from).collect();
    let opts = match device_uid {
        Some(uid) => mmp_core::playback::PlaybackOptions {
            device_uid: Some(uid),
        },
        None => mmp_core::playback::PlaybackOptions::default(),
    };

    match mmp_core::playback::play_files_blocking(&paths, &opts) {
        Ok(report) => {
            for f in &report.failures {
                eprintln!("skipped {}: {}", f.path.display(), f.error);
            }
            if report.device_fallback {
                eprintln!(
                    "warning: requested device not found or disconnected; \
                     playing on the default output"
                );
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
