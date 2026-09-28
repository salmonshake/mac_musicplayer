//! Up Next panel: shows upcoming queued tracks with click-to-jump.

use eframe::egui;
use crate::state::AppState;
use crate::ui::tracks::format_duration;

/// Show the "Up Next" right panel.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::right("queue")
        .exact_size(300.0)
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(crate::style::PANEL_MARGIN))
        .show(ui, |ui| {
            ui.heading("Up Next");
            ui.separator();

            let upcoming = &state.now.upcoming;
            if upcoming.is_empty() {
                ui.label(egui::RichText::new("No upcoming tracks").weak());
                return;
            }

            for &idx in upcoming {
                if let Some(track) = state.queue_items.get(idx) {
                    let label = format!("{title} — {artist}", title = track.title, artist = track.artist);
                    let b = egui::Button::new(())
                        .left_text(&label)
                        .right_text(format_duration(track.duration_ms))
                        .frame_when_inactive(true)
                        .corner_radius(6.0)
                        .min_size(egui::vec2(ui.available_width(), 30.0));
                    if ui.add(b).clicked() {
                        state.player.jump_to_item(idx);
                    }
                }
            }
        });
}
