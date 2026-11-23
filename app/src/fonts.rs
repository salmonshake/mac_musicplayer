use eframe::egui;
use eframe::epaint::text::{FontData, FontInsert, FontPriority, FontTweak, InsertFontFamily};
use std::path::PathBuf;

/// CJK font candidates on macOS.
const CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];

/// Install CJK fonts into the egui context.
///
/// Prioritizes Hiragino for all characters (including Latin) to eliminate
/// vertical misalignment between CJK and Latin text.
///
/// Environment variables (development only):
/// - `MMP_CJK_Y`: y_offset_factor override (default 0.20)
/// - `MMP_CJK_SCALE`: scale override (default 1.0)
/// - `MMP_CJK_PRIORITY`: "lowest" only overrides; default is "highest"
pub fn install_cjk_fonts(ctx: &egui::Context) -> Option<PathBuf> {
    for p in CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            let y = std::env::var("MMP_CJK_Y")
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.20);
            let scale = std::env::var("MMP_CJK_SCALE")
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(1.0);
            let lowest = std::env::var("MMP_CJK_PRIORITY")
                .map(|v| v == "lowest")
                .unwrap_or(false);
            let priority = if lowest {
                FontPriority::Lowest
            } else {
                FontPriority::Highest
            };
            let mut data = FontData::from_owned(bytes);
            data.tweak = FontTweak {
                y_offset_factor: y,
                scale,
                ..Default::default()
            };
            ctx.add_font(FontInsert {
                name: "cjk".to_owned(),
                data,
                families: vec![
                    InsertFontFamily {
                        family: egui::FontFamily::Proportional,
                        priority: priority.clone(),
                    },
                    InsertFontFamily {
                        family: egui::FontFamily::Monospace,
                        priority,
                    },
                ],
            });
            return Some(PathBuf::from(p));
        }
    }
    None
}
