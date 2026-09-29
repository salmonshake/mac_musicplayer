//! Sidebar panel with navigation items, genre list, and folder management.

use eframe::egui;
use crate::state::{AppState, Nav};
use crate::style::{subtle_fill, PANEL_MARGIN, SIDEBAR_WIDTH};

/// Show the left sidebar panel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar")
        .default_size(SIDEBAR_WIDTH)
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(PANEL_MARGIN))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                section(ui, "LIBRARY", 4.0);
                nav_item(ui, state, "all tracks", Nav::AllTracks);
                nav_item(ui, state, "artists", Nav::Artists);
                nav_item(ui, state, "albums", Nav::Albums);
                nav_item(ui, state, "genres", Nav::Genres);
                if section_with_add(ui, "PLAYLISTS", 16.0, "New Playlist")
                    && let Some(n) = state.create_playlist()
                {
                    state.renaming_playlist = Some((n.clone(), n));
                    state.rename_focus = true;
                }
                for name in state.playlists.clone() {
                    playlist_item(ui, state, &name);
                }
                section(ui, "GENRES", 16.0);
                let genres: Vec<(String, u32)> = state.genres.iter().take(30).cloned().collect();
                for (g, n) in genres {
                    genre_item(ui, state, &g, n);
                }
                section(ui, "FOLDERS", 16.0);
                let mut remove: Option<std::path::PathBuf> = None;
                for (_, p) in &state.folders {
                    ui.horizontal(|ui| {
                        let name = p
                            .file_name()
                            .map_or_else(|| p.display().to_string(), |s| s.to_string_lossy().into_owned());
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

fn section_label(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(11.0).strong().weak());
}

fn section(ui: &mut egui::Ui, title: &str, top: f32) {
    ui.add_space(top);
    section_label(ui, title);
    ui.add_space(4.0);
}

/// A section header with a "+" button on the right, both on the same line.
///
/// Returns `true` when the "+" button was clicked.
fn section_with_add(ui: &mut egui::Ui, title: &str, top: f32, tip: &str) -> bool {
    ui.add_space(top);
    let clicked = ui
        .horizontal(|ui| {
            section_label(ui, title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Button::new(egui::RichText::new("+").size(14.0)).frame(false))
                    .on_hover_text(tip)
                    .clicked()
            })
            .inner
        })
        .inner;
    ui.add_space(4.0);
    clicked
}

/// A full-width sidebar row, filled when selected, with an optional count on the right.
fn side_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::RichText>,
    count: Option<u32>,
    selected: bool,
) -> egui::Response {
    let mut b = egui::Button::new(())
        .left_text(label.into())
        .frame_when_inactive(selected)
        .min_size(egui::vec2(ui.available_width(), 30.0));
    if let Some(n) = count {
        b = b.right_text(egui::RichText::new(n.to_string()).weak());
    }
    if selected {
        b = b.fill(subtle_fill(ui, 22));
    }
    ui.add(b)
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, label: &str, nav: Nav) {
    if side_button(ui, label, None, state.nav == nav).clicked() {
        state.set_nav(nav);
    }
}

fn genre_item(ui: &mut egui::Ui, state: &mut AppState, g: &str, n: u32) {
    let selected = matches!(&state.nav, Nav::Genre(fg) if fg == g);
    if side_button(ui, g, Some(n), selected).clicked() {
        state.set_nav(Nav::Genre(g.to_string()));
    }
}

fn playlist_item(ui: &mut egui::Ui, state: &mut AppState, name: &str) {
    // Inline rename: a text field replaces the row while it is being renamed.
    if let Some((_, mut buf)) = state.renaming_playlist.clone().filter(|(old, _)| old == name) {
        let r = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(ui.available_width()));
        if std::mem::take(&mut state.rename_focus) {
            r.request_focus();
        }
        if r.lost_focus() {
            state.renaming_playlist = None;
            let new_name = buf.trim();
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && !new_name.is_empty() && new_name != name {
                state.rename_playlist(name, new_name);
            }
        } else {
            state.renaming_playlist = Some((name.to_string(), buf));
        }
        return;
    }
    let selected = matches!(&state.nav, Nav::Playlist(n) if n == name);
    let resp = side_button(ui, name, None, selected);
    if resp.clicked() {
        state.set_nav(Nav::Playlist(name.to_string()));
    }
    resp.context_menu(|ui| {
        if ui.button("Rename").clicked() {
            state.renaming_playlist = Some((name.to_string(), name.to_string()));
            state.rename_focus = true;
            ui.close();
        }
        if ui.button("Delete").clicked() {
            state.delete_playlist(name);
            ui.close();
        }
    });
}
