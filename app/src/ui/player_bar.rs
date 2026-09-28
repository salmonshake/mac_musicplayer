//! Bottom player bar: track info, transport buttons, volume, seek bar.

use eframe::egui;
use mmp_core::player::{PlayState, RepeatMode};
use crate::state::AppState;

/// Show the bottom player bar with transport controls, volume, and seek bar.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::bottom("player_bar")
        .exact_size(96.0)
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

                // --- center: transport buttons + seek bar ---
                let has_track = state.now_track().is_some();
                let playing = state.now.state == PlayState::Playing;

                ui.vertical(|ui| {
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
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
                    });
                    seek_bar(ui, state, 360.0);
                });

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

/// Elapsed time, seek bar, and total duration.
/// Seeks once on click or drag end (during drag, only follow position).
fn seek_bar(ui: &mut egui::Ui, state: &mut AppState, width: f32) {
    let duration = state.now_track().map(|t| t.duration_ms).unwrap_or(0);
    let pos = state.now.position_ms.min(duration);
    let id = egui::Id::new("seek_drag");
    let dragging: Option<u64> = ui.data(|d| d.get_temp::<u64>(id));
    let shown = dragging.unwrap_or(pos);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(crate::ui::tracks::format_duration(shown)).weak().small());

        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::click_and_drag());
        let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
        let p = ui.painter();
        p.rect_filled(bar, 2.0, egui::Color32::from_white_alpha(40));

        let frac = if duration > 0 { shown as f32 / duration as f32 } else { 0.0 };
        let mut filled = bar;
        filled.set_width(bar.width() * frac.clamp(0.0, 1.0));
        p.rect_filled(filled, 2.0, ui.visuals().selection.bg_fill);

        let to_ms = |x: f32| -> u64 {
            (((x - bar.left()) / bar.width()).clamp(0.0, 1.0) * duration as f32) as u64
        };

        if duration > 0 {
            if let Some(ptr) = resp.interact_pointer_pos()
                && resp.dragged()
            {
                ui.data_mut(|d| d.insert_temp(id, to_ms(ptr.x)));
            }
            if resp.drag_stopped() || resp.clicked() {
                let target = resp.interact_pointer_pos()
                    .map(|p| to_ms(p.x))
                    .or(dragging);
                if let Some(ms) = target {
                    state.player.seek_ms(ms);
                }
                ui.data_mut(|d| d.remove_temp::<u64>(id));
            }
        }

        ui.label(egui::RichText::new(crate::ui::tracks::format_duration(duration)).weak().small());
    });
}
