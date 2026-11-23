use eframe::egui::{self, FontFamily, FontId, TextStyle, vec2};

/// Table row height (mockup)
pub const ROW_HEIGHT: f32 = 34.0;

/// Table header height (mockup)
pub const HEADER_HEIGHT: f32 = 30.0;

/// Sidebar panel width (mockup)
pub const SIDEBAR_WIDTH: f32 = 230.0;

/// Panel inner margin (mockup)
pub const PANEL_MARGIN: f32 = 14.0;

/// Apply font sizes and spacing to match the GUI mockup (docs/gui-mockup.webp).
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.text_styles.insert(
            TextStyle::Heading,
            FontId::new(26.0, FontFamily::Proportional),
        );
        s.text_styles
            .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        );
        s.spacing.item_spacing = vec2(10.0, 8.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        // Rounded look throughout (user request).
        let r = egui::CornerRadius::same(8);
        s.visuals.widgets.noninteractive.corner_radius = r;
        s.visuals.widgets.inactive.corner_radius = r;
        s.visuals.widgets.hovered.corner_radius = r;
        s.visuals.widgets.active.corner_radius = r;
        s.visuals.widgets.open.corner_radius = r;
        s.visuals.window_corner_radius = egui::CornerRadius::same(12);
        s.visuals.menu_corner_radius = egui::CornerRadius::same(10);
        s.visuals.handle_shape = egui::style::HandleShape::Circle;
        // Labels are not selectable: clicks and drags go to the row, not text selection.
        s.interaction.selectable_labels = false;
        s.interaction.multi_widget_text_select = false;
    });
}

/// Translucent fill that works on both themes: white on dark, black on light.
pub fn subtle_fill(ui: &egui::Ui, alpha: u8) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_white_alpha(alpha)
    } else {
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, alpha)
    }
}

/// Accent for filled badges: white text on it has about 5.7:1 contrast (WCAG AA).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(26, 100, 200);

/// Accent for text and icons on the window background (at least 4.5:1 on both themes).
pub fn accent_text(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(110, 168, 255)
    } else {
        ACCENT
    }
}

/// Row tint for the playing track; stronger when the row is also selected.
pub fn playing_fill(ui: &egui::Ui, selected: bool) -> egui::Color32 {
    let a = match (ui.visuals().dark_mode, selected) {
        (true, false) => 70,
        (true, true) => 115,
        (false, false) => 38,
        (false, true) => 70,
    };
    egui::Color32::from_rgba_unmultiplied(26, 100, 200, a)
}

/// Small rounded pill with white text on ACCENT, sized to its text.
pub fn badge(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(10.5),
        egui::Color32::WHITE,
    );
    let size = galley.size() + egui::vec2(12.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, rect.height() / 2.0, ACCENT);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
    resp
}
