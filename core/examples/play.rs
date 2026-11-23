//! CLI player: plays files gaplessly through the playback engine.
//!
//! Usage: `play [--device <uid>] <audio file>...` or `play --list-devices`.

use std::path::PathBuf;
use std::time::Duration;

use maplay_core::coreaudio::device;
use maplay_core::player::{PlayState, Player};

fn usage() -> ! {
    eprintln!("usage: play [--device <uid>] <audio file>...");
    eprintln!("       play --list-devices");
    std::process::exit(2);
}

fn list_devices() {
    let devices = match device::output_devices() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let default_id = device::default_output_device().ok();
    for d in &devices {
        let prefix = if default_id == Some(d.id) { "* " } else { "  " };
        let bt = if d.is_bluetooth() { "  [BT]" } else { "" };
        println!(
            "{prefix}{}  {}  {}ch  {}Hz{bt}",
            d.uid, d.name, d.output_channels, d.nominal_sample_rate as u32
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (device_uid, files) = match args.first().map(String::as_str) {
        None => usage(),
        Some("--list-devices") => {
            list_devices();
            return;
        }
        Some("--device") => match args.get(1) {
            Some(uid) => (Some(uid.clone()), &args[2..]),
            None => usage(),
        },
        Some(_) => (None, &args[..]),
    };
    if files.is_empty() {
        usage();
    }
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();

    let player = Player::new(device_uid);
    player.play_queue(paths, 0);

    // Wait for playback to start, then for the queue to finish.
    let mut started = false;
    let mut last_error = None;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let s = player.status();
        if s.error.is_some() && s.error != last_error {
            if let Some(e) = &s.error {
                eprintln!("{e}");
            }
            last_error = s.error.clone();
        }
        match s.state {
            PlayState::Playing | PlayState::Paused => started = true,
            PlayState::Stopped if started => break,
            PlayState::Stopped => {
                // Nothing could be opened at all.
                if last_error.is_some() {
                    std::process::exit(1);
                }
            }
        }
    }
}
