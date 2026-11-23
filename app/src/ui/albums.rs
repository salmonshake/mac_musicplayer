//! Album list, album detail, artist list, and genre list.

use eframe::egui;
use egui_extras::Column;

use crate::state::{AppState, Nav};
use crate::style::{HEADER_HEIGHT, ROW_HEIGHT};
use crate::ui::table::{self, RowBackground};
use crate::ui::tracks::{count, format_duration, or_unknown};

/// Format total milliseconds as `h:mm:ss` (hours shown when > 0).
pub fn format_album_duration(ms: u64) -> String {
    let total_s = ms / 1000;
    let h = total_s / 3600;
    let m = (total_s % 3600) / 60;
    let s = total_s % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Show the album list.
pub fn show_albums(ui: &mut egui::Ui, state: &mut AppState) {
    if state.albums.is_empty() {
        return table::empty_message(ui, "No albums found.");
    }
    let albums = &state.albums;
    let (builder, row_x, colors) = table::builder(ui);
    let mut clicked: Option<String> = None;
    builder
        .column(Column::initial(280.0).clip(true))
        .column(Column::initial(200.0).clip(true))
        .column(Column::exact(56.0))
        .column(Column::exact(56.0))
        .column(Column::remainder())
        .header(HEADER_HEIGHT, |h| {
            table::header_row(h, &["Album", "Artist", "Year", "Tracks", "Length"]);
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, albums.len(), |mut row| {
                let i = row.index();
                let Some(a) = albums.get(i) else {
                    return;
                };
                let mut bg = None;
                row.col(|ui| {
                    bg = Some(RowBackground::reserve(ui, row_x));
                    ui.label(or_unknown(&a.album, "Unknown Album"));
                });
                row.col(|ui| {
                    ui.label(or_unknown(&a.album_artist, "Unknown Artist"));
                });
                row.col(|ui| {
                    ui.label(a.year.map(|y| y.to_string()).unwrap_or_default());
                });
                row.col(|ui| {
                    ui.label(a.track_count.to_string());
                });
                row.col(|ui| {
                    ui.label(format_duration(a.duration_ms));
                });
                let resp = row.response();
                if let Some(bg) = bg {
                    bg.paint(&resp, colors.fill(i, resp.hovered(), false, false));
                }
                if resp.clicked() {
                    clicked = Some(a.album_key.clone());
                }
            });
        });
    if let Some(key) = clicked {
        state.set_nav(Nav::Album(key));
    }
}

/// Show the detail view for a single album.
pub fn show_album(ui: &mut egui::Ui, state: &mut AppState, _key: &str) {
    let Some(first) = state.tracks.first() else {
        return table::empty_message(ui, "No tracks in this album.");
    };

    // Extract owned values before any mutable borrow of state.
    let album_key = first.album_key.clone();
    let art_track = first.path.clone();
    let album = first.album.clone();
    let artist = if first.album_artist.is_empty() {
        first.artist.clone()
    } else {
        first.album_artist.clone()
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(y) = first.year {
        parts.push(y.to_string());
    }
    parts.push(count(state.tracks.len(), "track", "tracks"));
    let total_ms: u64 = state.tracks.iter().map(|t| t.duration_ms).sum();
    parts.push(format_album_duration(total_ms));

    let back = ui.button("← Albums").clicked();
    let tex = state.art.get(&album_key, &art_track);
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        crate::artwork::art_image(ui, tex.as_ref(), 160.0);
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.add_space(36.0);
            ui.label(or_unknown(&album, "Unknown Album").size(26.0));
            ui.add_space(4.0);
            ui.label(or_unknown(&artist, "Unknown Artist").size(16.0));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(parts.join(" • ")).weak());
        });
    });
    ui.add_space(8.0);
    crate::ui::tracks::show(ui, state, false);
    if back {
        state.set_nav(Nav::Albums);
    }
}

/// Show the artist list.
pub fn show_artists(ui: &mut egui::Ui, state: &mut AppState) {
    if state.artists.is_empty() {
        return table::empty_message(ui, "No artists found.");
    }
    if let Some(name) = name_count_table(ui, &state.artists, "Artist") {
        state.set_nav(Nav::Artist(name));
    }
}

/// Show the genre list.
pub fn show_genres(ui: &mut egui::Ui, state: &mut AppState) {
    if state.genres.is_empty() {
        return table::empty_message(ui, "No genres found.");
    }
    if let Some(name) = name_count_table(ui, &state.genres, "Genre") {
        state.set_nav(Nav::Genre(name));
    }
}

/// A (name, track count) table; returns the clicked name.
fn name_count_table(
    ui: &mut egui::Ui,
    rows: &[(String, u32)],
    name_header: &str,
) -> Option<String> {
    let (builder, row_x, colors) = table::builder(ui);
    let mut clicked = None;
    builder
        .column(Column::initial(320.0).clip(true))
        .column(Column::remainder())
        .header(HEADER_HEIGHT, |h| {
            table::header_row(h, &[name_header, "Tracks"])
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                let i = row.index();
                let Some((name, n)) = rows.get(i) else {
                    return;
                };
                let mut bg = None;
                row.col(|ui| {
                    bg = Some(RowBackground::reserve(ui, row_x));
                    ui.label(name);
                });
                row.col(|ui| {
                    ui.label(n.to_string());
                });
                let resp = row.response();
                if let Some(bg) = bg {
                    bg.paint(&resp, colors.fill(i, resp.hovered(), false, false));
                }
                if resp.clicked() {
                    clicked = Some(name.clone());
                }
            });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::format_album_duration;

    #[test]
    fn format_album_duration_values() {
        assert_eq!(format_album_duration(2_580_000), "43:00");
        assert_eq!(format_album_duration(3_723_000), "1:02:03");
    }
}
