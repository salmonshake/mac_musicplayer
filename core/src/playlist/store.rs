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

    /// 空のプレイリストを作る。同名があれば [`PlaylistError::AlreadyExists`]。
    pub fn create(&self, name: &str) -> Result<(), PlaylistError> {
        Self::validate_name(name)?;
        if self.path_of(name).exists() {
            return Err(PlaylistError::AlreadyExists(name.to_owned()));
        }
        self.save(name, &[])
    }

    /// プレイリストの名前を変える。
    ///
    /// `old` が存在しなければ [`NotFound`]、`new` が既にあれば [`AlreadyExists`]。
    pub fn rename(&self, old: &str, new: &str) -> Result<(), PlaylistError> {
        Self::validate_name(old)?;
        Self::validate_name(new)?;
        if !self.path_of(old).exists() {
            return Err(PlaylistError::NotFound(old.to_owned()));
        }
        if old != new && self.path_of(new).exists() {
            return Err(PlaylistError::AlreadyExists(new.to_owned()));
        }
        std::fs::rename(self.path_of(old), self.path_of(new))?;
        Ok(())
    }

    /// プレイリストを削除する（曲ファイル自体は消さない）。
    ///
    /// 存在しなければ [`NotFound`]。
    pub fn delete(&self, name: &str) -> Result<(), PlaylistError> {
        Self::validate_name(name)?;
        let p = self.path_of(name);
        if !p.exists() {
            return Err(PlaylistError::NotFound(name.to_owned()));
        }
        std::fs::remove_file(p)?;
        Ok(())
    }

    /// 既存プレイリストの末尾に曲を追加する。
    ///
    /// 存在しないプレイリストには [`NotFound`] を返す。
    /// 同じ曲の重複追加も許す。
    pub fn add(&self, name: &str, paths: &[PathBuf]) -> Result<(), PlaylistError> {
        let mut v = self.load(name)?;
        v.extend_from_slice(paths);
        self.save(name, &v)
    }

    /// 先頭から index 番目の曲を取り除く。
    ///
    /// 範囲外の場合は何もしない（Err を返さない）。
    pub fn remove_at(&self, name: &str, index: usize) -> Result<(), PlaylistError> {
        let mut v = self.load(name)?;
        if index < v.len() {
            v.remove(index);
            self.save(name, &v)?;
        }
        Ok(())
    }

    /// 先頭から `from` 番目の曲を `to` 番目に移動する（`to` は移動後の位置）。
    ///
    /// 範囲外の場合は何もしない（Err を返さない）。
    pub fn move_item(&self, name: &str, from: usize, to: usize) -> Result<(), PlaylistError> {
        let mut v = self.load(name)?;
        if from < v.len() && to < v.len() && from != to {
            let item = v.remove(from);
            v.insert(to, item);
            self.save(name, &v)?;
        }
        Ok(())
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

        let has_tmp = std::fs::read_dir(tmp.path()).unwrap().any(|e| {
            let p = e.unwrap().path();
            p.extension().and_then(|x| x.to_str()) == Some("tmp")
        });
        assert!(!has_tmp);
    }

    /// (7) create: 空プレイリストが作れる・list に出る・二重 create が AlreadyExists
    #[test]
    fn test_create() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        assert!(store.create("Rock").is_ok());
        let names = store.list().unwrap();
        assert!(names.contains(&"Rock".to_owned()));

        // 同名は AlreadyExists
        match store.create("Rock").unwrap_err() {
            PlaylistError::AlreadyExists(n) => assert_eq!(n, "Rock"),
            other => panic!("expected AlreadyExists, got {other}"),
        }

        // 存在しない名前の load は NotFound
        assert!(store.load("no_such").is_err());
    }

    /// (8) rename: 古い名が消え、新しい名で load できる・既存名への rename が AlreadyExists・無い名前の rename が NotFound
    #[test]
    fn test_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        store.save("old", &[]).unwrap();

        // 正常な rename
        store.rename("old", "new_name").unwrap();
        assert!(store.load("old").is_err());
        assert!(store.load("new_name").is_ok());

        // 既存名への rename は AlreadyExists
        store.save("target", &[]).unwrap();
        match store.rename("new_name", "target").unwrap_err() {
            PlaylistError::AlreadyExists(n) => assert_eq!(n, "target"),
            other => panic!("expected AlreadyExists, got {other}"),
        }

        // 存在しない名前の rename は NotFound
        match store.rename("no_such", "x").unwrap_err() {
            PlaylistError::NotFound(n) => assert_eq!(n, "no_such"),
            other => panic!("expected NotFound, got {other}"),
        }
    }

    /// (9) delete: 削除後に list から消える・無い名前の delete が NotFound
    #[test]
    fn test_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        store.save("to_del", &[]).unwrap();
        assert!(store.list().unwrap().contains(&"to_del".to_owned()));

        store.delete("to_del").unwrap();
        assert!(!store.list().unwrap().contains(&"to_del".to_owned()));

        // 既に無ければ NotFound
        match store.delete("to_del").unwrap_err() {
            PlaylistError::NotFound(n) => assert_eq!(n, "to_del"),
            other => panic!("expected NotFound, got {other}"),
        }
    }

    /// (10) add: 末尾に追加・同じ曲の重複追加も許す
    #[test]
    fn test_add() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        store.save("pl", &[]).unwrap();

        store.add("pl", &[PathBuf::from("/a.flac")]).unwrap();
        store.add("pl", &[PathBuf::from("/b.mp3")]).unwrap();
        store.add("pl", &[PathBuf::from("/a.flac")]).unwrap(); // 重複追加 OK

        let loaded = store.load("pl").unwrap();
        assert_eq!(
            loaded,
            vec![
                PathBuf::from("/a.flac"),
                PathBuf::from("/b.mp3"),
                PathBuf::from("/a.flac"),
            ]
        );

        // 存在しないプレイリストへの add は NotFound
        assert!(store.add("no_such", &[PathBuf::from("/x")]).is_err());
    }

    /// (11) remove_at: 指定インデックスを削除・範囲外は変化なし
    #[test]
    fn test_remove_at() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        store
            .save(
                "pl",
                &[
                    PathBuf::from("/a"),
                    PathBuf::from("/b"),
                    PathBuf::from("/c"),
                ],
            )
            .unwrap();

        // index=1 ("b") を削除
        store.remove_at("pl", 1).unwrap();
        let loaded = store.load("pl").unwrap();
        assert_eq!(loaded, vec![PathBuf::from("/a"), PathBuf::from("/c")]);

        // 範囲外 (index=99) は何もしない
        store.remove_at("pl", 99).unwrap();
        let loaded = store.load("pl").unwrap();
        assert_eq!(loaded, vec![PathBuf::from("/a"), PathBuf::from("/c")]);
    }

    /// (12) move_item: [a,b,c] → move_item(0,2) → [b,c,a] → move_item(2,0) → [c,a,b]
    /// 範囲外は変化なし
    #[test]
    fn test_move_item() {
        let tmp = tempfile::tempdir().unwrap();
        let store = PlaylistStore::new(tmp.path().to_path_buf()).unwrap();

        store
            .save(
                "pl",
                &[
                    PathBuf::from("/a"),
                    PathBuf::from("/b"),
                    PathBuf::from("/c"),
                ],
            )
            .unwrap();

        // from=0 の "/a" を to=2 へ → [b, c, a]
        store.move_item("pl", 0, 2).unwrap();
        let loaded = store.load("pl").unwrap();
        assert_eq!(
            loaded,
            vec![
                PathBuf::from("/b"),
                PathBuf::from("/c"),
                PathBuf::from("/a")
            ]
        );

        // from=2 の "/a" を to=0 へ → [a, b, c]
        store.move_item("pl", 2, 0).unwrap();
        let loaded = store.load("pl").unwrap();
        assert_eq!(
            loaded,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );

        // 範囲外 (from=99) は何もしない
        store.move_item("pl", 99, 0).unwrap();
        let loaded = store.load("pl").unwrap();
        assert_eq!(
            loaded,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
    }
}
