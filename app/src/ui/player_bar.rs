//! Bottom player bar: track info, transport buttons, volume, seek bar.

use eframe::egui;
use egui::Color32;
use mmp_core::player::{PlayState, RepeatMode};
use crate::state::AppState;

/// Icon helper: returns a RichText with color based on whether the feature is active.
fn icon(s: &str, size: f32, on: bool, accent: Color32, normal: Color32) -> egui::RichText {
    egui::RichText::new(s).size(size).color(if on { accent } else { normal })
}

/// "44.1 kHz" / "96 kHz" style label for a sample rate in Hz.
fn khz(rate: f64) -> String {
    let k = rate / 1000.0;
    if (k - k.round()).abs() < 0.05 {
        format!("{} kHz", k.round() as u32)
    } else {
        format!("{k:.1} kHz")
    }
}

/// Show the bottom player bar with transport controls, volume, and seek bar.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::bottom("player_bar")
        .exact_size(100.0)
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 10)))
        .show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let left = egui::Rect::from_min_max(
                full.min,
                egui::pos2(full.min.x + 320.0, full.max.y),
            );
            let right = egui::Rect::from_min_max(
                egui::pos2(full.max.x - 260.0, full.min.y),
                full.max,
            );
            let center = egui::Rect::from_min_max(
                egui::pos2(left.max.x, full.min.y),
                egui::pos2(right.min.x, full.max.y),
            );

            // --- left: artwork placeholder + track info (vertically centered) ---
            let bp_label: Option<String> = if state.now.bit_perfect {
                let label = match state.now.device_rate {
                    Some(r) => format!("Bit-perfect · {}", khz(r)),
                    None => "Bit-perfect".to_string(),
                };
                Some(label)
            } else {
                None
            };

            ui.scope_builder(
                egui::UiBuilder::new().max_rect(left).layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    let now = state.now_track().map(|t| (t.album_key.clone(), t.path.clone()));
                    let tex = now.and_then(|(k, p)| state.art.get(&k, &p));
                    crate::artwork::art_image(ui, tex.as_ref(), 56.0);
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        if let Some(ref label) = bp_label {
                            ui.label(egui::RichText::new(label).small().color(crate::style::accent_text(ui)));
                        }
                        match state.now_track() {
                            Some(t) => {
                                    ui.add(egui::Label::new(egui::RichText::new(&t.title).strong().size(15.0)).truncate())
                                    .on_hover_text(&t.title);
                                ui.add(egui::Label::new(egui::RichText::new(&t.artist).weak()).truncate());
                            }
                            None => {
                                ui.label(egui::RichText::new("Not playing").weak());
                            }
                        }
                    });
                },
            );

            // --- center: transport buttons (centered) + seek bar ---
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(center).layout(egui::Layout::top_down(egui::Align::Center)),
                |ui| {
                    let has_track = state.now_track().is_some();
                    let playing = state.now.state == PlayState::Playing;
                    let accent = ui.visuals().selection.bg_fill;
                    let normal = ui.visuals().text_color();

                    ui.allocate_ui_with_layout(
                        egui::vec2(280.0, 40.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;

                            if ui.add(egui::Button::new(icon("🔀", 18.0, state.now.shuffle, accent, normal)).frame(false))
                                .on_hover_text("Shuffle")
                                .clicked()
                            {
                                state.player.set_shuffle(!state.now.shuffle);
                            }

                            if crate::ui::icons::icon_button(ui, 22.0, has_track, |p, r, c| crate::ui::icons::skip(p, r, c, false))
                                .on_hover_text("Previous")
                                .clicked()
                            {
                                state.player.prev();
                            }
                            let play_resp = crate::ui::icons::icon_button(ui, 34.0, has_track, |p, r, c| {
                                if playing {
                                    crate::ui::icons::pause(p, r, c)
                                } else {
                                    crate::ui::icons::play(p, r, c)
                                }
                            });
                            if play_resp.on_hover_text(if playing { "Pause" } else { "Play" }).clicked() {
                                state.player.toggle_pause();
                            }
                            if crate::ui::icons::icon_button(ui, 22.0, has_track, |p, r, c| crate::ui::icons::skip(p, r, c, true))
                                .on_hover_text("Next")
                                .clicked()
                            {
                                state.player.next();
                            }

                            let (sym, next_mode, tip) = match state.now.repeat {
                                RepeatMode::Off => ("🔁", RepeatMode::All, "Repeat: off"),
                                RepeatMode::All => ("🔁", RepeatMode::One, "Repeat: all"),
                                RepeatMode::One => ("🔂", RepeatMode::Off, "Repeat: one"),
                            };
                            let repeat_btn = egui::Button::new(icon(
                                sym,
                                18.0,
                                state.now.repeat != RepeatMode::Off,
                                accent,
                                normal,
                            )).frame(false);
                            if ui.add(repeat_btn)
                                .on_hover_text(tip)
                                .clicked()
                            {
                                state.player.set_repeat(next_mode);
                            }
                        },
                    );

                    ui.add_space(2.0);

                    let w = (center.width() - 140.0).clamp(200.0, 520.0);
                    seek_bar(ui, state, w);
                },
            );

            // --- right: queue toggle + volume slider ---
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    let accent = ui.visuals().selection.bg_fill;
                    let q_color = if state.show_queue { accent } else { ui.visuals().text_color() };

                    if ui.add(egui::Button::new(egui::RichText::new("☰").size(18.0).color(q_color)).frame(false))
                        .on_hover_text("Up Next")
                        .clicked()
                    {
                        state.show_queue = !state.show_queue;
                    }

                    ui.add_space(8.0);

                    let fixed = state.now.bit_perfect;
                    let mut vol = if fixed { 100.0 } else { (state.now.volume * 100.0).round() };
                    let r = ui
                        .add_enabled(!fixed, egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false))
                        .on_disabled_hover_text("Volume is fixed at 100% in bit-perfect mode");
                    if r.changed() {
                        state.player.set_volume(vol / 100.0);
                    }
                    ui.label(egui::RichText::new(format!("{}%", vol as i32)).weak());
                },
            );
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

    ui.allocate_ui_with_layout(
        egui::vec2(width + 110.0, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new(crate::ui::tracks::format_duration(shown)).weak().small());

            let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::click_and_drag());
            let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
            let track_bg = crate::style::subtle_fill(ui, 40);
            let p = ui.painter();
            p.rect_filled(bar, 2.0, track_bg);

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
        },
    );
}

#[cfg(test)]
mod tests {
    use super::khz;

    #[test]
    fn khz_labels() {
        assert_eq!(khz(44_100.0), "44.1 kHz");
        assert_eq!(khz(48_000.0), "48 kHz");
        assert_eq!(khz(96_000.0), "96 kHz");
        assert_eq!(khz(88_200.0), "88.2 kHz");
    }
}
