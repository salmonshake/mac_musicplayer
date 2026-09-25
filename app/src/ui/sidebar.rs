//! Sidebar panel with navigation items, genre list, and folder management.

use eframe::egui;
use crate::state::{AppState, Nav};

/// Show the left sidebar panel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar").show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            section(ui, "LIBRARY");
            nav_item(ui, state, "全曲", Nav::AllTracks);
            nav_item(ui, state, "アーティスト", Nav::Artists);
            nav_item(ui, state, "アルバム", Nav::Albums);
            nav_item(ui, state, "ジャンル", Nav::Genres);
            ui.add_space(12.0);
            section(ui, "GENRES");
            let genres: Vec<(String, u32)> = state.genres.iter().take(30).cloned().collect();
            for (g, n) in genres {
                nav_item(ui, state, &format!("{g}  ({n})"), Nav::Genre(g.clone()));
            }
            ui.add_space(12.0);
            section(ui, "FOLDERS");
            let mut remove: Option<std::path::PathBuf> = None;
            for (_, p) in &state.folders {
                ui.horizontal(|ui| {
                    let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
                    ui.label(name).on_hover_text(p.display().to_string());
                    if ui.small_button("−").on_hover_text("ライブラリから外す（ファイルは消えません）").clicked() {
                        remove = Some(p.clone());
                    }
                });
            }
            if let Some(p) = remove {
                state.remove_folder(&p);
            }
            if ui.button("フォルダを追加…").clicked()
                && let Some(p) = rfd::FileDialog::new().set_title("音楽フォルダを選択").pick_folder()
            {
                state.add_folder(p);
            }
            if state.scanning {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("スキャン中…");
                });
            }
        });
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).small().strong());
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, label: &str, nav: Nav) {
    let selected = state.nav == nav;
    if ui.selectable_label(selected, label).clicked() {
        state.set_nav(nav);
    }
}
