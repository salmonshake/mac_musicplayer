/// Application name used for macOS data directories.
pub const APP_NAME: &str = "Maplay";

/// Former name of the data directory; moved to APP_NAME on first use.
const LEGACY_APP_NAME: &str = "MacMusicPlay";

/// Build the data directory path inside `home/Library/Application Support/<APP_NAME>`.
fn data_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    home.join("Library")
        .join("Application Support")
        .join(APP_NAME)
}

/// Moves to the new name when only the old directory exists
/// (keeps the library, playlists and settings).
fn migrate_legacy_dir(home: &std::path::Path) -> std::io::Result<()> {
    let support = home.join("Library").join("Application Support");
    let old = support.join(LEGACY_APP_NAME);
    let new = support.join(APP_NAME);
    if old.is_dir() && !new.exists() {
        std::fs::rename(&old, &new)?;
    }
    Ok(())
}

/// Return the application data directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn data_dir() -> std::io::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?;
    let home = std::path::Path::new(&home);
    migrate_legacy_dir(home)?;
    let dir = data_dir_in(home);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Return the path to the library database file.
///
/// The directory is created lazily on first call.
pub fn library_db_path() -> std::io::Result<std::path::PathBuf> {
    Ok(data_dir()?.join("library.db"))
}

/// Build the playlists directory path inside the data directory under `home`.
#[cfg(test)]
fn playlists_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    data_dir_in(home).join("playlists")
}

/// Return the playlists directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn playlists_dir() -> std::io::Result<std::path::PathBuf> {
    let dir = data_dir()?.join("playlists");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build the artwork directory path inside the data directory under `home`.
#[cfg(test)]
fn artwork_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    data_dir_in(home).join("artwork")
}

/// Return the artwork directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn artwork_dir() -> std::io::Result<std::path::PathBuf> {
    let dir = data_dir()?.join("artwork");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build the settings file path inside the data directory under `home`.
#[cfg(test)]
fn settings_path_in(home: &std::path::Path) -> std::path::PathBuf {
    data_dir_in(home).join("settings.txt")
}

/// Return the path to the settings file, creating the data directory if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn settings_path() -> std::io::Result<std::path::PathBuf> {
    Ok(data_dir()?.join("settings.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_correct() {
        assert_eq!(APP_NAME, "Maplay");
    }

    #[test]
    fn migrate_legacy_dir_moves_old_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let support = home.join("Library").join("Application Support");
        let legacy = support.join(LEGACY_APP_NAME);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("settings.txt"), "x").unwrap();
        assert!(migrate_legacy_dir(home).is_ok());
        assert!(support.join(APP_NAME).join("settings.txt").exists());
        assert!(!support.join(LEGACY_APP_NAME).exists());
        // Second call is a no-op (old gone, new exists).
        assert!(migrate_legacy_dir(home).is_ok());
    }

    #[test]
    fn migrate_legacy_dir_keeps_existing_new_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let support = home.join("Library").join("Application Support");
        let legacy = support.join(LEGACY_APP_NAME);
        let new_dir = support.join(APP_NAME);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(legacy.join("old.txt"), "a").unwrap();
        std::fs::write(new_dir.join("new.txt"), "b").unwrap();
        assert!(migrate_legacy_dir(home).is_ok());
        assert!(support.join(LEGACY_APP_NAME).exists());
        assert!(support.join(APP_NAME).exists());
    }

    #[test]
    fn data_dir_in_correct() {
        let tmp = std::env::temp_dir();
        let result = data_dir_in(&tmp);
        let expected = tmp
            .join("Library")
            .join("Application Support")
            .join(APP_NAME);
        assert_eq!(result, expected);
    }

    #[test]
    fn playlists_dir_in_correct() {
        let tmp = std::env::temp_dir();
        let result = playlists_dir_in(&tmp);
        let expected = tmp
            .join("Library")
            .join("Application Support")
            .join(APP_NAME)
            .join("playlists");
        assert_eq!(result, expected);
    }

    #[test]
    fn artwork_dir_in_correct() {
        let tmp = std::env::temp_dir();
        let result = artwork_dir_in(&tmp);
        let expected = tmp
            .join("Library")
            .join("Application Support")
            .join(APP_NAME)
            .join("artwork");
        assert_eq!(result, expected);
    }

    #[test]
    fn settings_path_in_correct() {
        let tmp = std::env::temp_dir();
        let result = settings_path_in(&tmp);
        let expected = tmp
            .join("Library")
            .join("Application Support")
            .join(APP_NAME)
            .join("settings.txt");
        assert_eq!(result, expected);
    }
}
