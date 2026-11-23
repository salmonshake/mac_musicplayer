//! Library CLI sample (Task 5.7).
//!
//! Usage: `lib <db> <command> [args]`
//!
//! Commands:
//! - `add <folder>` — register a folder
//! - `scan` — background-scan all registered folders
//! - `stats` — show registered folders and total track count
//! - `watch` — watch mode (Ctrl+C to stop)

use std::path::{Path, PathBuf};

use maplay_core::library::{Library, LibraryWatcher, ScanEvent};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<String>>();

    if args.len() < 2 {
        eprintln!("usage: lib <db> <command> [args]");
        eprintln!("commands: add <folder>, scan, stats, watch");
        std::process::exit(2);
    }

    let db_path = PathBuf::from(&args[0]);
    let command = &args[1];

    match command.as_str() {
        "add" => cmd_add(&db_path, &args),
        "scan" => cmd_scan(&db_path),
        "stats" => cmd_stats(&db_path),
        "watch" => cmd_watch(&db_path),
        other => {
            eprintln!("usage: lib <db> <command> [args]");
            eprintln!("commands: add <folder>, scan, stats, watch");
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
    }
}

/// `lib <db> add <folder>`
fn cmd_add(db_path: &Path, args: &[String]) {
    let folder = match args.get(2) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: lib <db> add <folder>");
            std::process::exit(2);
        }
    };

    match Library::open(db_path) {
        Ok(lib) => match lib.add_folder(&folder) {
            Ok(id) => println!("added folder {id}: {}", folder.display()),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// `lib <db> scan`
fn cmd_scan(db_path: &Path) {
    let handle = maplay_core::library::scan_all_in_background(db_path.to_path_buf());

    while let Ok(event) = handle.events.recv() {
        match event {
            ScanEvent::Started { folder } => {
                println!("scanning {}", folder.display());
            }
            ScanEvent::Finished { folder, stats } => {
                if stats.folder_missing {
                    println!("{}: folder missing (tracks kept)", folder.display());
                } else {
                    println!(
                        "{}: +{} ~{} -{} ={} errors {}",
                        folder.display(),
                        stats.added,
                        stats.updated,
                        stats.removed,
                        stats.unchanged,
                        stats.errors,
                    );
                }
            }
            ScanEvent::Failed { folder, error } => {
                eprintln!("{}: failed: {error}", folder.display());
            }
            ScanEvent::AllDone => {
                break;
            }
        }
    }

    handle.wait();
}

/// `lib <db> stats`
fn cmd_stats(db_path: &Path) {
    let lib = match Library::open(db_path) {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let folders = match lib.folders() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    for (id, path) in &folders {
        println!("{id}  {}", path.display());
    }

    match lib.track_count() {
        Ok(count) => println!("tracks: {count}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// `lib <db> watch`
fn cmd_watch(db_path: &Path) {
    match LibraryWatcher::start(
        db_path.to_path_buf(),
        std::time::Duration::from_millis(1000),
    ) {
        Ok((_, events)) => {
            println!("watching... (Ctrl+C to stop)");
            while let Ok(event) = events.recv() {
                print_scan_event(&event);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// Print a scan event to stdout/stderr.
fn print_scan_event(event: &ScanEvent) {
    match event {
        ScanEvent::Started { folder } => {
            println!("scanning {}", folder.display());
        }
        ScanEvent::Finished { folder, stats } => {
            if stats.folder_missing {
                println!("{}: folder missing (tracks kept)", folder.display());
            } else {
                println!(
                    "{}: +{} ~{} -{} ={} errors {}",
                    folder.display(),
                    stats.added,
                    stats.updated,
                    stats.removed,
                    stats.unchanged,
                    stats.errors,
                );
            }
        }
        ScanEvent::Failed { folder, error } => {
            eprintln!("{}: failed: {error}", folder.display());
        }
        ScanEvent::AllDone => {}
    }
}
