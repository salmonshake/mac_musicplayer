//! Sidebar panel with navigation items, genre list, and folder management.

use eframe::egui;
use crate::state::{AppState, Nav};
use crate::style::{PANEL_MARGIN, SIDEBAR_WIDTH};

/// Show the left sidebar panel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar")
        .default_size(SIDEBAR_WIDTH)
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(PANEL_MARGIN))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                section(ui, "LIBRARY", 4.0);
                nav_item(ui, state, "all tracks", Nav::AllTracks, None);
                nav_item(ui, state, "artists", Nav::Artists, None);
                nav_item(ui, state, "albums", Nav::Albums, None);
                nav_item(ui, state, "genres", Nav::Genres, None);
                section(ui, "GENRES", 16.0);
                let genres: Vec<(String, u32)> = state.genres.iter().take(30).cloned().collect();
                for (g, n) in genres {
                    genre_item(ui, state, &g, n);
                }
                section(ui, "FOLDERS", 16.0);
                let mut remove: Option<std::path::PathBuf> = None;
                for (_, p) in &state.folders {
                    ui.horizontal(|ui| {
                        let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
                        ui.label(name).on_hover_text(p.display().to_string());
                        if ui.small_button("−").on_hover_text("Remove from library (files are not deleted)").clicked() {
                            remove = Some(p.clone());
                        }
                    });
                }
                if let Some(p) = remove {
                    state.remove_folder(&p);
                }
                if ui.button("Add folder...").clicked()
                    && let Some(p) = rfd::FileDialog::new().set_title("Choose a music folder").pick_folder()
                {
                    state.add_folder(p);
                }
                if state.scanning {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Scanning…");
                    });
                }
            });
        });
}

fn section(ui: &mut egui::Ui, title: &str, top: f32) {
    ui.add_space(top);
    ui.label(egui::RichText::new(title).size(11.0).strong().weak());
    ui.add_space(4.0);
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, label: &str, nav: Nav, _count: Option<u32>) {
    let selected = state.nav == nav;
    let fill = ui.visuals().widgets.inactive.weak_bg_fill;
    let mut b = egui::Button::new(()).left_text(label).frame_when_inactive(false).corner_radius(6.0).min_size(egui::vec2(ui.available_width(), 30.0));
    if selected {
        b = b.fill(fill);
    }
    if ui.add(b).clicked() {
        state.set_nav(nav);
    }
}

fn genre_item(ui: &mut egui::Ui, state: &mut AppState, g: &str, n: u32) {
    let selected = matches!(state.nav, Nav::Genre(ref fg) if fg == g);
    let fill = ui.visuals().widgets.inactive.weak_bg_fill;
    let label = egui::RichText::new(g);
    let count_label = egui::RichText::new(n.to_string()).weak();
    let mut b = egui::Button::new(()).left_text(label).right_text(count_label).frame_when_inactive(false).corner_radius(6.0).min_size(egui::vec2(ui.available_width(), 30.0));
    if selected {
        b = b.fill(fill);
    }
    if ui.add(b).clicked() {
        state.set_nav(Nav::Genre(g.to_string()));
    }
}
