//! Top toolbar with search box and status text.

use eframe::egui;
use crate::state::{AppState, Nav};
use crate::ui::tracks::count;

/// Show the top toolbar panel with a search field and status text.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("toolbar")
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 8)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut q = state.search.clone();
                let id = egui::Id::new("search_box");
                let resp = ui
                    .add(
                        egui::TextEdit::singleline(&mut q)
                            .id(id)
                            .hint_text("Search")
                            .desired_width(320.0),
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
        | Nav::Album(_)
        | Nav::Artist(_)
        | Nav::Genre(_)
        | Nav::Playlist(_) => {
            count(state.tracks.len(), "track", "tracks")
        }
        Nav::Albums => {
            count(state.albums.len(), "album", "albums")
        }
        Nav::Artists => {
            count(state.artists.len(), "artist", "artists")
        }
        Nav::Genres => {
            count(state.genres.len(), "genre", "genres")
        }
    }
}
