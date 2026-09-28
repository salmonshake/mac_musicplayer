//! Bottom player bar: track info, transport buttons, volume.

use eframe::egui;
use mmp_core::player::{PlayState, RepeatMode};
use crate::state::AppState;

/// Show the bottom player bar with transport controls and volume.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::bottom("player_bar")
        .exact_size(84.0)
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 10)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // --- left: artwork placeholder + track info (width 260) ---
                let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 6.0, egui::Color32::from_white_alpha(20));
                ui.allocate_ui(egui::vec2(260.0, 44.0), |ui| {
                    ui.vertical(|ui| {
                        match state.now_track() {
                            Some(t) => {
                                ui.label(egui::RichText::new(&t.title).strong());
                                ui.label(egui::RichText::new(&t.artist).weak());
                            }
                            None => {
                                ui.label(egui::RichText::new("Not playing").weak());
                            }
                        }
                    });
                });

                // --- center: transport buttons ---
                let has_track = state.now_track().is_some();
                let playing = state.now.state == PlayState::Playing;

                ui.add_space(24.0);

                if ui.add(egui::Button::new("🔀").selected(state.now.shuffle))
                    .on_hover_text("Shuffle")
                    .clicked()
                {
                    state.player.set_shuffle(!state.now.shuffle);
                }

                if ui.add_enabled(has_track, egui::Button::new("⏮"))
                    .on_hover_text("Previous")
                    .clicked()
                {
                    state.player.prev();
                }

                let play_label = if playing { "⏸" } else { "▶" };
                if ui.add_enabled(has_track, egui::Button::new(play_label).min_size(egui::vec2(40.0, 32.0)))
                    .on_hover_text(if playing { "Pause" } else { "Play" })
                    .clicked()
                {
                    state.player.toggle_pause();
                }

                if ui.add_enabled(has_track, egui::Button::new("⏭"))
                    .on_hover_text("Next")
                    .clicked()
                {
                    state.player.next();
                }

                let (icon, next_mode, tip) = match state.now.repeat {
                    RepeatMode::Off => ("🔁", RepeatMode::All, "Repeat: off"),
                    RepeatMode::All => ("🔁", RepeatMode::One, "Repeat: all"),
                    RepeatMode::One => ("🔂", RepeatMode::Off, "Repeat: one"),
                };
                if ui.add(egui::Button::new(icon).selected(state.now.repeat != RepeatMode::Off))
                    .on_hover_text(tip)
                    .clicked()
                {
                    state.player.set_repeat(next_mode);
                }

                // --- right: volume slider ---
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut vol = (state.now.volume * 100.0).round();
                    if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false))
                        .changed()
                    {
                        state.player.set_volume(vol / 100.0);
                    }
                    ui.label(format!("{}%", vol as i32));
                });
            });
        });
}
