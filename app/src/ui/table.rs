//! Shared pieces for the list tables: our table setup, header row, and
//! rounded row backgrounds (egui_extras only paints square ones).

use eframe::egui;
use egui_extras::{TableBuilder, TableRow};

use crate::style;

/// Row background colours for the current theme.
#[derive(Clone, Copy)]
pub struct RowColors {
    stripe: egui::Color32,
    hover: egui::Color32,
    selected: egui::Color32,
    playing: egui::Color32,
    playing_selected: egui::Color32,
}

impl RowColors {
    fn new(ui: &egui::Ui) -> Self {
        Self {
            stripe: style::subtle_fill(ui, 8),
            hover: style::subtle_fill(ui, 18),
            selected: style::subtle_fill(ui, 24),
            playing: style::playing_fill(ui, false),
            playing_selected: style::playing_fill(ui, true),
        }
    }

    /// Fill for row `i`: playing / selected first, then hover, then stripes.
    pub fn fill(&self, i: usize, hovered: bool, selected: bool, playing: bool) -> egui::Color32 {
        match (playing, selected) {
            (true, true) => self.playing_selected,
            (true, false) => self.playing,
            (false, true) => self.selected,
            _ if hovered => self.hover,
            _ if i % 2 == 1 => self.stripe,
            _ => egui::Color32::TRANSPARENT,
        }
    }
}

/// A table with clickable rows and vertically centred cells, without
/// egui_extras' own stripes and square hover fill. Also returns the rows'
/// x-range (for `RowBackground::reserve`) and the row colours.
pub fn builder(ui: &mut egui::Ui) -> (TableBuilder<'_>, egui::Rangef, RowColors) {
    let colors = RowColors::new(ui);
    let row_x = ui.max_rect().x_range();
    ui.visuals_mut().widgets.hovered.bg_fill = egui::Color32::TRANSPARENT;
    let builder = TableBuilder::new(ui)
        .striped(false)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    (builder, row_x, colors)
}

/// Header row of dimmed column titles.
pub fn header_row(mut h: TableRow<'_, '_>, labels: &[&str]) {
    for &t in labels {
        h.col(|ui| {
            ui.label(egui::RichText::new(t).weak());
        });
    }
}

/// Centred message for an empty list.
pub fn empty_message(ui: &mut egui::Ui, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(text);
    });
}

/// A slot reserved under a row's contents, filled once the row's rect is known.
pub struct RowBackground {
    painter: egui::Painter,
    idx: egui::layers::ShapeIdx,
}

impl RowBackground {
    /// Call at the start of the row's first cell.
    pub fn reserve(ui: &egui::Ui, row_x: egui::Rangef) -> Self {
        let mut painter = ui.painter().clone();
        // The cell's clip covers only its column; widen it to the whole row.
        painter.set_clip_rect(egui::Rect::from_x_y_ranges(row_x, ui.clip_rect().y_range()));
        let idx = painter.add(egui::Shape::Noop);
        Self { painter, idx }
    }

    /// Paints a rounded background behind the whole row.
    pub fn paint(self, row: &egui::Response, fill: egui::Color32) {
        let rect = row.rect.shrink2(egui::vec2(0.0, 1.0));
        self.painter
            .set(self.idx, egui::Shape::rect_filled(rect, 8.0, fill));
    }
}
