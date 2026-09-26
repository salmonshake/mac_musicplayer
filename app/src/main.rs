mod fonts;
mod state;
mod ui;

use eframe::egui;
use std::path::PathBuf;

struct MmpApp {
    _font: Option<PathBuf>,
    state: Result<state::AppState, String>,
}

impl eframe::App for MmpApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(app_state) = &mut self.state {
            app_state.poll(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(s) = &mut self.state {
            ui::sidebar::show(ui, s);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            match &mut self.state {
                Ok(s) => match s.nav.clone() {
                    state::Nav::AllTracks
                    | state::Nav::Artist(_)
                    | state::Nav::Genre(_) => {
                        ui::tracks::show(ui, s);
                    }
                    state::Nav::Albums => {
                        ui::albums::show_albums(ui, s);
                    }
                    state::Nav::Album(k) => {
                        ui::albums::show_album(ui, s, &k);
                    }
                    state::Nav::Artists => {
                        ui::albums::show_artists(ui, s);
                    }
                    state::Nav::Genres => {
                        ui::albums::show_genres(ui, s);
                    }
                },
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
            Ok(Box::new(MmpApp { _font: font, state }))
        }),
    )
}
