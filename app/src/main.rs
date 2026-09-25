mod fonts;
mod state;

use eframe::egui;
use std::path::PathBuf;

struct MmpApp {
    font: Option<PathBuf>,
    state: Result<state::AppState, String>,
}

impl eframe::App for MmpApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(app_state) = &mut self.state {
            app_state.poll(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("MacMusicPlay");
            ui.label("日本語表示テスト：一曲目 / テスト歌手");
            ui.label(match &self.font {
                Some(p) => format!("font: {}", p.display()),
                None => "CJK font not found".to_owned(),
            });
            match &self.state {
                Ok(app_state) => {
                    ui.label(format!(
                        "status: {} (scanning: {})",
                        app_state.status,
                        app_state.scanning
                    ));
                    ui.label(format!(
                        "tracks: {}  folders: {}",
                        app_state.tracks.len(),
                        app_state.folders.len()
                    ));
                }
                Err(e) => {
                    ui.label(format!("Error: {e}"));
                }
            }
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
            let state = state::AppState::open().map_err(|e| e.to_string());
            Ok(Box::new(MmpApp { font, state }))
        }),
    )
}
