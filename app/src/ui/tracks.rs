//! Track list with a virtualized table.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::AppState;
use crate::style::{HEADER_HEIGHT, ROW_HEIGHT};

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
    if state.tracks.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("No tracks yet. Add a music folder from the sidebar.");
        });
        return;
    }
    let tracks = &state.tracks;
    let selected = state.selected_track;
    let mut clicked: Option<usize> = None;
    let builder = TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::exact(36.0))
        .column(Column::initial(260.0).resizable(true).clip(true));
    let builder = if show_album_column {
        builder
            .column(Column::exact(56.0))
            .column(Column::initial(160.0).resizable(true).clip(true))
            .column(Column::initial(180.0).resizable(true).clip(true))
            .column(Column::initial(100.0).resizable(true).clip(true))
            .column(Column::remainder().clip(true))
    } else {
        builder
            .column(Column::exact(56.0))
            .column(Column::initial(160.0).resizable(true).clip(true))
            .column(Column::initial(100.0).resizable(true).clip(true))
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
            row.set_selected(selected == Some(i));
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
            // Title column: center
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(&t.title);
                });
            });
            // Length column: right-aligned
            row.col(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format_duration(t.duration_ms));
                });
            });
            // Artist column: center
            row.col(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(&t.artist);
                });
            });
            if show_album_column {
                // Album column: center
                row.col(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(&t.album);
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
        });
    });
    if let Some(i) = clicked {
        state.selected_track = Some(i);
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn format_duration_values() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(61_000), "1:01");
        assert_eq!(format_duration(3_600_000), "60:00");
    }
}
