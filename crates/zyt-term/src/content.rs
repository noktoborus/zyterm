//! Snapshot types handed to a renderer. They contain no types of the
//! emulation backend.

/// Palette entry of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Entry of the sixteen color palette.
    Palette(u8),
    /// Entry of the 256 color palette.
    Indexed(u8),
    /// Direct color value.
    Rgb {
        /// Red channel.
        r: u8,
        /// Green channel.
        g: u8,
        /// Blue channel.
        b: u8,
    },
    /// Default foreground of the theme.
    Foreground,
    /// Default background of the theme.
    Background,
    /// Bright default foreground of the theme.
    BrightForeground,
    /// Dim default foreground of the theme.
    DimForeground,
    /// Cursor color of the theme.
    Cursor,
}

/// Rendering attributes of a cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellStyle {
    /// Bold text.
    pub bold: bool,
    /// Dim text.
    pub dim: bool,
    /// Italic text.
    pub italic: bool,
    /// Any underline variant.
    pub underline: bool,
    /// Struck out text.
    pub strikeout: bool,
    /// Foreground and background are exchanged.
    pub inverse: bool,
    /// Text is not drawn.
    pub hidden: bool,
    /// Cell holds the left half of a double width character.
    pub wide: bool,
    /// Cell is the spacer of a double width character.
    pub wide_spacer: bool,
}

/// A color as the program gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

/// Position of a hyperlink in the link table of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkId(pub u16);

/// One grid cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Character of the cell.
    pub ch: char,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Rendering attributes.
    pub style: CellStyle,
    /// Cell is part of the current selection.
    pub selected: bool,
    /// Cell is part of a search match other than the current one.
    pub matched: bool,
    /// Hyperlink of the cell, an index into the link table of the snapshot.
    pub link: Option<LinkId>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Foreground,
            bg: Color::Background,
            style: CellStyle::default(),
            selected: false,
            matched: false,
            link: None,
        }
    }
}

/// Shape of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    /// Filled block.
    Block,
    /// Vertical bar.
    Beam,
    /// Horizontal bar.
    Underline,
    /// Block outline, used when the widget has no focus.
    Hollow,
    /// Cursor is hidden.
    Hidden,
}

/// Cursor position and shape in viewport coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorInfo {
    /// Column of the cursor.
    pub column: usize,
    /// Row of the cursor inside the viewport.
    pub row: usize,
    /// Shape of the cursor.
    pub shape: CursorShape,
}

/// Terminal modes a renderer or an input encoder has to know about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalModes {
    /// Cursor keys send application sequences.
    pub app_cursor: bool,
    /// Keypad sends application sequences.
    pub app_keypad: bool,
    /// Pasted text is wrapped in bracketed paste markers.
    pub bracketed_paste: bool,
    /// The alternate screen is shown.
    pub alt_screen: bool,
    /// Any mouse reporting mode is active.
    pub mouse_report: bool,
    /// Mouse reports use the SGR encoding.
    pub sgr_mouse: bool,
    /// Mouse motion is reported while a button is held.
    pub mouse_drag: bool,
    /// Mouse motion is reported at all times.
    pub mouse_motion: bool,
}

/// Full snapshot of the visible terminal state.
#[derive(Debug, Clone, Default)]
pub struct RenderableContent {
    /// Number of columns.
    pub columns: usize,
    /// Number of rows.
    pub rows: usize,
    /// Cells in row major order, `rows * columns` entries.
    pub cells: Vec<Cell>,
    /// Cursor state.
    pub cursor: Option<CursorInfo>,
    /// Lines currently scrolled back.
    pub display_offset: usize,
    /// Lines available in the scrollback buffer.
    pub history_size: usize,
    /// Active terminal modes.
    pub modes: TerminalModes,
    /// Where a selection would begin, as a column and a row of the page.
    ///
    /// It is a place between two characters and not a character: the bar drawn
    /// for it stands before the one it names, which is the first one a
    /// selection started there would take. Nothing when no press has said
    /// where, or when the line it was put on has been scrolled off the page.
    pub selection_anchor: Option<(usize, usize)>,
    /// Every hyperlink of the snapshot, addressed by [`LinkId`].
    pub links: Vec<String>,
    /// Background the program asked for, when it asked for one.
    pub background: Option<Rgb>,
    /// Foreground the program asked for, when it asked for one.
    pub foreground: Option<Rgb>,
    /// Cursor color the program asked for, when it asked for one.
    pub cursor_color: Option<Rgb>,
    /// The colors of the 256 color table a program painted over, by their
    /// index, and nothing where it left one alone.
    ///
    /// Either [`PALETTE_COLORS`] entries or none at all: a snapshot nobody
    /// repainted carries an empty table rather than 256 times nothing. What a
    /// program did not set is the color of the theme, and so is every entry
    /// while the caller refuses a program the palette.
    pub palette: Vec<Option<Rgb>>,
}

/// Entries of the color table a program may paint over, which is the 256 color
/// cube and not the named colors behind it.
pub const PALETTE_COLORS: usize = 256;

impl RenderableContent {
    /// Cell at the given viewport position.
    pub fn cell(&self, column: usize, row: usize) -> Option<&Cell> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        self.cells.get(row * self.columns + column)
    }

    /// Rows as slices, top to bottom.
    pub fn rows(&self) -> impl Iterator<Item = &[Cell]> {
        self.cells.chunks(self.columns.max(1))
    }

    /// Hyperlink of a cell, when it has one.
    pub fn link_at(&self, column: usize, row: usize) -> Option<&str> {
        let id = self.cell(column, row)?.link?;
        self.links.get(usize::from(id.0)).map(String::as_str)
    }
}
