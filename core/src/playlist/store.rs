//! プレイリストの永続化ストア。
//!
//! 各プレイリストを `<dir>/<名前>.m3u8` ファイルとして保存する。
//! データベースとは独立して、ファイル単位で読み書きできる。

use std::io;
use std::path::{Path, PathBuf};

use super::{parse_m3u, write_m3u};

/// プレイリスト操作のエラー型。
#[derive(Debug, thiserror::Error)]
pub enum PlaylistError {
    #[error("invalid playlist name: {0}")]
    InvalidName(String),
    #[error("playlist already exists: {0}")]
    AlreadyExists(String),
    #[error("playlist not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// プレイリストを `<dir>/<名前>.m3u8` として保存する。DB とは独立。
pub struct PlaylistStore {
    dir: PathBuf,
}

impl PlaylistStore {
    /// `dir` を作成して store を構築する。
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// プレイリスト名が有効か検証する。
    ///
    /// 空・"/"・":"・制御文字を含む・先頭が "."・255 バイト超 は不正。
    pub fn validate_name(name: &str) -> Result<(), PlaylistError> {
        let trimmed = name.trim();
        let bad = trimmed.is_empty()
            || name.contains('/')
            || name.contains(':')
            || name.starts_with('.')
            || name.chars().any(|c| c.is_control())
            || name.len() > 255;
        if bad {
            Err(PlaylistError::InvalidName(name.to_owned()))
        } else {
            Ok(())
        }
    }

    fn path_of(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.m3u8"))
    }

    /// 名前の一覧を返す（大文字小文字を無視した名前順）。
    ///
    /// 拡張子 `.m3u8` のファイルだけを対象にし、先頭が `.` のファイル（一時ファイル等）は除外する。
    pub fn list(&self) -> io::Result<Vec<String>> {
        let mut v = Vec::new();
        let mut entries = std::fs::read_dir(&self.dir)?;
        while let Some(Ok(e)) = entries.next() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("m3u8")
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                && !stem.starts_with('.')
            {
                v.push(stem.to_owned());
            }
        }
        v.sort_by_key(|s| s.to_lowercase());
        Ok(v)
    }

    /// 名前付きプレイリストをロードする。
    pub fn load(&self, name: &str) -> Result<Vec<PathBuf>, PlaylistError> {
        Self::validate_name(name)?;
        let p = self.path_of(name);
        if !p.exists() {
            return Err(PlaylistError::NotFound(name.to_owned()));
        }
        let text = std::fs::read_to_string(&p)?;
        Ok(parse_m3u(&text, &self.dir))
    }

    /// 一時ファイルに書いてから rename する（atomic write）。
    ///
    /// 途中で落ちても元のファイルが壊れない。
    pub fn save(&self, name: &str, paths: &[PathBuf]) -> Result<(), PlaylistError> {
        Self::validate_name(name)?;
        let tmp = self.dir.join(format!(".{name}.m3u8.tmp"));
        let content = write_m3u(paths);
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, self.path_of(name))?;
        Ok(())
    }

    /// ストアディレクトリへのパスを返す。
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (1) new が存在しないディレクトリを作成する
    #[test]
    fn test_new_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("playlists");
        assert!(!sub.exists());
        let store = PlaylistStore::new(sub.clone()).unwrap();
        assert!(sub.is_dir());
        assert_eq!(store.dir(), sub);
    }

    /// (2) save → load の往復（日本語名「お気に入り」、パス2つ）
    #[test]
    fn test_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();
        let paths = vec![
            PathBuf::from("/Music/rock/a.flac"),
            PathBuf::from("/Music/日本語/b.mp3"),
        ];
        store.save("お気に入り", &paths).unwrap();
        let loaded = store.load("お気に入り").unwrap();
        assert_eq!(loaded, paths);
    }

    /// (3) list が名前順で .m3u8 以外と一時ファイルを含まない
    #[test]
    fn test_list_filters_and_sorts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        // 有効なプレイリスト
        store.save("Zebra", &[]).unwrap();
        store.save("Apple", &[]).unwrap();

        // 無視されるもの: 拡張子不同、一時ファイル
        std::fs::write(tmp.path().join("readme.txt"), "hi").unwrap();
        std::fs::write(tmp.path().join(".x.m3u8.tmp"), "#EXTM3U\n").unwrap();

        let names = store.list().unwrap();
        assert_eq!(names, vec!["Apple", "Zebra"]);
    }

    /// (4) validate_name: 不正な名前が Err、正常なものが Ok
    #[test]
    fn test_validate_name() {
        assert!(PlaylistStore::validate_name("a/b").is_err());
        assert!(PlaylistStore::validate_name("a:b").is_err());
        assert!(PlaylistStore::validate_name(".hidden").is_err());
        assert!(PlaylistStore::validate_name("").is_err());
        assert!(PlaylistStore::validate_name("   ").is_err());

        assert!(PlaylistStore::validate_name("Rock Mix").is_ok());
    }

    /// (5) load 存在しない名目で NotFound
    #[test]
    fn test_load_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();
        let err = store.load("no_such").unwrap_err();
        match &err {
            PlaylistError::NotFound(n) => assert_eq!(n, "no_such"),
            other => panic!("expected NotFound, got {other}"),
        }
    }

    /// (6) save で一時ファイルが残らない
    #[test]
    fn test_save_no_leaving_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();
        store.save("clean", &[]).unwrap();

        let has_tmp = std::fs::read_dir(tmp.path())
            .unwrap()
            .any(|e| {
                let p = e.unwrap().path();
                p.extension().and_then(|x| x.to_str()) == Some("tmp")
            });
        assert!(!has_tmp);
    }
}
