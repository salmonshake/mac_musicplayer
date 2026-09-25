//! Track list with a virtualized table.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::state::AppState;

/// Format milliseconds as `m:ss`.
pub fn format_duration(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Show the track list in the central panel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    if state.tracks.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("曲がありません。左の「フォルダを追加…」から音楽フォルダを追加してください");
        });
        return;
    }
    let row_h = 22.0;
    let selected = state.selected_track;
    let tracks = &state.tracks;
    let mut clicked: Option<usize> = None;
    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .column(Column::exact(36.0))
        .column(Column::initial(260.0).resizable(true).clip(true))
        .column(Column::exact(56.0))
        .column(Column::initial(160.0).resizable(true).clip(true))
        .column(Column::initial(180.0).resizable(true).clip(true))
        .column(Column::initial(100.0).resizable(true).clip(true))
        .column(Column::remainder().clip(true))
        .header(24.0, |mut h| {
            for t in ["＃", "タイトル", "長さ", "アーティスト", "アルバム", "ジャンル", "フォーマット"] {
                h.col(|ui| {
                    ui.strong(t);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, tracks.len(), |mut row| {
                let i = row.index();
                let Some(t) = tracks.get(i) else {
                    return;
                };
                row.set_selected(selected == Some(i));
                row.col(|ui| {
                    ui.label(t.track_no.map(|n| n.to_string()).unwrap_or_default());
                });
                row.col(|ui| {
                    ui.label(&t.title);
                });
                row.col(|ui| {
                    ui.label(format_duration(t.duration_ms));
                });
                row.col(|ui| {
                    ui.label(&t.artist);
                });
                row.col(|ui| {
                    ui.label(&t.album);
                });
                row.col(|ui| {
                    ui.label(&t.genre);
                });
                row.col(|ui| {
                    ui.label(t.format_label());
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
