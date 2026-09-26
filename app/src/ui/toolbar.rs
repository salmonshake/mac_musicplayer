//! Top toolbar with search box and status text.

use eframe::egui;
use crate::state::{AppState, Nav};

/// Show the top toolbar panel with a search field and status text.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("toolbar").show(ui, |ui| {
        ui.horizontal(|ui| {
            let mut q = state.search.clone();
            let id = egui::Id::new("search_box");
            let resp = ui
                .add(
                    egui::TextEdit::singleline(&mut q)
                        .id(id)
                        .hint_text("検索")
                        .desired_width(280.0),
                );
            // ⌘F: focus the search box
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                resp.request_focus();
            }
            // Esc while focused: clear
            if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                q.clear();
            }
            // Push back to state when changed
            if q != state.search {
                state.set_search(q);
            }

            // Status text on the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(status_text(state));
            });
        });
    });
}

/// Status text: state.status when non-empty, otherwise view-dependent count.
fn status_text(state: &AppState) -> String {
    if !state.status.is_empty() {
        return state.status.clone();
    }
    match &state.nav {
        Nav::AllTracks
        | Nav::Artist(_)
        | Nav::Genre(_) => {
            format!("{} 曲", state.tracks.len())
        }
        Nav::Albums | Nav::Album(_) => {
            format!("{} アルバム", state.albums.len())
        }
        Nav::Artists => {
            format!("{} アーティスト", state.artists.len())
        }
        Nav::Genres => {
            format!("{} ジャンル", state.genres.len())
        }
    }
}
