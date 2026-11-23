//! Bottom player bar: track info, transport buttons, seek bar, volume.

use eframe::egui;
use maplay_core::player::{PlayState, RepeatMode};

use crate::state::AppState;
use crate::ui::icons;
use crate::ui::tracks::format_duration;

/// Width of the track-info area on the left.
const LEFT_WIDTH: f32 = 320.0;
/// Width of the volume / queue area on the right.
const RIGHT_WIDTH: f32 = 260.0;

/// "44.1 kHz" / "96 kHz" style label for a sample rate in Hz.
fn khz(rate: f64) -> String {
    let k = rate / 1000.0;
    if (k - k.round()).abs() < 0.05 {
        format!("{} kHz", k.round() as u32)
    } else {
        format!("{k:.1} kHz")
    }
}

/// Show the bottom player bar.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::bottom("player_bar")
        .exact_size(100.0)
        .frame(
            egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let left =
                egui::Rect::from_min_max(full.min, egui::pos2(full.min.x + LEFT_WIDTH, full.max.y));
            let right = egui::Rect::from_min_max(
                egui::pos2(full.max.x - RIGHT_WIDTH, full.min.y),
                full.max,
            );
            let center = egui::Rect::from_min_max(
                egui::pos2(left.max.x, full.min.y),
                egui::pos2(right.min.x, full.max.y),
            );
            let region = |rect, layout| egui::UiBuilder::new().max_rect(rect).layout(layout);

            ui.scope_builder(
                region(left, egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    track_info(ui, state);
                },
            );
            ui.scope_builder(
                region(center, egui::Layout::top_down(egui::Align::Center)),
                |ui| {
                    transport(ui, state);
                    ui.add_space(2.0);
                    let width = (center.width() - 140.0).clamp(200.0, 520.0);
                    seek_bar(ui, state, width);
                },
            );
            ui.scope_builder(
                region(right, egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    if toggle_icon(ui, "☰", state.show_queue, "Up Next").clicked() {
                        state.show_queue = !state.show_queue;
                    }
                    ui.add_space(8.0);
                    volume(ui, state);
                },
            );
        });
}

/// Artwork, bit-perfect badge, title and artist of the playing track.
fn track_info(ui: &mut egui::Ui, state: &mut AppState) {
    let art_source = state
        .now_track()
        .map(|t| (t.album_key.clone(), t.path.clone()));
    let tex = art_source.and_then(|(k, p)| state.art.get(&k, &p));
    crate::artwork::art_image(ui, tex.as_ref(), 56.0);
    ui.add_space(12.0);
    ui.vertical(|ui| {
        ui.add_space(6.0);
        if state.now.bit_perfect {
            let label = match state.now.device_rate {
                Some(r) => format!("Bit-perfect · {}", khz(r)),
                None => "Bit-perfect".to_string(),
            };
            ui.label(
                egui::RichText::new(label)
                    .small()
                    .color(crate::style::accent_text(ui)),
            );
        }
        match state.now_track() {
            Some(t) => {
                ui.add(
                    egui::Label::new(egui::RichText::new(&t.title).strong().size(15.0)).truncate(),
                )
                .on_hover_text(&t.title);
                ui.add(egui::Label::new(egui::RichText::new(&t.artist).weak()).truncate());
            }
            None => {
                ui.label(egui::RichText::new("Not playing").weak());
            }
        }
    });
}

/// Frameless glyph button, drawn in the accent colour while `on`.
fn toggle_icon(ui: &mut egui::Ui, glyph: &str, on: bool, tip: &str) -> egui::Response {
    let color = if on {
        crate::style::accent_text(ui)
    } else {
        ui.visuals().text_color()
    };
    ui.add(egui::Button::new(egui::RichText::new(glyph).size(18.0).color(color)).frame(false))
        .on_hover_text(tip)
}

/// Shuffle, previous, play/pause, next, repeat.
fn transport(ui: &mut egui::Ui, state: &mut AppState) {
    let has_track = state.now_track().is_some();
    let playing = state.now.state == PlayState::Playing;
    ui.allocate_ui_with_layout(
        egui::vec2(280.0, 40.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 18.0;

            if toggle_icon(ui, "🔀", state.now.shuffle, "Shuffle").clicked() {
                state.player.set_shuffle(!state.now.shuffle);
            }
            let prev =
                icons::icon_button(ui, 22.0, has_track, |p, r, c| icons::skip(p, r, c, false));
            if prev.on_hover_text("Previous").clicked() {
                state.player.prev();
            }
            let play = icons::icon_button(ui, 34.0, has_track, |p, r, c| {
                if playing {
                    icons::pause(p, r, c)
                } else {
                    icons::play(p, r, c)
                }
            });
            if play
                .on_hover_text(if playing { "Pause" } else { "Play" })
                .clicked()
            {
                state.player.toggle_pause();
            }
            let next =
                icons::icon_button(ui, 22.0, has_track, |p, r, c| icons::skip(p, r, c, true));
            if next.on_hover_text("Next").clicked() {
                state.player.next();
            }

            let (glyph, next_mode, tip) = match state.now.repeat {
                RepeatMode::Off => ("🔁", RepeatMode::All, "Repeat: off"),
                RepeatMode::All => ("🔁", RepeatMode::One, "Repeat: all"),
                RepeatMode::One => ("🔂", RepeatMode::Off, "Repeat: one"),
            };
            if toggle_icon(ui, glyph, state.now.repeat != RepeatMode::Off, tip).clicked() {
                state.player.set_repeat(next_mode);
            }
        },
    );
}

/// Volume slider (fixed at 100% in bit-perfect mode) and its percentage.
fn volume(ui: &mut egui::Ui, state: &mut AppState) {
    let fixed = state.now.bit_perfect;
    let mut vol = if fixed {
        100.0
    } else {
        (state.now.volume * 100.0).round()
    };
    let r = ui
        .add_enabled(
            !fixed,
            egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false),
        )
        .on_disabled_hover_text("Volume is fixed at 100% in bit-perfect mode");
    if r.changed() {
        state.player.set_volume(vol / 100.0);
    }
    ui.label(egui::RichText::new(format!("{}%", vol as i32)).weak());
}

/// Elapsed time, seek bar, and total duration.
/// Seeks once on click or drag end (while dragging only the shown position follows).
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
            ui.label(egui::RichText::new(format_duration(shown)).weak().small());

            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::click_and_drag());
            let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
            let frac = if duration > 0 {
                shown as f32 / duration as f32
            } else {
                0.0
            };
            let mut filled = bar;
            filled.set_width(bar.width() * frac.clamp(0.0, 1.0));
            let track_bg = crate::style::subtle_fill(ui, 40);
            ui.painter().rect_filled(bar, 2.0, track_bg);
            ui.painter().rect_filled(filled, 2.0, crate::style::ACCENT);

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
                    let target = resp.interact_pointer_pos().map(|p| to_ms(p.x)).or(dragging);
                    if let Some(ms) = target {
                        state.player.seek_ms(ms);
                    }
                    ui.data_mut(|d| d.remove_temp::<u64>(id));
                }
            }

            ui.label(
                egui::RichText::new(format_duration(duration))
                    .weak()
                    .small(),
            );
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
