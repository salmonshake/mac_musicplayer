//! Track list with a virtualized table.

use eframe::egui;
use egui_extras::Column;

use crate::state::{AppState, Nav};
use crate::style::{HEADER_HEIGHT, ROW_HEIGHT};
use crate::ui::icons;
use crate::ui::table::{self, RowBackground};

/// Context-menu actions, applied after the table is drawn.
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
    let in_playlist = matches!(state.nav, Nav::Playlist(_));
    if state.tracks.is_empty() {
        let msg = if in_playlist {
            "This playlist is empty. Right-click a track to add it."
        } else {
            "No tracks yet. Add a music folder from the sidebar."
        };
        return table::empty_message(ui, msg);
    }
    let playlists = &state.playlists;
    let tracks = &state.tracks;
    let selected = state.selected_track;
    let playing_path = state.now.path.as_deref();
    let accent_text = crate::style::accent_text(ui);
    let mut clicked: Option<usize> = None;
    let mut double_clicked: Option<usize> = None;
    let mut action: Option<RowAction> = None;

    let (builder, row_x, colors) = table::builder(ui);
    let builder = builder
        .column(Column::exact(36.0))
        .column(Column::initial(260.0).clip(true))
        .column(Column::exact(56.0))
        .column(Column::initial(160.0).clip(true));
    let builder = if show_album_column {
        builder.column(Column::initial(180.0).clip(true))
    } else {
        builder
    };
    let header_labels: &[&str] = if show_album_column {
        &["#", "Title", "Length", "Artist", "Album", "Genres", "Bitrate/Format"]
    } else {
        &["#", "Title", "Length", "Artist", "Genres", "Bitrate/Format"]
    };
    builder
        .column(Column::initial(100.0).clip(true))
        .column(Column::remainder().clip(true))
        .header(HEADER_HEIGHT, |h| table::header_row(h, header_labels))
        .body(|body| {
            body.rows(ROW_HEIGHT, tracks.len(), |mut row| {
                let i = row.index();
                let Some(t) = tracks.get(i) else {
                    return;
                };
                let is_playing = playing_path == Some(t.path.as_path());
                let mut bg = None;
                // #: track number, or a play icon on the playing row (not colour alone).
                row.col(|ui| {
                    bg = Some(RowBackground::reserve(ui, row_x));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if is_playing {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            icons::play(ui.painter(), r, accent_text);
                        } else {
                            ui.label(
                                egui::RichText::new(t.track_no.map(|n| n.to_string()).unwrap_or_default())
                                    .monospace(),
                            );
                        }
                    });
                });
                row.col(|ui| {
                    let title = egui::RichText::new(&t.title);
                    let title = if is_playing { title.strong() } else { title };
                    ui.add(egui::Label::new(title).truncate());
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format_duration(t.duration_ms));
                    });
                });
                // Artist, followed by the PLAYING badge (as in the mockup). Room is
                // kept for the badge so a long name is truncated, not the badge.
                row.col(|ui| {
                    let reserve = if is_playing { 72.0 } else { 0.0 };
                    let w = (ui.available_width() - reserve).max(24.0);
                    ui.scope(|ui| {
                        ui.set_max_width(w);
                        ui.add(egui::Label::new(or_unknown(&t.artist, "Unknown Artist")).truncate());
                    });
                    if is_playing {
                        crate::style::badge(ui, "PLAYING");
                    }
                });
                if show_album_column {
                    row.col(|ui| {
                        ui.label(or_unknown(&t.album, "Unknown Album"));
                    });
                }
                row.col(|ui| {
                    ui.label(&t.genre);
                });
                row.col(|ui| {
                    ui.label(t.format_label());
                });

                let resp = row.response();
                if let Some(bg) = bg {
                    bg.paint(&resp, colors.fill(i, resp.hovered(), selected == Some(i), is_playing));
                }
                if resp.clicked() {
                    clicked = Some(i);
                }
                if resp.double_clicked() {
                    double_clicked = Some(i);
                }
                resp.context_menu(|ui| {
                    if let Some(a) = row_menu(ui, i, playlists, in_playlist) {
                        action = Some(a);
                    }
                });
            });
        });

    if let Some(i) = clicked {
        state.selected_track = Some(i);
    }
    // Double-click: play from this track.
    if let Some(i) = double_clicked {
        state.play_from(i);
        state.selected_track = Some(i);
    }
    match action {
        Some(RowAction::AddTo(name, i)) => {
            if let Some(p) = state.tracks.get(i).map(|t| t.path.clone()) {
                state.add_to_playlist(&name, &[p]);
            }
        }
        Some(RowAction::Remove(i)) => state.remove_from_playlist(i),
        Some(RowAction::MoveUp(i)) => state.move_in_playlist(i, true),
        Some(RowAction::MoveDown(i)) => state.move_in_playlist(i, false),
        None => {}
    }
}

/// Right-click menu for row `i`: Add to Playlist ▸, and in a playlist
/// Remove / Move Up / Move Down.
fn row_menu(ui: &mut egui::Ui, i: usize, playlists: &[String], in_playlist: bool) -> Option<RowAction> {
    let mut action = None;
    let mut pick = |ui: &mut egui::Ui, label: &str, a: RowAction| {
        if ui.button(label).clicked() {
            action = Some(a);
            ui.close();
        }
    };
    ui.menu_button("Add to Playlist", |ui| {
        if playlists.is_empty() {
            ui.label(egui::RichText::new("No playlists").weak());
        }
        for name in playlists {
            pick(ui, name, RowAction::AddTo(name.clone(), i));
        }
    });
    if in_playlist {
        ui.separator();
        pick(ui, "Remove from Playlist", RowAction::Remove(i));
        pick(ui, "Move Up", RowAction::MoveUp(i));
        pick(ui, "Move Down", RowAction::MoveDown(i));
    }
    action
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
