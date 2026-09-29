//! Top toolbar with search box and status text.

use eframe::egui;
use crate::state::{AppState, Nav};
use crate::ui::tracks::count;

/// Show the top toolbar panel with a search field and status text.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::top("toolbar")
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 8)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut q = state.search.clone();
                let id = egui::Id::new("search_box");
                let resp = ui
                    .add(
                        egui::TextEdit::singleline(&mut q)
                            .id(id)
                            .hint_text("Search")
                            .desired_width(320.0),
                    );
                // ⌘F: focus the search box
                if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                    resp.request_focus();
                }
                // Esc while focused: clear
                if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    q.clear();
                }
                // Push back to state when changed
                if q != state.search {
                    state.set_search(q);
                }

                // Status text on the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mode = state.settings.theme;
                    let (icon, name) = match mode {
                        mmp_core::settings::ThemeMode::System => ("◐", "System"),
                        mmp_core::settings::ThemeMode::Light => ("☀", "Light"),
                        mmp_core::settings::ThemeMode::Dark => ("🌙", "Dark"),
                    };
                    let clicked = ui
                        .add(egui::Button::new(egui::RichText::new(icon).size(16.0)).frame(false))
                        .on_hover_text(format!("Theme: {name}"))
                        .clicked();
                    if clicked {
                        let ctx = ui.ctx().clone();
                        state.set_theme(&ctx, mode.next());
                    }
                    // ⚙ settings menu (flat, no border)
                    ui.add_space(8.0);
                    // ⚙ settings menu: stays open while toggling options
                    let gear = egui::Button::new(egui::RichText::new("⚙").size(16.0)).frame(false);
                    egui::containers::menu::MenuButton::from_button(gear)
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            let available = state.now.bit_perfect_available;
                            let mut bp = state.settings.bit_perfect;
                            let r = ui
                                .add_enabled(available || bp, egui::Checkbox::new(&mut bp, "Bit-perfect mode"))
                                .on_hover_text("Match the device sample rate to each track; volume fixed at 100%");
                            if r.changed() {
                                state.set_bit_perfect(bp);
                            }
                            if !available {
                                ui.label(
                                    egui::RichText::new("Not available on Bluetooth or AirPlay outputs")
                                        .small()
                                        .weak(),
                                );
                            }
                            let mut hog = state.settings.hog_mode;
                            let r = ui
                                .add_enabled(state.settings.bit_perfect, egui::Checkbox::new(&mut hog, "Exclusive access (hog mode)"))
                                .on_hover_text("Other apps cannot play sound while this app uses the device");
                            if r.changed() {
                                state.set_hog_mode(hog);
                            }
                            ui.separator();
                            ui.label(egui::RichText::new("Output").small().strong().weak());
                            // Only queried while the menu is open.
                            let devices = mmp_core::coreaudio::device::output_devices().unwrap_or_default();
                            let current = state.settings.output_device.clone();
                            if ui.radio(current.is_none(), "System default").clicked() && current.is_some() {
                                state.set_output_device(None);
                            }
                            for d in &devices {
                                let selected = current.as_deref() == Some(d.uid.as_str());
                                let label = if d.is_bit_perfect_capable() {
                                    d.name.clone()
                                } else {
                                    format!("{} (wireless)", d.name)
                                };
                                if ui.radio(selected, label).clicked() && !selected {
                                    state.set_output_device(Some(d.uid.clone()));
                                }
                            }
                        });
                    ui.add_space(12.0);
                    ui.label(status_text(state));
                });
            });
        });
}

/// Status text: state.status when non-empty, otherwise view-dependent count.
fn status_text(state: &AppState) -> String {
    if !state.status.is_empty() {
        return state.status.clone();
    }
    match &state.nav {
        Nav::AllTracks
        | Nav::Album(_)
        | Nav::Artist(_)
        | Nav::Genre(_)
        | Nav::Playlist(_) => {
            count(state.tracks.len(), "track", "tracks")
        }
        Nav::Albums => {
            count(state.albums.len(), "album", "albums")
        }
        Nav::Artists => {
            count(state.artists.len(), "artist", "artists")
        }
        Nav::Genres => {
            count(state.genres.len(), "genre", "genres")
        }
    }
}
