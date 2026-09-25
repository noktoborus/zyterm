//! Colors used to draw the terminal.

use egui::Color32;
use zyt_term::Color;

/// Palette of a terminal view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTheme {
    /// Default background.
    pub background: Color32,
    /// Default foreground.
    pub foreground: Color32,
    /// Bright default foreground.
    pub bright_foreground: Color32,
    /// Dim default foreground.
    pub dim_foreground: Color32,
    /// Cursor color.
    pub cursor: Color32,
    /// Background of selected cells.
    pub selection: Color32,
    /// Background of a search match that is not the current one.
    pub search_match: Color32,
    /// The sixteen palette entries.
    pub palette: [Color32; 16],
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl TerminalTheme {
    /// Palette for a dark interface.
    pub fn dark() -> Self {
        Self {
            background: Color32::from_rgb(0x12, 0x14, 0x18),
            foreground: Color32::from_rgb(0xd0, 0xd4, 0xda),
            bright_foreground: Color32::from_rgb(0xf0, 0xf2, 0xf5),
            dim_foreground: Color32::from_rgb(0x8a, 0x90, 0x99),
            cursor: Color32::from_rgb(0xd0, 0xd4, 0xda),
            selection: Color32::from_rgb(0x33, 0x3b, 0x47),
            search_match: Color32::from_rgb(0x5a, 0x49, 0x1e),
            palette: [
                Color32::from_rgb(0x1b, 0x1e, 0x24),
                Color32::from_rgb(0xd2, 0x5f, 0x5f),
                Color32::from_rgb(0x74, 0xb2, 0x6b),
                Color32::from_rgb(0xd0, 0xa3, 0x54),
                Color32::from_rgb(0x5c, 0x8f, 0xd6),
                Color32::from_rgb(0xa9, 0x78, 0xd8),
                Color32::from_rgb(0x4f, 0xaf, 0xb0),
                Color32::from_rgb(0xc2, 0xc7, 0xcf),
                Color32::from_rgb(0x4b, 0x52, 0x5d),
                Color32::from_rgb(0xf0, 0x7d, 0x7d),
                Color32::from_rgb(0x94, 0xd6, 0x8a),
                Color32::from_rgb(0xf0, 0xc0, 0x6a),
                Color32::from_rgb(0x7c, 0xaf, 0xf0),
                Color32::from_rgb(0xc8, 0x98, 0xf0),
                Color32::from_rgb(0x6f, 0xd0, 0xd2),
                Color32::from_rgb(0xf0, 0xf3, 0xf7),
            ],
        }
    }

    /// Palette for a light interface.
    pub fn light() -> Self {
        Self {
            background: Color32::from_rgb(0xfa, 0xfa, 0xf8),
            foreground: Color32::from_rgb(0x24, 0x28, 0x2e),
            bright_foreground: Color32::from_rgb(0x10, 0x12, 0x16),
            dim_foreground: Color32::from_rgb(0x60, 0x66, 0x70),
            cursor: Color32::from_rgb(0x24, 0x28, 0x2e),
            selection: Color32::from_rgb(0xcf, 0xdc, 0xf0),
            search_match: Color32::from_rgb(0xf3, 0xe0, 0x9a),
            palette: [
                Color32::from_rgb(0x2b, 0x2f, 0x36),
                Color32::from_rgb(0xb5, 0x32, 0x32),
                Color32::from_rgb(0x3f, 0x82, 0x3f),
                Color32::from_rgb(0x9a, 0x6d, 0x14),
                Color32::from_rgb(0x2a, 0x5a, 0xb5),
                Color32::from_rgb(0x7a, 0x3f, 0xb0),
                Color32::from_rgb(0x1f, 0x7d, 0x80),
                Color32::from_rgb(0x50, 0x56, 0x60),
                Color32::from_rgb(0x70, 0x76, 0x80),
                Color32::from_rgb(0xd0, 0x45, 0x45),
                Color32::from_rgb(0x4f, 0xa0, 0x4f),
                Color32::from_rgb(0xb8, 0x88, 0x20),
                Color32::from_rgb(0x36, 0x70, 0xd0),
                Color32::from_rgb(0x94, 0x52, 0xd0),
                Color32::from_rgb(0x28, 0x98, 0x9c),
                Color32::from_rgb(0x1a, 0x1c, 0x20),
            ],
        }
    }

    /// Resolves one cell color.
    pub fn resolve(&self, color: Color) -> Color32 {
        match color {
            Color::Palette(index) => self.palette[usize::from(index) & 0x0f],
            Color::Indexed(index) => self.indexed(index),
            Color::Rgb { r, g, b } => Color32::from_rgb(r, g, b),
            Color::Foreground => self.foreground,
            Color::Background => self.background,
            Color::BrightForeground => self.bright_foreground,
            Color::DimForeground => self.dim_foreground,
            Color::Cursor => self.cursor,
        }
    }

    /// Resolves one color of the 256 color cube.
    ///
    /// The first sixteen entries are the palette of this theme, so a theme
    /// paints `\x1b[38;5;1m` the same red as `\x1b[31m`.
    fn indexed(&self, index: u8) -> Color32 {
        match index {
            0..=15 => self.palette[usize::from(index)],
            16..=231 => {
                let value = index - 16;
                let steps = [0u8, 95, 135, 175, 215, 255];
                Color32::from_rgb(
                    steps[usize::from(value / 36) % 6],
                    steps[usize::from((value % 36) / 6) % 6],
                    steps[usize::from(value % 6)],
                )
            }
            _ => {
                let level = 8 + (index - 232) * 10;
                Color32::from_rgb(level, level, level)
            }
        }
    }
}
