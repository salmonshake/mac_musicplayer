//! User settings persisted as key=value lines in settings.txt.

use std::io;
use std::path::Path;

/// Color theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Follow the macOS appearance.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// Next mode for the toolbar toggle: System -> Light -> Dark -> System.
    pub fn next(self) -> Self {
        match self {
            ThemeMode::System => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim() {
            "system" => Some(ThemeMode::System),
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            _ => None,
        }
    }
}

/// All persisted settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub theme: ThemeMode,
}

impl Settings {
    /// Parses key=value lines; unknown keys and bad values are ignored.
    pub fn parse(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if k.trim() == "theme"
                && let Some(t) = ThemeMode::from_str_opt(v)
            {
                s.theme = t;
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        format!("theme={}\n", self.theme.as_str())
    }

    /// Loads settings; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    /// Writes settings atomically (tmp file, then rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("txt.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycles() {
        assert_eq!(ThemeMode::System.next(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.next(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.next(), ThemeMode::System);
    }

    #[test]
    fn parse_and_roundtrip() {
        assert_eq!(Settings::parse("theme=dark\n").theme, ThemeMode::Dark);
        assert_eq!(Settings::parse("# c\nfoo=1\ntheme = light \n").theme, ThemeMode::Light);
        assert_eq!(Settings::parse("theme=purple").theme, ThemeMode::System);
        let s = Settings { theme: ThemeMode::Light };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn load_missing_is_default_and_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.txt");
        assert_eq!(Settings::load(&p), Settings::default());
        let s = Settings { theme: ThemeMode::Dark };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
    }
}
