//! Track list with a virtualized table.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::AppState;
use crate::style::{HEADER_HEIGHT, ROW_HEIGHT};

/// Private enum for track-list context-menu actions.
enum RowAction {
    /// Add the track at index `i` to the playlist named `name`.
    AddTo(String, usize),
    /// Remove the track at index `i` from its playlist.
    Remove(usize),
    /// Move the track at index `i` up one position.
    MoveUp(usize),
    /// Move the track at index `i` down one position.
    MoveDown(usize),
}

/// Format milliseconds as `m:ss`.
pub fn format_duration(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Show the track list in the central panel.
///
/// When `show_album_column` is false (album detail view), the Album column is
/// omitted from both the header and the rows.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, show_album_column: bool) {
    let playlists: Vec<String> = state.playlists.clone();
    let in_playlist = matches!(state.nav, crate::state::Nav::Playlist(_));
    let mut action: Option<RowAction> = None;
    if state.tracks.is_empty() {
        ui.centered_and_justified(|ui| {
            if in_playlist {
                ui.label("This playlist is empty. Right-click a track to add it.");
            } else {
                ui.label("No tracks yet. Add a music folder from the sidebar.");
            }
        });
        return;
    }
    let tracks = &state.tracks;
    let selected = state.selected_track;
    // Current playing track path (for highlighting + double-click replay).
    let playing_path: Option<std::path::PathBuf> = state.now.path.clone();
    let mut clicked: Option<usize> = None;
    let mut double_clicked: Option<usize> = None;
    let builder = TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::exact(36.0))
        .column(Column::initial(260.0).clip(true));
    let builder = if show_album_column {
        builder
            .column(Column::exact(56.0))
            .column(Column::initial(160.0).clip(true))
            .column(Column::initial(180.0).clip(true))
            .column(Column::initial(100.0).clip(true))
            .column(Column::remainder().clip(true))
    } else {
        builder
            .column(Column::exact(56.0))
            .column(Column::initial(160.0).clip(true))
            .column(Column::initial(100.0).clip(true))
            .column(Column::remainder().clip(true))
    };
    let header_labels = if show_album_column {
        &["#", "Title", "Length", "Artist", "Album", "Genres", "Bitrate/Format"][..]
    } else {
        &["#", "Title", "Length", "Artist", "Genres", "Bitrate/Format"][..]
    };
    builder.header(HEADER_HEIGHT, |mut h| {
        for &t in header_labels {
            h.col(|ui| {
                ui.label(egui::RichText::new(t).weak());
            });
        }
    })
    .body(|body| {
        body.rows(ROW_HEIGHT, tracks.len(), |mut row| {
            let i = row.index();
            let Some(t) = tracks.get(i) else {
                return;
            };
            // Highlight the currently playing row (different from selection).
            let is_playing = playing_path.as_deref() == Some(t.path.as_path());
            row.set_selected(selected == Some(i) || is_playing);
            // # column: right-aligned
            row.col(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            t.track_no.map(|n| n.to_string()).unwrap_or_default(),
                        )
                        .monospace(),
                    );
                });
            });
            // Title column: center, show "PLAYING" badge when this track is active.
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(&t.title);
                    if is_playing {
                        ui.label(egui::RichText::new(" PLAYING ")
                            .small()
                            .strong()
                            .color(egui::Color32::WHITE)
                            .background_color(ui.visuals().selection.bg_fill));
                    }
                });
            });
            // Length column: right-aligned
            row.col(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format_duration(t.duration_ms));
                });
            });
            // Artist column: center, show "Unknown Artist" in weak when empty.
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(or_unknown(&t.artist, "Unknown Artist"));
                });
            });
            if show_album_column {
                // Album column: center, show "Unknown Album" in weak when empty.
                row.col(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(or_unknown(&t.album, "Unknown Album"));
                    });
                });
            }
            // Genre column: center
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(&t.genre);
                });
            });
            // Bitrate/Format column: center
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(t.format_label());
                });
            });
            if row.response().clicked() {
                clicked = Some(i);
            }
            if row.response().double_clicked() {
                double_clicked = Some(i);
            }
            // Context menu: right-click actions (Add to Playlist / Remove / Move).
            row.response().context_menu(|ui| {
                ui.menu_button("Add to Playlist", |ui| {
                    if playlists.is_empty() {
                        ui.label(egui::RichText::new("No playlists").weak());
                    }
                    for name in &playlists {
                        if ui.button(name).clicked() {
                            action = Some(RowAction::AddTo(name.clone(), i));
                            ui.close();
                        }
                    }
                });
                if in_playlist {
                    ui.separator();
                    if ui.button("Remove from Playlist").clicked() {
                        action = Some(RowAction::Remove(i));
                        ui.close();
                    }
                    if ui.button("Move Up").clicked() {
                        action = Some(RowAction::MoveUp(i));
                        ui.close();
                    }
                    if ui.button("Move Down").clicked() {
                        action = Some(RowAction::MoveDown(i));
                        ui.close();
                    }
                }
            });
        });
    });
    if let Some(i) = clicked {
        state.selected_track = Some(i);
    }
    // Double-click: play from this track (replay from the clicked position).
    if let Some(i) = double_clicked {
        state.play_from(i);
        state.selected_track = Some(i);
    }
    // Apply context-menu action.
    if let Some(action) = action {
        match action {
            RowAction::AddTo(name, i) => {
                if let Some(p) = state.tracks.get(i).map(|t| t.path.clone()) {
                    state.add_to_playlist(&name, &[p]);
                }
            }
            RowAction::Remove(i) => state.remove_from_playlist(i),
            RowAction::MoveUp(i) => state.move_in_playlist(i, true),
            RowAction::MoveDown(i) => state.move_in_playlist(i, false),
        }
    }
}

/// Format a count with singular/plural labels: `"0 tracks"`, `"1 track"`, `"2 tracks"`.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Return a `RichText` that renders `s` when non-empty, otherwise `placeholder` rendered with `.weak()`.
pub fn or_unknown(s: &str, placeholder: &str) -> egui::RichText {
    if s.is_empty() {
        egui::RichText::new(placeholder).weak()
    } else {
        egui::RichText::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{count, format_duration};

    #[test]
    fn format_duration_values() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(61_000), "1:01");
        assert_eq!(format_duration(3_600_000), "60:00");
    }

    #[test]
    fn count_values() {
        assert_eq!(count(0, "track", "tracks"), "0 tracks");
        assert_eq!(count(1, "track", "tracks"), "1 track");
        assert_eq!(count(2, "track", "tracks"), "2 tracks");
    }
}
