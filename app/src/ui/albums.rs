//! Album list, album detail, artist list, and genre list.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::{AppState, Nav};
use crate::ui::tracks::format_duration;

/// Show the album list grid.
pub fn show_albums(ui: &mut egui::Ui, state: &mut AppState) {
    if state.albums.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("アルバムがありません");
        });
        return;
    }
    let row_h = 22.0;
    let albums = &state.albums;
    let mut clicked: Option<String> = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::initial(280.0).resizable(true).clip(true))
        .column(Column::initial(200.0).resizable(true).clip(true))
        .column(Column::exact(56.0))
        .column(Column::exact(56.0))
        .column(Column::remainder())
        .header(24.0, |mut h| {
            for t in ["アルバム", "アーティスト", "年", "曲数", "長さ"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, albums.len(), |mut row| {
                let i = row.index();
                let a = match albums.get(i) {
                    Some(a) => a,
                    None => return,
                };
                row.col(|ui| {
                    ui.label(&a.album);
                });
                row.col(|ui| {
                    ui.label(&a.album_artist);
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
                if row.response().clicked() {
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
        ui.centered_and_justified(|ui| {
            ui.label("曲がありません");
        });
        return;
    };

    // Extract owned values before any mutable borrow of state.
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
    parts.push(format!("{} 曲", state.tracks.len()));
    let total_ms: u64 = state.tracks.iter().map(|t| t.duration_ms).sum();
    parts.push(format_duration(total_ms));

    let back = ui.button("← アルバム一覧").clicked();
    ui.heading(album);
    ui.label(artist);
    ui.label(egui::RichText::new(parts.join(" • ")).weak());
    ui.add_space(8.0);
    crate::ui::tracks::show(ui, state);
    if back {
        state.set_nav(Nav::Albums);
    }
}

/// Show the artist list.
pub fn show_artists(ui: &mut egui::Ui, state: &mut AppState) {
    if state.artists.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("アーティストがありません");
        });
        return;
    }
    let row_h = 22.0;
    let artists = &state.artists;
    let mut clicked: Option<String> = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::initial(320.0).resizable(true).clip(true))
        .column(Column::remainder())
        .header(24.0, |mut h| {
            for t in ["アーティスト", "曲数"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, artists.len(), |mut row| {
                let i = row.index();
                let (name, count) = match artists.get(i) {
                    Some(n) => n,
                    None => return,
                };
                row.col(|ui| {
                    ui.label(name);
                });
                row.col(|ui| {
                    ui.label(count.to_string());
                });
                if row.response().clicked() {
                    clicked = Some(name.clone());
                }
            });
        });
    if let Some(name) = clicked {
        state.set_nav(Nav::Artist(name));
    }
}

/// Show the genre list.
pub fn show_genres(ui: &mut egui::Ui, state: &mut AppState) {
    if state.genres.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("ジャンルがありません");
        });
        return;
    }
    let row_h = 22.0;
    let genres = &state.genres;
    let mut clicked: Option<String> = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::initial(320.0).resizable(true).clip(true))
        .column(Column::remainder())
        .header(24.0, |mut h| {
            for t in ["ジャンル", "曲数"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, genres.len(), |mut row| {
                let i = row.index();
                let (name, count) = match genres.get(i) {
                    Some(n) => n,
                    None => return,
                };
                row.col(|ui| {
                    ui.label(name);
                });
                row.col(|ui| {
                    ui.label(count.to_string());
                });
                if row.response().clicked() {
                    clicked = Some(name.clone());
                }
            });
        });
    if let Some(name) = clicked {
        state.set_nav(Nav::Genre(name));
    }
}
