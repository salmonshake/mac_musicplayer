mod fonts;
mod screenshot;
mod style;
mod state;
mod ui;

use eframe::egui;

struct MmpApp {
    state: Result<state::AppState, String>,
    shot: Option<screenshot::Screenshotter>,
    pending_album: bool,
}

impl eframe::App for MmpApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(app_state) = &mut self.state {
            app_state.poll(ctx);
        }
        if let (Some(shot), Ok(s)) = (&mut self.shot, &mut self.state) {
            if self.pending_album && !s.scanning {
                self.pending_album = false;
                if s.nav != state::Nav::Albums {
                    s.set_nav(state::Nav::Albums);
                }
                if let Some(k) = s.albums.first().map(|a| a.album_key.clone()) {
                    s.set_nav(state::Nav::Album(k));
                }
            }
            shot.tick(ctx, s.scanning);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts (before panels so they work regardless of focus).
        if let Ok(s) = &mut self.state {
            let typing = ui.ctx().egui_wants_keyboard_input();
            let (space, next, prev) = ui.input(|i| {
                (i.key_pressed(egui::Key::Space),
                 i.modifiers.command && i.key_pressed(egui::Key::ArrowRight),
                 i.modifiers.command && i.key_pressed(egui::Key::ArrowLeft))
            });
            if space && !typing {
                if s.now_track().is_some() {
                    s.player.toggle_pause();
                } else if let Some(i) = s.selected_track {
                    s.play_from(i);
                }
            }
            if next {
                s.player.next();
            }
            if prev {
                s.player.prev();
            }
        }
        // Toolbar (top panel)
        if let Ok(s) = &mut self.state {
            ui::toolbar::show(ui, s);
        }
        // Player bar (bottom panel — must be shown before other panels)
        if let Ok(s) = &mut self.state {
            ui::player_bar::show(ui, s);
        }
        // Sidebar (left panel)
        if let Ok(s) = &mut self.state {
            ui::sidebar::show(ui, s);
        }
        // Up Next (right panel)
        if let Ok(s) = &mut self.state
            && s.show_queue
        {
            ui::queue::show(ui, s);
        }
        // Central panel
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(16.0))
            .show(ui, |ui| {
                match &mut self.state {
                    Ok(s) => match s.nav.clone() {
                        state::Nav::AllTracks
                        | state::Nav::Artist(_)
                        | state::Nav::Genre(_) => {
                            ui::tracks::show(ui, s, true);
                        }
                    state::Nav::Albums => {
                        ui::albums::show_albums(ui, s);
                    }
                    state::Nav::Album(k) => {
                        ui::albums::show_album(ui, s, &k);
                    }
                    state::Nav::Artists => {
                        ui::albums::show_artists(ui, s);
                    }
                    state::Nav::Genres => {
                        ui::albums::show_genres(ui, s);
                    }
                },
                Err(e) => {
                    ui.label(format!("Error: {e}"));
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MacMusicPlay")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MacMusicPlay",
        options,
        Box::new(|cc| {
            style::apply(&cc.egui_ctx);
            let found = fonts::install_cjk_fonts(&cc.egui_ctx);
            if found.is_none() {
                eprintln!("CJK font not found");
            }
            let opened = state::AppState::open().map_err(|e| e.to_string());
            let shot = screenshot::Screenshotter::from_env();
            let pending_album = shot.is_some()
                && std::env::var("MMP_NAV")
                    .map(|v| v == "album")
                    .unwrap_or(false);
            let app_state = match opened {
                Ok(mut s) => {
                    match std::env::var("MMP_NAV").as_deref() {
                        Ok("albums") => s.set_nav(state::Nav::Albums),
                        Ok("artists") => s.set_nav(state::Nav::Artists),
                        Ok("genres") => s.set_nav(state::Nav::Genres),
                        _ => {}
                    }
                    Ok(s)
                }
                Err(e) => Err(e),
            };
            Ok(Box::new(MmpApp {
                state: app_state,
                shot,
                pending_album,
            }))
        }),
    )
}
