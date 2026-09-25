//! Font and cell geometry of a terminal view.

use egui::{FontFamily, FontId};

/// Font settings of a terminal view.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalFont {
    /// Font family; monospace by default.
    pub family: FontFamily,
    /// Font size in points.
    pub size: f32,
    /// Factor applied to the natural row height.
    pub line_height: f32,
}

impl Default for TerminalFont {
    fn default() -> Self {
        Self {
            family: FontFamily::Monospace,
            size: 13.0,
            line_height: 1.0,
        }
    }
}

impl TerminalFont {
    /// Font identifier used for drawing.
    pub fn font_id(&self) -> FontId {
        FontId::new(self.size, self.family.clone())
    }

    /// Width and height of one cell.
    pub fn cell_size(&self, ctx: &egui::Context) -> egui::Vec2 {
        let font_id = self.font_id();
        ctx.fonts_mut(|fonts| {
            let width = fonts.glyph_width(&font_id, 'M').max(1.0);
            let height = (fonts.row_height(&font_id) * self.line_height).max(1.0);
            egui::vec2(width, height)
        })
    }

    /// Grid size that fits into the given area.
    pub fn grid_size(&self, ctx: &egui::Context, area: egui::Vec2) -> (usize, usize) {
        let cell = self.cell_size(ctx);
        let columns = (area.x / cell.x).floor().max(2.0) as usize;
        let rows = (area.y / cell.y).floor().max(1.0) as usize;
        (columns, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_asks_for_a_family_every_context_knows() {
        let font = TerminalFont::default();
        assert_eq!(font.family, FontFamily::Monospace);
        assert_eq!(font.font_id().family, FontFamily::Monospace);
    }

    #[test]
    fn a_grid_never_falls_below_one_cell() {
        let context = egui::Context::default();
        let font = TerminalFont::default();

        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let (columns, rows) = font.grid_size(ui.ctx(), egui::vec2(1.0, 1.0));
            assert!(columns >= 2 && rows >= 1);
        });
        output.textures_delta.clear();
    }
}
