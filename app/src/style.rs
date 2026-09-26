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
        s.text_styles.insert(
            TextStyle::Body,
            FontId::new(14.0, FontFamily::Proportional),
        );
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
    });
}
