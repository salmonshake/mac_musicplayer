use std::path::{Path, PathBuf};

/// M3U/M3U8 テキストからメディアパスを取り出す。
/// `#` で始まる行と空行は無視する。相対パスは `base`（m3u ファイルのあるディレクトリ）基準で絶対化する。CRLF にも対応する。
pub fn parse_m3u(text: &str, base: &Path) -> Vec<PathBuf> {
    text.lines()
        .map(|l| l.trim_end_matches('\r').trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let p = PathBuf::from(l);
            if p.is_absolute() { p } else { base.join(p) }
        })
        .collect()
}

/// パス一覧を M3U8 テキストにする。
/// 先頭行は `#EXTM3U`、各行末は `\n` である。
pub fn write_m3u(paths: &[PathBuf]) -> String {
    let mut s = String::from("#EXTM3U\n");
    for p in paths {
        s.push_str(&p.to_string_lossy());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (1) 往復テスト: write → parse で同じ Vec になる
    #[test]
    fn test_roundtrip() {
        let paths = vec![
            PathBuf::from("/Music/a.flac"),
            PathBuf::from("/Music/日本語/曲.mp3"),
        ];
        let text = write_m3u(&paths);
        let result = parse_m3u(&text, Path::new("/"));
        assert_eq!(result, paths);
    }

    /// (2) コメント・空行をスキップ
    #[test]
    fn test_skip_comments_and_blank_lines() {
        let text = "#EXTM3U\n#EXTINF:123,Title\n\n/a.flac\n";
        let result = parse_m3u(text, Path::new("/"));
        assert_eq!(result, vec![PathBuf::from("/a.flac")]);
    }

    /// (3) 相対パスを base 基準で絶対化
    #[test]
    fn test_relative_path_resolved() {
        let text = "sub/b.mp3\n";
        let result = parse_m3u(text, Path::new("/Music"));
        assert_eq!(result, vec![PathBuf::from("/Music/sub/b.mp3")]);
    }

    /// (4) CRLF 対応
    #[test]
    fn test_crlf() {
        let text = "#EXTM3U\r\n/a.flac\r\n/b.flac\r\n";
        let result = parse_m3u(text, Path::new("/"));
        assert_eq!(
            result,
            vec![PathBuf::from("/a.flac"), PathBuf::from("/b.flac")]
        );
    }

    /// (5) 空リストの write
    #[test]
    fn test_write_empty() {
        assert_eq!(write_m3u(&[]), "#EXTM3U\n");
    }
}
