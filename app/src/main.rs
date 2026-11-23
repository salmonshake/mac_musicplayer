mod artwork;
mod fonts;
mod media;
mod screenshot;
mod state;
mod style;
mod ui;

use eframe::egui;

use media::MediaAction;
use state::{AppState, Nav};

struct MaplayApp {
    state: Result<AppState, String>,
    media: Option<media::MediaKeys>,
    dev: Option<screenshot::DevHooks>,
}

impl eframe::App for MaplayApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Ok(s) = &mut self.state else {
            return;
        };
        s.poll(ctx);
        if let Some(m) = &mut self.media {
            for ev in m.poll_events() {
                let dur = s.now_track().map_or(0, |t| t.duration_ms);
                if let Some(a) = media::media_action(&ev, s.now.position_ms, dur) {
                    s.apply_media_action(a);
                }
            }
            m.update(&s.now, s.now_track());
        }
        if let Some(dev) = &mut self.dev {
            dev.tick(ctx, s);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let s = match &mut self.state {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("Error: {e}");
                egui::CentralPanel::default().show(ui, |ui| ui.label(msg));
                return;
            }
        };
        // Before the panels so shortcuts work regardless of focus.
        shortcuts(ui, s);
        // Side panels first (the bottom bar before the others), then the centre.
        ui::toolbar::show(ui, s);
        ui::player_bar::show(ui, s);
        ui::sidebar::show(ui, s);
        if s.show_queue {
            ui::queue::show(ui, s);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(16.0))
            .show(ui, |ui| match s.nav.clone() {
                Nav::AllTracks | Nav::Artist(_) | Nav::Genre(_) | Nav::Playlist(_) => {
                    ui::tracks::show(ui, s, true);
                }
                Nav::Albums => ui::albums::show_albums(ui, s),
                Nav::Album(k) => ui::albums::show_album(ui, s, &k),
                Nav::Artists => ui::albums::show_artists(ui, s),
                Nav::Genres => ui::albums::show_genres(ui, s),
            });
    }
}

/// Space: play/pause (not while typing); ⌘→ / ⌘←: next / previous.
fn shortcuts(ui: &egui::Ui, s: &mut AppState) {
    let typing = ui.ctx().egui_wants_keyboard_input();
    let (space, next, prev) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Space),
            i.modifiers.command && i.key_pressed(egui::Key::ArrowRight),
            i.modifiers.command && i.key_pressed(egui::Key::ArrowLeft),
        )
    });
    if space && !typing {
        s.apply_media_action(MediaAction::Toggle);
    }
    if next {
        s.apply_media_action(MediaAction::Next);
    }
    if prev {
        s.apply_media_action(MediaAction::Prev);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Maplay")
            .with_inner_size(screenshot::window_size().unwrap_or([1200.0, 760.0]))
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Maplay",
        options,
        Box::new(|cc| {
            style::apply(&cc.egui_ctx);
            if fonts::install_cjk_fonts(&cc.egui_ctx).is_none() {
                eprintln!("CJK font not found");
            }
            let dev = screenshot::DevHooks::from_env();
            let mut state = AppState::open(&cc.egui_ctx);
            if let (Some(dev), Ok(s)) = (&dev, &mut state) {
                dev.apply_startup(&cc.egui_ctx, s);
            }
            Ok(Box::new(MaplayApp {
                state,
                media: media::MediaKeys::new(cc.egui_ctx.clone()),
                dev,
            }))
        }),
    )
}
