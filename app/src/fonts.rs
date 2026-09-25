use eframe::egui;
use eframe::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};
use std::path::PathBuf;

const CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];

pub fn install_cjk_fonts(ctx: &egui::Context) -> Option<PathBuf> {
    for p in CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            let data = FontData::from_owned(bytes);
            ctx.add_font(FontInsert {
                name: "cjk".to_owned(),
                data,
                families: vec![
                    InsertFontFamily {
                        family: egui::FontFamily::Proportional,
                        priority: FontPriority::Lowest,
                    },
                    InsertFontFamily {
                        family: egui::FontFamily::Monospace,
                        priority: FontPriority::Lowest,
                    },
                ],
            });
            return Some(PathBuf::from(p));
        }
    }
    None
}
