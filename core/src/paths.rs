/// Application name used for macOS data directories.
pub const APP_NAME: &str = "MacMusicPlay";

/// Build the data directory path inside `home/Library/Application Support/<APP_NAME>`.
fn data_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    home.join("Library")
        .join("Application Support")
        .join(APP_NAME)
}

/// Return the application data directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn data_dir() -> std::io::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set")
    })?;
    let dir = data_dir_in(std::path::Path::new(&home));
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
fn playlists_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    data_dir_in(home).join("playlists")
}

/// Return the playlists directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn playlists_dir() -> std::io::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set")
    })?;
    let dir = playlists_dir_in(std::path::Path::new(&home));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build the artwork directory path inside the data directory under `home`.
fn artwork_dir_in(home: &std::path::Path) -> std::path::PathBuf {
    data_dir_in(home).join("artwork")
}

/// Return the artwork directory, creating it if necessary.
///
/// Fails with `ErrorKind::NotFound` when the `HOME` environment variable
/// is not set.
pub fn artwork_dir() -> std::io::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set")
    })?;
    let dir = artwork_dir_in(std::path::Path::new(&home));
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
        assert_eq!(APP_NAME, "MacMusicPlay");
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
        let expected = tmp.join("Library").join("Application Support").join(APP_NAME).join("settings.txt");
        assert_eq!(result, expected);
    }
}
