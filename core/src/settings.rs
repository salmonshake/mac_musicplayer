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

/// Parse a boolean value from a string (on/true/1 → true, off/false/0 → false).
fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "on" | "true" | "1" => Some(true),
        "off" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// Format a bool as on/off for persisted text.
fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// All persisted settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub theme: ThemeMode,
    /// Bit-perfect output (device rate follows the track, software volume off).
    pub bit_perfect: bool,
    /// Exclusive (hog mode) access while in bit-perfect mode.
    pub hog_mode: bool,
    /// Output device UID; None follows the system default output.
    pub output_device: Option<String>,
}

impl Settings {
    /// Parses key=value lines; unknown keys and bad values are ignored.
    pub fn parse(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "theme" => {
                    if let Some(t) = ThemeMode::from_str_opt(v) {
                        s.theme = t;
                    }
                }
                "bit_perfect" => {
                    if let Some(b) = parse_bool(v) {
                        s.bit_perfect = b;
                    }
                }
                "hog_mode" => {
                    if let Some(b) = parse_bool(v) {
                        s.hog_mode = b;
                    }
                }
                "output_device" => {
                    let v = v.trim();
                    s.output_device = if v.is_empty() {
                        None
                    } else {
                        Some(v.to_string())
                    };
                }
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        format!(
            "theme={}\nbit_perfect={}\nhog_mode={}\noutput_device={}\n",
            self.theme.as_str(),
            on_off(self.bit_perfect),
            on_off(self.hog_mode),
            self.output_device.as_deref().unwrap_or("")
        )
    }

    /// Loads settings; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
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
        assert_eq!(
            Settings::parse("# c\nfoo=1\ntheme = light \n").theme,
            ThemeMode::Light
        );
        assert_eq!(Settings::parse("theme=purple").theme, ThemeMode::System);
        let s = Settings {
            theme: ThemeMode::Light,
            ..Default::default()
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn load_missing_is_default_and_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.txt");
        assert_eq!(Settings::load(&p), Settings::default());
        let s = Settings {
            theme: ThemeMode::Dark,
            ..Default::default()
        };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
    }

    #[test]
    fn parse_output_mode() {
        let s = Settings::parse("bit_perfect=on\nhog_mode=off\n");
        assert!(s.bit_perfect);
        assert!(!s.hog_mode);
        assert_eq!(s.theme, ThemeMode::System);

        assert!(!Settings::parse("bit_perfect=maybe").bit_perfect);

        let s = Settings {
            theme: ThemeMode::Dark,
            bit_perfect: true,
            hog_mode: true,
            ..Default::default()
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn parse_output_device() {
        let s = Settings::parse("output_device=AppleUSBAudioEngine:XYZ\n");
        assert_eq!(s.output_device, Some("AppleUSBAudioEngine:XYZ".to_string()));

        assert_eq!(Settings::parse("output_device=\n").output_device, None);

        let s = Settings {
            output_device: Some("BuiltInSpeakerDevice".into()),
            ..Default::default()
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }
}
