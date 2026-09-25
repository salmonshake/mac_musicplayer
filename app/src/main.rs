mod fonts;

use eframe::egui;
use std::path::PathBuf;

struct MmpApp {
    font: Option<PathBuf>,
}

impl eframe::App for MmpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("MacMusicPlay");
            ui.label("日本語表示テスト：一曲目 / テスト歌手");
            ui.label(match &self.font {
                Some(p) => format!("font: {}", p.display()),
                None => "CJK font not found".to_owned(),
            });
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MacMusicPlay")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MacMusicPlay",
        options,
        Box::new(|cc| {
            let font = fonts::install_cjk_fonts(&cc.egui_ctx);
            Ok(Box::new(MmpApp { font }))
        }),
    )
}
