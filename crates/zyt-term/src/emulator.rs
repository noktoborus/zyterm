//! Terminal emulation driven by a byte stream.

use crate::content::{
    Cell, CellStyle, Color, CursorInfo, CursorShape, LinkId, PALETTE_COLORS, RenderableContent,
    Rgb, TerminalModes,
};
use crate::error::{Result, TermError};
use crate::event::TerminalEvent;
use crate::images::{ImageId, Picture};
use crate::osc::{MarkKind, OscReport, OscSniffer, Sniffed, SniffedReport};
use crate::search::{SearchDirection, SearchKind, SearchOptions, pattern_for};
use crate::sixel::{SixelPlaceholder, SixelReport};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, GridCell, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{
    Config, Osc52, Term, TermMode, point_to_viewport, viewport_to_point,
};
use alacritty_terminal::vte::ansi::{Color as VteColor, Handler, NamedColor, Processor};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Where a command began, written so that it survives scrolling.
///
/// `grid().cursor.point` names a line of the screen, and the screen moves: one
/// line of output pushes everything up by one, so a line recorded before the
/// command ran would name another row by the time the command is read back.
/// What ever left the screen is the scrollback, and it grows by exactly what
/// the screen moved, so the two added together stand still.
#[derive(Debug, Clone, Copy)]
struct CommandStart {
    /// Line, counted from the oldest line the scrollback ever held.
    line: i32,
    /// Column the typing began in.
    column: usize,
}

/// Answer to a clipboard request, kept until the caller decides.
type ClipboardFormatter = Arc<dyn Fn(&str) -> String + Send + Sync + 'static>;

/// Kind of a selection started by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    /// Character selection.
    Simple,
    /// Word selection.
    Semantic,
    /// Line selection.
    Lines,
    /// Rectangular selection.
    Block,
}

/// One step of a selection made with the keyboard.
///
/// A selection of the keyboard is a block, because what it walks by is cells of
/// a row and rows of the grid: the four steps are those two counts, and the two
/// ends of the terminal are where the grid itself ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStep {
    /// One row towards the oldest line.
    Up,
    /// One row towards the newest.
    Down,
    /// One cell to the left.
    Left,
    /// One cell to the right.
    Right,
    /// The first cell of the row the caret stands on.
    LineStart,
    /// The last cell of that row.
    LineEnd,
}

impl SelectionStep {
    /// The step that carries on where this one cannot, which the two sideways
    /// steps have and nothing else does.
    ///
    /// A row is a span the eye takes in at once, so a block that goes on
    /// widening while a key is held is a block somebody is watching grow. The
    /// grid is as tall as the history, and one that grew downwards when asked
    /// to go up would walk away from the rows being read. The two ends of a row
    /// are places and not directions, so they cannot be turned around at all.
    fn opposite(self) -> Option<Self> {
        match self {
            Self::Left => Some(Self::Right),
            Self::Right => Some(Self::Left),
            Self::Up | Self::Down | Self::LineStart | Self::LineEnd => None,
        }
    }
}

/// How much of the grid a selection covers.
///
/// The three numbers are counted over the text the selection would copy, so
/// what the count says and what the clipboard receives cannot disagree: a
/// trailing run of blanks the grid holds and the copy drops is not counted
/// either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionSize {
    /// Characters of the longest line of the selection.
    pub columns: usize,
    /// Lines the selection covers.
    pub lines: usize,
    /// Characters, the line breaks between the lines left out.
    pub characters: usize,
}

impl SelectionSize {
    /// Counts the selected text.
    fn of(text: &str) -> Self {
        let mut size = Self {
            columns: 0,
            lines: 0,
            characters: 0,
        };
        for line in text.lines() {
            let characters = line.chars().count();
            size.columns = size.columns.max(characters);
            size.characters += characters;
            size.lines += 1;
        }
        size
    }
}

/// What one cell of the grid costs in memory.
///
/// A row of the grid is kept whole, at the full width of the window, whatever
/// little stands in it, so this is the price of one line of the scrollback: a
/// caller that gives the grid a budget of memory counts in this rather than
/// guessing at it.
pub const GRID_CELL_BYTES: usize = std::mem::size_of::<alacritty_terminal::term::cell::Cell>();

/// What the backend answers a program asking what this terminal is.
///
/// A VT102, which is what `alacritty_terminal` says of itself.
const PRIMARY_ATTRIBUTES: &str = "\u{1b}[?6c";

/// The same, with sixel among what it can do.
///
/// `4` is the number of sixel graphics in the primary device attributes, and a
/// program that reads it there is a program that will send a picture.
const PRIMARY_ATTRIBUTES_WITH_PICTURES: &str = "\u{1b}[?6;4c";

/// The state of a mode in an answer to a question of it (`DECRPM`): set.
const MODE_SET: u8 = 1;

/// The same: reset.
const MODE_RESET: u8 = 2;

/// Pixels of one cell a picture is measured against while the caller has said
/// nothing (`Terminal::set_cell_size`).
///
/// It is a plausible cell of a plausible font, so a picture placed before the
/// renderer has spoken takes a plausible number of cells rather than one.
pub const DEFAULT_CELL: (usize, usize) = (8, 17);

/// The colours the page is actually painted in, as a program asking is told.
///
/// They are the caller's answer the way the size of a cell is: a theme, a
/// setting that may refuse a program its own colours, and the resolving of the
/// two are all on the other side of this crate, so what is kept here is the
/// three colours that came out of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultColors {
    /// Colour the text is drawn in.
    pub foreground: Rgb,
    /// Colour behind it.
    pub background: Rgb,
    /// Colour of the cursor.
    pub cursor: Rgb,
}

/// What a program may do with the clipboard through OSC 52.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardAccess {
    /// Neither storing nor reading is honoured.
    Disabled,
    /// A program may store text and may ask for it.
    CopyPaste,
    /// A program may store text but never read it.
    #[default]
    CopyOnly,
}

impl ClipboardAccess {
    fn to_osc52(self) -> Osc52 {
        match self {
            Self::Disabled => Osc52::Disabled,
            Self::CopyPaste => Osc52::CopyPaste,
            Self::CopyOnly => Osc52::OnlyCopy,
        }
    }
}

/// Settings of a terminal instance.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Number of columns.
    pub columns: usize,
    /// Number of rows.
    pub rows: usize,
    /// Number of lines kept in the scrollback buffer.
    pub scrollback: usize,
    /// What a program may do with the clipboard.
    pub clipboard: ClipboardAccess,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            columns: 80,
            rows: 24,
            scrollback: 10_000,
            clipboard: ClipboardAccess::default(),
        }
    }
}

/// Grid geometry handed to the emulation backend.
#[derive(Debug, Clone, Copy)]
struct GridSize {
    columns: usize,
    rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

#[derive(Clone, Default)]
struct EventCollector {
    events: Rc<RefCell<Vec<Event>>>,
}

impl EventListener for EventCollector {
    fn send_event(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

/// A terminal fed from any byte source.
pub struct Terminal {
    term: Term<EventCollector>,
    /// The title a program set last; an empty one is none.
    title: Option<String>,
    parser: Processor,
    collector: EventCollector,
    size: GridSize,
    output: Vec<u8>,
    pending: Vec<TerminalEvent>,
    sniffer: OscSniffer,
    reports: Vec<OscReport>,
    sixel_placeholder: Option<SixelPlaceholder>,
    images: crate::images::ImageStore,
    cell: (usize, usize),
    colors: Option<DefaultColors>,
    scheme: crate::scheme::SchemeReports,
    picture_limit: crate::sixel::PictureLimit,
    picture_weight: usize,
    pictures: crate::sixel::PictureAccess,
    /// The display mode of sixel, `CSI ? 80 h`: a picture goes to the top
    /// left corner, nothing scrolls, the cursor stays.
    sixel_display: bool,
    command_start: Option<CommandStart>,
    clipboard_request: Option<ClipboardFormatter>,
    clipboard: ClipboardAccess,
    scrollback: usize,
    search: Option<RegexSearch>,
    search_options: SearchOptions,
    search_current: Option<Match>,
    anchor: Option<Anchor>,
    /// The end of the selection that moved last.
    ///
    /// It is the caret of the keys and the end that follows the pointer while a
    /// selection is dragged: one place, whichever moved it, kept the way the
    /// anchor is — against the history of the moment — so output arriving
    /// carries it with the line it stands on. A caller draws its plate away
    /// from it, and the keys walk from it.
    caret: Option<Anchor>,
    /// Whether the keys are what put the caret where it is.
    ///
    /// A selection dragged with the pointer is not a gesture of the keyboard,
    /// so a key that would end one leaves it alone.
    by_key: bool,
    marked: Vec<u8>,
    selection_size: Option<(SelectionRange, SelectionSize)>,
    /// The count of [`Terminal::changes`].
    changes: u64,
    /// The number of [`Terminal::identity`].
    identity: u64,
}

/// A number no other terminal of this process carries.
fn next_identity() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Terminal {
    /// Creates a terminal with the given geometry.
    pub fn new(config: TerminalConfig) -> Result<Self> {
        let size = check_size(config.columns, config.rows)?;
        let collector = EventCollector::default();
        let term_config = Config {
            scrolling_history: config.scrollback,
            osc52: config.clipboard.to_osc52(),
            ..Config::default()
        };
        let term = Term::new(term_config, &size, collector.clone());
        Ok(Self {
            term,
            title: None,
            parser: Processor::new(),
            collector,
            size,
            output: Vec::new(),
            pending: Vec::new(),
            sniffer: OscSniffer::new(),
            reports: Vec::new(),
            sixel_placeholder: None,
            images: crate::images::ImageStore::default(),
            cell: DEFAULT_CELL,
            colors: None,
            scheme: crate::scheme::SchemeReports::default(),
            picture_limit: crate::sixel::PictureLimit::default(),
            picture_weight: crate::sixel::DEFAULT_PICTURE_WEIGHT,
            pictures: crate::sixel::PictureAccess::default(),
            sixel_display: false,
            command_start: None,
            clipboard_request: None,
            clipboard: config.clipboard,
            scrollback: config.scrollback,
            search: None,
            search_options: SearchOptions::default(),
            search_current: None,
            anchor: None,
            caret: None,
            by_key: false,
            marked: Vec::new(),
            selection_size: None,
            changes: 1,
            identity: next_identity(),
        })
    }

    /// Marks a gap: output skipped on the other end. The screen as it stands
    /// goes up into the scrollback, the line of the gap under it — the sign of
    /// a pause, `text`, a dashed rule ([`GapPart`](crate::GapPart)) — and the
    /// screen is left empty with the cursor home, for what comes after the gap.
    ///
    /// On the alternate screen, which has no scrollback, the screen would be
    /// lost: the caller starts that terminal over instead.
    pub fn mark_gap(&mut self, text: &str) {
        let (columns, rows) = self.size();
        let mut bytes = format!("\x1b[m\x1b[r\x1b[?6l\x1b[{rows};1H").into_bytes();
        bytes.extend(std::iter::repeat_n(b'\n', rows));
        bytes.extend_from_slice(b"\x1b[H");
        bytes.extend_from_slice(crate::gap::line(text, columns).as_bytes());
        bytes.extend_from_slice(format!("\x1b[{rows};1H\n\x1b[H").as_bytes());
        self.feed(&bytes);
    }

    /// Processes received bytes.
    ///
    /// The parser and the sniffer walk the same chunk together, not one after
    /// the other: a mark of a shell says where it stands in the output, and
    /// the grid only says where that is while the bytes before it have been
    /// drawn and the bytes after it have not. A chunk carrying no sequence the
    /// sniffer wants is still one `advance` and nothing more.
    ///
    /// The runs of NUL bytes are written into the chunk before either of them
    /// reads it, so both walk the same bytes and an offset means one thing.
    /// A chunk carrying no NUL byte is not copied at all, which is every chunk
    /// of an ordinary session.
    ///
    /// What the backend has to say is taken before every sequence the sniffer
    /// answers for, so the answers leave in the order the questions arrived: a
    /// program that asked two things in one write reads the first answer first,
    /// whichever of the two parsers it was that had it.
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let mut marked = std::mem::take(&mut self.marked);
        let bytes = match crate::null::mark_runs(bytes, &mut marked) {
            true => marked.as_slice(),
            false => bytes,
        };

        let mut sniffed = Vec::new();
        self.sniffer.feed(bytes, &mut sniffed);

        let mut start = 0;
        for SniffedReport { end, report } in sniffed {
            self.parser.advance(&mut self.term, &bytes[start..end]);
            start = end;
            self.pump_events();
            match report {
                Sniffed::Osc(report) => {
                    self.note_mark(&report);
                    self.reports.push(report);
                }
                Sniffed::Sixel { report, payload } => self.place_sixel(report, payload),
                Sniffed::Graphics(query) => self.answer_graphics(query),
                Sniffed::CellSize => self.answer_cell_size(),
                Sniffed::PrivateModes { set, modes } => self.set_private_modes(set, &modes),
                Sniffed::ColorSchemeQuery => self.answer_color_scheme(),
                Sniffed::FullReset => {
                    self.scheme.set_wanted(false);
                    self.sixel_display = false;
                }
            }
        }
        self.parser.advance(&mut self.term, &bytes[start..]);

        self.marked = marked;
        self.selection_size = None;
        self.touch();
        self.pump_events();
    }

    /// What this terminal does with the pictures a program sends.
    ///
    /// A picture already on the page stays as it was placed: it is a part of
    /// the output the way text is, and output does not change because a setting
    /// did.
    pub fn set_picture_access(&mut self, access: crate::sixel::PictureAccess) {
        self.pictures = access;
    }

    /// The size a larger picture is shrunk to, which is also the size a program
    /// asking is told.
    ///
    /// One number for both, because a program that believes the answer sends
    /// what it was told we take: an answer larger than the limit would be an
    /// invitation to send a picture that is then shrunk, its pixels sent for
    /// nothing.
    pub fn set_picture_limit(&mut self, limit: crate::sixel::PictureLimit) {
        self.picture_limit = limit;
    }

    /// The most one picture may weigh in bytes, its payload and its pixels
    /// together.
    ///
    /// A heavier one is kept not at all and its line says it was too large, so
    /// it names no picture and has no menu of one. The payload is held while it
    /// arrives, so this is also how long one may grow before the rest of it is
    /// thrown away.
    pub fn set_picture_weight(&mut self, weight: usize) {
        self.picture_weight = weight;
        self.sniffer.set_picture_weight(weight);
    }

    /// How large one cell is in pixels, which is how many cells a picture
    /// comes to.
    ///
    /// It is the caller's answer and not the crate's: a cell is as large as the
    /// font the renderer laid out and the scale the screen is drawn at, and
    /// this crate knows of neither. It is the same kind of number as the
    /// columns and rows of `resize`, and a caller that never says it leaves
    /// pictures measured against [`DEFAULT_CELL`].
    ///
    /// A picture already placed keeps the cells it was placed in: they are
    /// where it stands on the page, and the drawing of it follows the cells.
    pub fn set_cell_size(&mut self, width: usize, height: usize) {
        self.cell = (width.max(1), height.max(1));
    }

    /// How large one cell is in pixels, as the caller last said.
    ///
    /// It is read back because the cell is two numbers the caller handed over
    /// and everything about the page in pixels is worked out from them: a
    /// caller telling the far side the size of the window — the pixels of a
    /// `winsize`, which no sequence carries — is asking for its own answer
    /// back rather than keeping a second copy of it.
    pub fn cell_size(&self) -> (usize, usize) {
        self.cell
    }

    /// The colours the page is painted in, which is what a program asking for
    /// them is told (OSC 10, OSC 11, OSC 12).
    ///
    /// It is the caller's answer and not the crate's, the way the size of a
    /// cell is: the colour of the page is a theme, and a theme is the
    /// interface. What is handed over is what is painted — the caller has
    /// already decided whether a program may repaint it — so a program is told
    /// the colour it would see and no second opinion about it.
    ///
    /// A caller that never says leaves the question unanswered, which is what a
    /// program asking it reads as a terminal that does not know.
    ///
    /// A program asking for an entry of the 256 colour table (OSC 4) is not
    /// answered: those are the theme's as well, and nothing has asked for them
    /// yet.
    pub fn set_default_colors(&mut self, colors: DefaultColors) {
        self.colors = Some(colors);
    }

    /// Whether the page is dark or light, which a program that set private
    /// mode 2031 is told of when it changes.
    ///
    /// It is the caller's answer, the way the colours of the page are. The
    /// first call is no change, so it reports nothing. A caller that never
    /// calls leaves `CSI ? 996 n` unanswered.
    ///
    /// It is said after [`Terminal::set_default_colors`]: a program told of a
    /// change asks for the colours (`OSC 11`) and has to hear the new ones.
    pub fn set_color_scheme(&mut self, scheme: crate::ColorScheme) {
        if let Some(report) = self.scheme.change(scheme) {
            self.output.extend_from_slice(report);
        }
    }

    /// What a picture the terminal does not draw leaves on the page in its
    /// place.
    ///
    /// Nothing is left by default, so a caller that says nothing sees a payload
    /// drop as it always did. The words are the caller's because the crate has
    /// none of its own, and it is a call and not a template so that whatever
    /// the caller writes numbers with is what writes these.
    pub fn set_sixel_placeholder(&mut self, placeholder: Option<SixelPlaceholder>) {
        self.sixel_placeholder = placeholder;
    }

    /// Puts a picture on the page.
    ///
    /// A picture takes the cells its pixels come to, at the size a cell is
    /// now: a payload says pixels and a page is cells, and how many cells a
    /// pixel is worth is the one thing only the caller knows
    /// (`set_cell_size`). What does not fit the room left on the line is
    /// fitted to it rather than clipped, because a picture cut off at the
    /// right edge is a picture nobody can read, and the pixels are untouched —
    /// it is the rectangle of cells that is smaller, and the drawing follows
    /// the cells.
    ///
    /// The cells of that rectangle carry the line the caller writes, row by
    /// row: a picture of eight cells across is narrower than the line, and a
    /// line cut off at its width would say less than it was written to say.
    /// The drawing covers it, so it is read in two cases only — a picture
    /// nothing draws, and a selection copied off the page — and both want the
    /// same words.
    ///
    /// Every cell of the rectangle carries the number of the picture, so a
    /// pointer anywhere on it finds the picture and a renderer knows which
    /// cells are it. A picture nothing was kept for — the decoder refused it,
    /// the budget could not hold it — is the line and nothing more, and
    /// nothing at all when its payload declared no size.
    ///
    /// The cursor comes to rest where the picture began, on the last row of
    /// it: one line feed for every row but the last. A program that draws a
    /// picture and then writes beside it moves the cursor itself — it is told
    /// what to add by the size of the picture it sent — and a terminal that
    /// spent a line of its own on the picture would put that text one line
    /// further down than the program meant. What a program writes with no
    /// cursor move of its own lands on the picture and takes those cells from
    /// it.
    ///
    /// With the display mode on (`CSI ? 80 h`) the picture is placed from the
    /// top left corner of the page instead, wherever the cursor stands. Nothing
    /// scrolls: the rows past the bottom of the page are not written, so that
    /// part of the picture is cut away. The cursor is put back where it was, so
    /// what the program writes next stands where it meant it to.
    ///
    /// The characters are handed to the emulation one at a time rather than fed
    /// to the parser, for two reasons. The parser stands inside the terminator
    /// of the sequence at this point — the escape of it has been read and the
    /// backslash has not — so bytes fed here would be read as the sequence that
    /// escape began. And a line the caller wrote is text and nothing else: a
    /// control code in it must not be able to do anything.
    fn place_sixel(&mut self, report: SixelReport, payload: Option<Vec<u8>>) {
        if !self.sixel_display {
            self.place_sixel_at_cursor(report, payload);
            return;
        }
        let cursor = self.term.grid().cursor.clone();
        let corner = &mut self.term.grid_mut().cursor;
        corner.point = Point::new(Line(0), Column(0));
        corner.input_needs_wrap = false;
        self.place_sixel_at_cursor(report, payload);
        self.term.grid_mut().cursor = cursor;
    }

    /// Places a picture at the cursor: [`Terminal::place_sixel`] with the
    /// cursor already where the picture goes.
    fn place_sixel_at_cursor(&mut self, mut report: SixelReport, payload: Option<Vec<u8>>) {
        if self.sixel_line(&report).is_empty() {
            return;
        }

        let kept = match self.pictures {
            crate::sixel::PictureAccess::Refused => {
                report.refused = Some(crate::sixel::SixelRefusal::Ignored);
                None
            }
            _ => self.keep_picture(&mut report, payload),
        };
        let text = self.sixel_line(&report);
        let Some(id) = kept else {
            if report.declared.is_some() {
                self.write_marked(&text, |_| None);
            }
            return;
        };

        if self.pictures == crate::sixel::PictureAccess::Kept {
            self.write_marked(&text, |column| {
                Some(crate::images::ImagePart { id, column, row: 0 })
            });
            return;
        }

        let start = self.term.grid().cursor.point.column.0;
        let room = self.size.columns.saturating_sub(start).max(1);
        let cells = self
            .images
            .get(id)
            .map(|picture| {
                fitted_cells(picture.pixels.width, picture.pixels.height, self.cell, room)
            })
            .unwrap_or((1, 1));
        self.images.set_cells(id, cells);

        let (columns, rows) = cells;
        let rows = match self.sixel_display {
            true => rows.min(self.size.rows),
            false => rows,
        };
        for row in 0..rows {
            let written: Vec<char> = (0..columns)
                .map(|column| text.get(row * columns + column).copied().unwrap_or(' '))
                .collect();
            self.write_marked(&written, |column| {
                Some(crate::images::ImagePart { id, column, row })
            });

            self.term.carriage_return();
            if row + 1 < rows {
                self.next_row_of_picture();
            }
            if start > 0 {
                self.term.move_forward(start);
            }
        }
    }

    /// Moves the cursor to the next row of a picture: a line feed, or with the
    /// display mode on the row below, which never scrolls the page.
    fn next_row_of_picture(&mut self) {
        if !self.sixel_display {
            self.term.linefeed();
            return;
        }
        let cursor = &mut self.term.grid_mut().cursor;
        cursor.point.line += 1;
        cursor.input_needs_wrap = false;
    }

    /// Writes characters at the cursor and marks the cells they landed in.
    ///
    /// The cells are marked row by row as they are written, because a line feed
    /// may scroll the page and a place remembered before that is a place the
    /// text has moved away from.
    ///
    /// The cell written is the one the cursor stood in, which holds while the
    /// writing does not wrap: a row of a picture starts at the column the
    /// picture does and is no wider than the room left there. A mark of nothing
    /// is the line of a picture that was not kept, which wraps like any other
    /// text and is marked nowhere.
    fn write_marked(
        &mut self,
        characters: &[char],
        part: impl Fn(usize) -> Option<crate::images::ImagePart>,
    ) {
        for (column, character) in characters.iter().enumerate() {
            let at = self.term.grid().cursor.point;
            self.term.input(*character);
            let Some(part) = part(column) else {
                continue;
            };
            for marker in crate::images::mark(part) {
                self.term.grid_mut()[at].push_zerowidth(marker);
            }
        }
    }

    /// The line the caller writes for a picture, without the control codes
    /// that would let it do anything but stand there.
    fn sixel_line(&self, report: &SixelReport) -> Vec<char> {
        let Some(placeholder) = self.sixel_placeholder.as_ref() else {
            return Vec::new();
        };
        placeholder(report)
            .chars()
            .filter(|character| !character.is_control())
            .collect()
    }

    /// Reads a payload into pixels and keeps both, answering the number the
    /// cells will carry.
    ///
    /// A payload the decoder cannot read, or one the budget cannot hold, is
    /// kept not at all: the line of it is then the text it is, which is the
    /// same answer a reader gets for a picture dropped an hour ago. A picture
    /// heavier than one may be — a payload cut short on the way, or one whose
    /// pixels come to more — is marked so in the report, so its line says why.
    fn keep_picture(
        &mut self,
        report: &mut SixelReport,
        payload: Option<Vec<u8>>,
    ) -> Option<ImageId> {
        let Some(payload) = payload.filter(|payload| payload.len() <= self.picture_weight) else {
            report.refused = Some(crate::sixel::SixelRefusal::TooLarge);
            return None;
        };
        let pixels = match crate::sixel::decode(report, &payload, self.picture_limit) {
            Ok(pixels) => pixels,
            Err(error) => {
                log::debug!("a picture of {} bytes: {error}", report.bytes);
                return None;
            }
        };
        if payload.len() + pixels.rgba.len() > self.picture_weight {
            report.refused = Some(crate::sixel::SixelRefusal::TooLarge);
            return None;
        }
        self.images.keep(Picture {
            report: *report,
            payload: payload.into(),
            cells: (1, 1),
            folded: false,
            pixels: std::sync::Arc::new(pixels),
        })
    }

    /// The picture of that number, while the terminal still has it.
    ///
    /// A number read off a cell the grid kept for an hour may name a picture
    /// the budget has since dropped, so this answers nothing rather than
    /// promising that a cell and a picture live as long as each other.
    pub fn picture(&self, id: ImageId) -> Option<std::sync::Arc<Picture>> {
        self.images.get(id)
    }

    /// How many pictures are kept, how much they take together, and how much
    /// they may take, in that order.
    pub fn pictures_held(&self) -> (usize, usize, usize) {
        self.images.held()
    }

    /// Changes how much the pictures may take, dropping the oldest down to it.
    pub fn set_picture_budget(&mut self, budget: usize) {
        self.images.set_budget(budget);
    }

    /// Folds a picture into the line it leaves on the page, or unfolds it into
    /// the picture again.
    ///
    /// It is a reader's answer and not a program's: the cells, the pixels and
    /// the line are all where they were, and what changes is whether a renderer
    /// is handed the picture to draw ([`crate::SnapshotPicture::folded`]).
    pub fn fold_picture(&mut self, id: ImageId, folded: bool) {
        self.images.fold(id, folded);
        self.touch();
    }

    /// Drops every picture kept.
    ///
    /// It is for a caller that clears the page and the scrollback: the cells
    /// that named the pictures are gone, and what they named would otherwise
    /// hold its memory until the budget pushed it out.
    pub fn clear_pictures(&mut self) {
        self.images.clear();
    }

    /// Follows the marks of a shell far enough to read the command back.
    ///
    /// What was typed is not in any sequence: it is the text the shell echoed
    /// between the mark that opens the command and the mark that opens its
    /// output, so it is read out of the grid at the moment the second mark
    /// arrives. A mark that opens a prompt or ends a command drops whatever
    /// was recorded, because nothing may be read between those two.
    fn note_mark(&mut self, report: &OscReport) {
        let OscReport::Mark(kind) = report else {
            return;
        };
        match kind {
            MarkKind::CommandStart => self.command_start = Some(self.mark_here()),
            MarkKind::OutputStart => {
                let Some(start) = self.command_start.take() else {
                    return;
                };
                let Some(start) = self.mark_now(&start) else {
                    return;
                };
                let Some(end) = self.cell_before_cursor() else {
                    return;
                };
                if end < start {
                    return;
                }
                let line = plain(self.term.bounds_to_string(start, end).trim());
                if !line.is_empty() {
                    self.pending.push(TerminalEvent::Command(line));
                }
            }
            MarkKind::PromptStart | MarkKind::CommandEnd(_) => self.command_start = None,
        }
    }

    /// Where the cursor stands, written so that it survives scrolling.
    fn mark_here(&self) -> CommandStart {
        let point = self.term.grid().cursor.point;
        CommandStart {
            line: self.term.grid().history_size() as i32 + point.line.0,
            column: point.column.0,
        }
    }

    /// That place as the grid numbers it now, or nothing when it is gone.
    ///
    /// The sum of the line and the scrollback only stands still while the
    /// scrollback is still growing. Once it is full it drops its oldest line
    /// for every new one, so it stops following the screen and a place that has
    /// already left the screen can no longer be said to be anywhere. Only the
    /// echo of what was typed stands between the two marks, so a command that
    /// has left the screen by then is a command longer than the screen is tall
    /// — and one that is answered with nothing rather than with whatever
    /// happens to stand there now.
    fn mark_now(&self, start: &CommandStart) -> Option<Point> {
        let history = self.term.grid().history_size();
        let line = start.line - history as i32;
        if line < 0 && history >= self.scrollback {
            return None;
        }
        if line < -(history as i32) {
            return None;
        }
        Some(Point::new(Line(line), Column(start.column)))
    }

    /// The cell the cursor stands past, which is the last one written.
    fn cell_before_cursor(&self) -> Option<Point> {
        let point = self.term.grid().cursor.point;
        if point.column.0 > 0 {
            return Some(Point::new(point.line, Column(point.column.0 - 1)));
        }
        if point.line.0 <= -(self.term.grid().history_size() as i32) {
            return None;
        }
        Some(Point::new(
            point.line - 1,
            Column(self.size.columns.saturating_sub(1)),
        ))
    }

    /// A count of the changes of the visible state.
    ///
    /// It only grows, and nothing that reads the page resets it, so whoever
    /// keeps a copy of the page keeps the count it was taken at and compares.
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// A number no other terminal of this process carries, so a copy of a page
    /// says which terminal it was taken from.
    pub fn identity(&self) -> u64 {
        self.identity
    }

    /// Counts one change of the visible state.
    fn touch(&mut self) {
        self.changes = self.changes.wrapping_add(1);
    }

    /// Changes the grid geometry.
    ///
    /// A picture is never fitted again: the cells a narrower page cannot hold
    /// are cut off it ([`Self::crop_pictures`]) and a page made wide again gets
    /// them back ([`Self::grow_pictures`]), so what a page shows is as much of
    /// the picture as it holds and never a picture of another size. What a
    /// picture is scaled to at all is the limit of the settings, which is
    /// answered when it arrives.
    pub fn resize(&mut self, columns: usize, rows: usize) -> Result<()> {
        let size = check_size(columns, rows)?;
        if size.columns == self.size.columns && size.rows == self.size.rows {
            return Ok(());
        }
        let drawn = self.pictures == crate::sixel::PictureAccess::Drawn;
        let narrower = size.columns < self.size.columns;
        let wider = size.columns > self.size.columns;
        if drawn && narrower {
            self.crop_pictures(size.columns);
        }
        self.size = size;
        self.term.resize(size);
        if drawn && wider {
            self.grow_pictures();
        }
        self.touch();
        Ok(())
    }

    /// Empties the cells of a picture that a narrower page cannot hold.
    ///
    /// A line too wide for the page is wrapped onto the line below, which is
    /// what text wants: nothing of it is lost. A picture does not. Pixels
    /// carried onto the next line are a strip of the picture standing under the
    /// picture, and the lines the wrap adds are lines of the page nobody
    /// printed — a page of thumbnails doubles in height and the captions beside
    /// them are read between the halves.
    ///
    /// So the cells are emptied before the grid is resized, and the reflow then
    /// has nothing there to carry: an emptied tail of a line is truncated and
    /// not wrapped. Only the cells of a picture are emptied, so the text beside
    /// one wraps the way it always did.
    ///
    /// What is cut is cut for good: a picture is as wide as the page was at its
    /// narrowest. The pixels are untouched — the store has the whole of it to
    /// save or to copy — and the cells that are left draw the part of it they
    /// always drew, so what shows is the picture with its right edge taken off.
    fn crop_pictures(&mut self, width: usize) {
        for line in self.term.topmost_line().0..=self.term.bottommost_line().0 {
            for column in width..self.size.columns {
                let point = Point::new(Line(line), Column(column));
                if picture_of(&self.term.grid()[point]).is_some() {
                    self.term.grid_mut()[point] = alacritty_terminal::term::cell::Cell::default();
                }
            }
        }
    }

    /// Gives a picture back the cells a page made wide again has room for.
    ///
    /// The pixels never left — the store holds the whole of every picture — so
    /// what a cut took is the marks, and a wider page can have them back. Every
    /// row of a picture is grown from where it ends to where its rectangle ends
    /// (`Picture::cells`) and no further: a picture is not scaled to a window,
    /// and what it was placed in is what it takes.
    ///
    /// Only empty cells are written into, and the growing stops at the first
    /// cell that is not: the room to the right of a picture is the page's, and
    /// a program may have written there while the window was narrow. A row
    /// whose first cell the cut took has nothing to grow from and stays gone.
    fn grow_pictures(&mut self) {
        let columns = self.size.columns;
        for line in self.term.topmost_line().0..=self.term.bottommost_line().0 {
            let mut column = 0;
            while column < columns {
                let at = Point::new(Line(line), Column(column));
                let Some(part) = picture_of(&self.term.grid()[at]).filter(|part| part.column == 0)
                else {
                    column += 1;
                    continue;
                };

                let start = column;
                let mut last = column;
                while last + 1 < columns
                    && picture_of(&self.term.grid()[Point::new(Line(line), Column(last + 1))])
                        .is_some_and(|next| {
                            next.id == part.id
                                && next.row == part.row
                                && next.column == last + 1 - start
                        })
                {
                    last += 1;
                }

                let rectangle = self
                    .images
                    .get(part.id)
                    .map(|picture| picture.cells.0)
                    .unwrap_or(0);
                let room = columns.min(start.saturating_add(rectangle));
                let mut grown = last + 1;
                while grown < room {
                    let point = Point::new(Line(line), Column(grown));
                    if !self.term.grid()[point].is_empty() {
                        break;
                    }
                    for character in crate::images::mark(crate::images::ImagePart {
                        id: part.id,
                        column: grown - start,
                        row: part.row,
                    }) {
                        self.term.grid_mut()[point].push_zerowidth(character);
                    }
                    grown += 1;
                }
                column = grown.max(last + 1);
            }
        }
    }

    /// Current grid geometry as columns and rows.
    pub fn size(&self) -> (usize, usize) {
        (self.size.columns, self.size.rows)
    }

    /// Bytes the terminal answers with, for example replies to device queries.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Throws away the answers the terminal has not handed over yet.
    ///
    /// They are replies to what a program asked — where the cursor stands, what
    /// the terminal is — so a caller that drops them leaves that program waiting
    /// for an answer it will never get. It is for a caller giving up on
    /// everything on its way to the device, where these bytes are on their way
    /// too.
    pub fn forget_output(&mut self) {
        self.output.clear();
    }

    /// Events collected since the last call.
    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        self.pump_events();
        std::mem::take(&mut self.pending)
    }

    /// Scrolls the viewport by the given number of lines, positive is up.
    pub fn scroll(&mut self, lines: i32) {
        if lines != 0 {
            self.term.scroll_display(Scroll::Delta(lines));
            self.touch();
        }
    }

    /// Scrolls the viewport to the newest line.
    pub fn scroll_to_bottom(&mut self) {
        if self.display_offset() != 0 {
            self.term.scroll_display(Scroll::Bottom);
            self.touch();
        }
    }

    /// Lines the viewport is currently scrolled back.
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Lines available in the scrollback buffer.
    pub fn history_size(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Scrolls the viewport so that it is `offset` lines above the newest line.
    pub fn scroll_to(&mut self, offset: usize) {
        let offset = offset.min(self.history_size());
        let delta = offset as i32 - self.display_offset() as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
            self.touch();
        }
    }

    /// The state a program set that the cells do not show
    /// ([`crate::TermState`]).
    pub fn state(&self) -> crate::TermState {
        crate::TermState::of(&self.term, self.title.as_deref())
    }

    /// Active terminal modes.
    pub fn modes(&self) -> TerminalModes {
        modes_of(self.term.mode())
    }

    /// Starts a selection on the character at the given viewport position.
    ///
    /// That character is in the selection whichever way it then grows: it is
    /// what was pointed at, and half a cell is not a thing anybody aims at.
    /// [`Self::selection_update`] moves the other end and leaves this one where
    /// it is.
    ///
    /// The place is kept as the anchor, which is the end that stays: the window
    /// draws it as where the next selection would begin, and every later call
    /// grows the selection from it.
    ///
    /// A word started on a link is the link whole, from its first character to
    /// its last: the separators of a word take an address apart at its colon
    /// (`crate::link`). It is a run of characters from there on, anchored at the
    /// head of the link.
    pub fn selection_start(
        &mut self,
        kind: SelectionKind,
        column: usize,
        row: usize,
    ) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        self.by_key = false;
        if kind == SelectionKind::Semantic
            && let Some((first, last)) = crate::link::link_around(&self.term, point)
        {
            self.set_anchor(first);
            self.set_caret(last);
            self.select(SelectionType::Simple, first, last);
            return Ok(());
        }
        self.set_anchor(point);
        self.set_caret(point);
        self.select(selection_type(kind), point, point);
        Ok(())
    }

    /// Puts the anchor of a selection at the given viewport position without
    /// starting one.
    ///
    /// It is where a selection would begin, which is what a press that selects
    /// nothing says: the window draws a bar before that character.
    ///
    /// It is kept as a place in the text and not on the screen, so it walks
    /// with the line it was put on — the page scrolled back to that line shows
    /// it there, and output arriving carries it up with the text it stands in —
    /// and it is nowhere while that line is off the page.
    pub fn set_selection_anchor(&mut self, column: usize, row: usize) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        if self.anchor.map(|anchor| anchor.point) != Some(point) {
            self.set_anchor(point);
            self.touch();
        }
        Ok(())
    }

    /// Moves the far end of the selection to the given viewport position.
    ///
    /// The anchor stays where it was put and this end follows the pointer, so a
    /// selection dragged left holds the character it started on as readily as
    /// one dragged right: both ends take the whole of the character they stand
    /// on.
    ///
    /// The kind is the one the selection was started with, so a word selection
    /// dragged still walks by words. A selection whose anchor has scrolled out of
    /// the history is left as it stands: there is nothing left to hold it by.
    pub fn selection_update(&mut self, column: usize, row: usize) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        let Some(kind) = self.term.selection.as_ref().map(|selection| selection.ty) else {
            return Ok(());
        };
        let Some(anchor) = self.anchor_point() else {
            return Ok(());
        };
        self.set_caret(point);
        self.by_key = false;
        self.select(kind, anchor, point);
        Ok(())
    }

    /// Grows the selection to take in the character at the given viewport
    /// position, which is what a press with `Shift` held asks for.
    ///
    /// The anchor jumps to the end of the selection further from the press, and
    /// what follows is what follows any anchor: the selection runs from it to the
    /// press, and a drag begun with `Shift` held goes on growing from it.
    ///
    /// Which end is further is asked of the two ends and not of a point between
    /// them, and it is asked again on every press, so a press that moved one end
    /// moved what the next press is measured against. A press outside the
    /// selection adds what lies between and one inside cuts back to it: either
    /// way the far end stays, so the selection never turns over.
    ///
    /// It grows character by character whatever the selection was made by. A word
    /// or a line is what a press picked out; a press that adds to it is aimed at
    /// a character, and a selection that went on walking by words would take in
    /// what nobody pointed at.
    ///
    /// With no selection the anchor is the end that stays, which is where the
    /// last press landed. With neither there is nothing to grow, and nothing is
    /// what happens: a press that would select the whole page up to itself is a
    /// press that selects what nobody asked for.
    pub fn selection_extend(&mut self, column: usize, row: usize) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        let Some(range) = self.selection_range() else {
            let Some(anchor) = self.anchor_point() else {
                return Ok(());
            };
            self.select(SelectionType::Simple, anchor, point);
            return Ok(());
        };

        let kept = farther_end(range, point, self.size.columns);
        self.set_anchor(kept);
        self.set_caret(point);
        self.by_key = false;
        self.select(SelectionType::Simple, kept, point);
        Ok(())
    }

    /// Starts or grows a block selection with the keyboard.
    ///
    /// The first step begins at the cursor of the device, because that is where
    /// somebody looking at the output is looking, and it leaves the anchor
    /// there: every later step moves the far end and the selection runs from
    /// the cursor to it. The kind is always a block — the keys walk by cells
    /// and rows, and a run of text has no column to walk.
    ///
    /// [`SelectionStep::LineStart`] and [`SelectionStep::LineEnd`] walk to the
    /// ends of the row the caret stands on, and not to the ends of the
    /// terminal: what a step of a block is measured in is the row it is on, and
    /// a key that took in the whole scrollback would take in what nobody
    /// pointed at.
    ///
    /// A sideways step the caret cannot take — it is against the left or the
    /// right edge — moves the anchor the other way instead, so a key held down
    /// against the edge goes on widening the block. Up and down stop at the
    /// oldest line and the newest, because a block that grew the other way
    /// would walk off the rows being read.
    ///
    /// The selection stays once the keys are let go of: nothing here is undone
    /// by a release. [`Self::forget_key_selection`] is what ends the gesture.
    pub fn select_by_key(&mut self, step: SelectionStep) {
        let fresh = self.caret_point().is_none();
        let caret = match self.caret_point() {
            Some(point) => point,
            None => self.term.grid().cursor.point,
        };
        let anchor = match fresh {
            true => caret,
            false => self.anchor_point().unwrap_or(caret),
        };

        let (anchor, caret) = match self.walk(caret, Some(step)) {
            walked if walked == caret => (self.walk(anchor, step.opposite()), caret),
            walked => (anchor, walked),
        };

        self.set_anchor(anchor);
        self.set_caret(caret);
        self.by_key = true;
        self.select(SelectionType::Block, anchor, caret);
        self.reveal(caret);
        self.touch();
    }

    /// Ends a selection made with the keyboard.
    ///
    /// The caret goes and the selection with it, which is what the next arrow
    /// pressed without the modifiers asks for: that key is typing again, and
    /// the place the typing starts from is where the cursor stood before any of
    /// this. The anchor stays, because it says where the next selection would
    /// begin and letting one go does not change that.
    pub fn forget_key_selection(&mut self) {
        if !self.by_key {
            return;
        }
        self.selection_clear();
    }

    /// Whether the terminal is picking out a selection.
    ///
    /// It stands from the press or the first step that begins one until the
    /// selection is let go of, whichever way it was made, and it is what a
    /// caller reads to know that the keys and the pointer belong to the
    /// selection rather than to the device.
    pub fn selecting(&self) -> bool {
        self.by_key || self.term.selection.is_some()
    }

    /// Whether a selection made with the keyboard is standing.
    pub fn key_selecting(&self) -> bool {
        self.by_key && self.caret.is_some()
    }

    /// The end of the selection that moved last, where the page shows it.
    ///
    /// It is the pointer while a selection is dragged and the caret while it is
    /// walked with the keys, so a caller that draws beside a selection has one
    /// place to keep away from however the selection was made. It is nothing
    /// while no selection stands and while that end is off the page.
    pub fn selection_edge(&self) -> Option<(usize, usize)> {
        self.term.selection.as_ref()?;
        let point = self.caret_point()?;
        let viewport = point_to_viewport(self.term.grid().display_offset(), point)?;
        Some((viewport.column.0, viewport.line))
    }

    /// One step from a place in the text, held inside the grid.
    ///
    /// A step walks and does not wrap: a block has a left edge and a right one,
    /// and a column that ran past either would be a block of another shape than
    /// the one the keys drew.
    fn walk(&self, from: Point, step: Option<SelectionStep>) -> Point {
        let Some(step) = step else {
            return from;
        };
        let history = self.term.grid().history_size() as i32;
        let oldest = -history;
        let newest = self.size.rows as i32 - 1;
        let last_column = self.size.columns.saturating_sub(1);

        match step {
            SelectionStep::Up => {
                Point::new(Line(from.line.0.saturating_sub(1).max(oldest)), from.column)
            }
            SelectionStep::Down => Point::new(Line((from.line.0 + 1).min(newest)), from.column),
            SelectionStep::Left => Point::new(from.line, Column(from.column.0.saturating_sub(1))),
            SelectionStep::Right => {
                Point::new(from.line, Column((from.column.0 + 1).min(last_column)))
            }
            SelectionStep::LineStart => Point::new(from.line, Column(0)),
            SelectionStep::LineEnd => Point::new(from.line, Column(last_column)),
        }
    }

    /// Puts the caret at a place in the text, against the history of this
    /// moment.
    fn set_caret(&mut self, point: Point) {
        self.caret = Some(Anchor {
            point,
            history: self.term.grid().history_size(),
        });
    }

    /// Where the caret stands now, while the history still holds that line.
    fn caret_point(&self) -> Option<Point> {
        self.caret?.point(self.term.grid().history_size())
    }

    /// The selection between two places in the text, with the whole of the
    /// character at each end in it.
    ///
    /// `Selection::include_all` is what turns the sides of the two ends
    /// outwards, so neither end loses the character it stands on and the
    /// direction the selection was made in does not matter.
    fn select(&mut self, kind: SelectionType, from: Point, to: Point) {
        let mut selection = Selection::new(kind, from, Side::Left);
        selection.update(to, Side::Left);
        selection.include_all();
        self.term.selection = Some(selection);
        self.touch();
    }

    /// Puts the anchor at a place in the text, against the history of this
    /// moment.
    fn set_anchor(&mut self, point: Point) {
        self.anchor = Some(Anchor {
            point,
            history: self.term.grid().history_size(),
        });
    }

    /// Where the anchor stands now, while the history still holds that line.
    fn anchor_point(&self) -> Option<Point> {
        self.anchor?.point(self.term.grid().history_size())
    }

    /// The range the current selection covers, top left to bottom right.
    fn selection_range(&self) -> Option<SelectionRange> {
        self.term.selection.as_ref()?.to_range(&self.term)
    }

    /// Drops the current selection. The anchor stands where it stood: it says
    /// where the next one would begin, which a selection that was let go of
    /// does not change.
    pub fn selection_clear(&mut self) {
        self.by_key = false;
        if self.caret.take().is_some() {
            self.touch();
        }
        if self.term.selection.take().is_some() {
            self.touch();
        }
    }

    /// Takes the anchor away, so nothing says where a selection would begin.
    pub fn clear_selection_anchor(&mut self) {
        if self.anchor.take().is_some() {
            self.touch();
        }
    }

    /// Text of the current selection.
    ///
    /// A cell of a run of NUL bytes reads as the character it was drawn as, so
    /// what was copied out of the grid is what stood in it.
    pub fn selected_text(&self) -> Option<String> {
        self.term.selection_to_string().map(|text| plain(&text))
    }

    /// How much of the grid the current selection covers.
    ///
    /// Counting walks the selected text, which is as long as the scrollback the
    /// selection spans, so the answer is kept until the selection names another
    /// range of the grid or bytes arrive. A caller may therefore ask once a
    /// frame while a selection stands.
    pub fn selection_size(&mut self) -> Option<SelectionSize> {
        let range = self.term.selection.as_ref()?.to_range(&self.term)?;
        if let Some((counted, size)) = self.selection_size
            && counted == range
        {
            return Some(size);
        }
        let size = SelectionSize::of(&self.selected_text()?);
        self.selection_size = Some((range, size));
        Some(size)
    }

    /// Sets the pattern every later step of the search uses. An empty query
    /// takes the search down again.
    pub fn search_set(&mut self, query: &str, options: SearchOptions) -> Result<()> {
        self.search_options = options;
        self.search_current = None;
        self.touch();
        if query.is_empty() {
            self.search = None;
            self.term.selection = None;
            return Ok(());
        }
        let pattern = pattern_for(query, &options);
        match RegexSearch::new(&pattern) {
            Ok(regex) => {
                self.search = Some(regex);
                Ok(())
            }
            Err(_) => {
                self.search = None;
                self.term.selection = None;
                Err(TermError::InvalidPattern { pattern })
            }
        }
    }

    /// Walks to the next match in the given direction and brings it into view.
    /// The search wraps around at the ends of the scrollback. The match becomes
    /// the selection, so it can be copied like any other.
    pub fn search_advance(&mut self, direction: SearchDirection) -> bool {
        if self.search.is_none() {
            return false;
        }
        let backwards = direction == SearchDirection::Up;
        let way = if backwards {
            Direction::Left
        } else {
            Direction::Right
        };
        let limit = Some(self.scrollback + self.size.rows);
        let whole_word = self.search_options.kind == SearchKind::Word;
        let mut origin = self.search_origin(backwards);
        let mut wrapped = false;
        let mut first = None;
        let found = loop {
            let candidate = {
                let Some(regex) = self.search.as_mut() else {
                    return false;
                };
                self.term.search_next(regex, origin, way, Side::Left, limit)
            };
            let Some(candidate) = candidate else {
                if wrapped {
                    break None;
                }
                wrapped = true;
                origin = grid_edge(&self.term, backwards);
                continue;
            };
            if first == Some(*candidate.start()) {
                break None;
            }
            if first.is_none() {
                first = Some(*candidate.start());
            }
            if !whole_word || whole_word_at(&self.term, &candidate) {
                break Some(candidate);
            }
            let next = step_past(&self.term, &candidate, backwards);
            if next == origin {
                break None;
            }
            origin = next;
        };
        let Some(found) = found else {
            return false;
        };
        let start = *found.start();
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(*found.end(), Side::Right);
        self.term.selection = Some(selection);
        self.search_current = Some(found);
        self.reveal(start);
        self.touch();
        true
    }

    /// Drops the search, its current match and the selection that showed it.
    pub fn search_clear(&mut self) {
        self.search = None;
        self.search_current = None;
        self.search_options = SearchOptions::default();
        self.term.selection = None;
        self.touch();
    }

    /// Where the next step of the search starts: one cell past the current
    /// match, or the edge of the viewport the search runs towards.
    fn search_origin(&self, backwards: bool) -> Point {
        let display_offset = self.term.grid().display_offset();
        match &self.search_current {
            Some(found) => step_past(&self.term, found, backwards),
            None if backwards => viewport_to_point(
                display_offset,
                Point::new(self.size.rows - 1, Column(self.size.columns - 1)),
            ),
            None => viewport_to_point(display_offset, Point::new(0, Column(0))),
        }
    }

    /// Scrolls the viewport until the given position is on the screen.
    fn reveal(&mut self, point: Point) {
        let rows = self.size.rows as i32;
        let row = point.line.0 + self.display_offset() as i32;
        let wanted = if row < 0 {
            -point.line.0
        } else if row >= rows {
            rows - 1 - point.line.0
        } else {
            return;
        };
        self.scroll_to(wanted.max(0) as usize);
    }

    /// Marks every match of the visible rows, when the caller asked for it.
    fn mark_matches(&mut self, out: &mut RenderableContent, display_offset: usize) {
        if !self.search_options.highlight_all || out.rows == 0 || out.columns == 0 {
            return;
        }
        let columns = out.columns;
        let rows = out.rows;
        let start = viewport_to_point(display_offset, Point::new(0, Column(0)));
        let end = viewport_to_point(display_offset, Point::new(rows - 1, Column(columns - 1)));
        let Some(regex) = self.search.as_mut() else {
            return;
        };
        let whole_word = self.search_options.kind == SearchKind::Word;
        let found_all: Vec<Match> =
            RegexIter::new(start, end, Direction::Right, &self.term, regex).collect();
        for found in found_all {
            if whole_word && !whole_word_at(&self.term, &found) {
                continue;
            }
            for line in found.start().line.0..=found.end().line.0 {
                let Some(row) = line.checked_add(display_offset as i32) else {
                    continue;
                };
                if row < 0 || row as usize >= rows {
                    continue;
                }
                let first = if line == found.start().line.0 {
                    found.start().column.0
                } else {
                    0
                };
                let last = if line == found.end().line.0 {
                    found.end().column.0
                } else {
                    columns - 1
                };
                for column in first..=last.min(columns - 1) {
                    out.cells[row as usize * columns + column].matched = true;
                }
            }
        }
    }

    /// Writes the current state into `out`, reusing its buffers.
    pub fn render_into(&mut self, out: &mut RenderableContent) {
        let content = self.term.renderable_content();
        let columns = self.size.columns;
        let rows = self.size.rows;
        let display_offset = content.display_offset;
        let selection = content.selection;
        let mode = content.mode;

        out.columns = columns;
        out.rows = rows;
        out.display_offset = display_offset;
        out.modes = modes_of(&mode);
        out.cells.clear();
        out.cells.resize(columns * rows, Cell::default());
        out.links.clear();
        out.pictures.clear();

        for indexed in content.display_iter {
            let point = indexed.point;
            let Some(row) = point.line.0.checked_add(display_offset as i32) else {
                continue;
            };
            if row < 0 {
                continue;
            }
            let row = row as usize;
            let column = point.column.0;
            if row >= rows || column >= columns {
                continue;
            }
            let selected = selection
                .map(|range| range.contains(point))
                .unwrap_or(false);
            let link = link_id(&mut out.links, indexed.cell);
            let part = picture_of(indexed.cell);
            if let Some(part) = part
                && self.pictures == crate::sixel::PictureAccess::Drawn
                && !out.pictures.iter().any(|known| known.id == part.id)
                && let Some(picture) = self.images.get(part.id)
            {
                out.pictures.push(crate::content::SnapshotPicture {
                    id: part.id,
                    cells: picture.cells,
                    folded: picture.folded,
                    pixels: picture.pixels.clone(),
                });
            }
            out.cells[row * columns + column] = convert_cell(indexed.cell, selected, link, part);
        }

        out.background = named_color(content.colors, NamedColor::Background);
        out.foreground = named_color(content.colors, NamedColor::Foreground);
        out.cursor_color = named_color(content.colors, NamedColor::Cursor);
        fill_palette(content.colors, &mut out.palette);
        out.history_size = self.term.grid().history_size();
        out.cursor = cursor_of(&content.cursor, display_offset, columns, rows, &mode);
        out.selection_anchor = self
            .anchor
            .and_then(|anchor| anchor.on_page(self.term.grid().history_size(), display_offset))
            .filter(|point| point.line < rows && point.column.0 < columns)
            .map(|point| (point.column.0, point.line));
        out.selection_edge = self
            .caret
            .filter(|_| self.term.selection.is_some())
            .and_then(|caret| caret.on_page(self.term.grid().history_size(), display_offset))
            .filter(|point| point.line < rows && point.column.0 < columns)
            .map(|point| (point.column.0, point.line));
        out.selecting = self.selecting();
        out.selection_size = self.selection_size();
        self.mark_matches(out, display_offset);
    }

    /// Fresh snapshot of the current state.
    pub fn content(&mut self) -> RenderableContent {
        let mut content = RenderableContent::default();
        self.render_into(&mut content);
        content
    }

    /// Answers what a program asked about the pictures this terminal takes.
    ///
    /// A terminal that takes none says nothing, the way it says nothing of
    /// sixel when it is asked what it is: a program that reads no answer reads
    /// it as no support, which is what refusing pictures is.
    fn answer_graphics(&mut self, query: crate::sixel::GraphicsQuery) {
        if self.pictures == crate::sixel::PictureAccess::Refused {
            return;
        }
        if let Some(answer) = crate::sixel::graphics_answer(query, self.picture_limit) {
            self.output.extend_from_slice(answer.as_bytes());
        }
    }

    /// The colour a program asking by that index is told, while the caller has
    /// said what the page is painted in.
    ///
    /// The three colours of the page are the ones a program asks for by name,
    /// and the entries of the colour table are left unanswered: what is painted
    /// is the caller's and the table is not kept here.
    fn color_asked_for(&self, index: usize) -> Option<Rgb> {
        let colors = self.colors?;
        match index {
            index if index == NamedColor::Foreground as usize => Some(colors.foreground),
            index if index == NamedColor::Background as usize => Some(colors.background),
            index if index == NamedColor::Cursor as usize => Some(colors.cursor),
            _ => None,
        }
    }

    /// Answers `CSI ? 996 n` with the scheme the caller said.
    fn answer_color_scheme(&mut self) {
        if let Some(answer) = self.scheme.answer() {
            self.output.extend_from_slice(answer);
        }
    }

    /// Answers `CSI 16 t` with the size of one cell.
    ///
    /// It is the same two numbers `CSI 14 t` is answered with, divided by the
    /// cells of the page: a program that asks this one is a program that would
    /// rather not do that division itself, and one that is told nothing cannot.
    fn answer_cell_size(&mut self) {
        let size = self.window_size();
        let answer = format!("\u{1b}[6;{};{}t", size.cell_height, size.cell_width);
        self.output.extend_from_slice(answer.as_bytes());
    }

    /// The page as a program asking for it in pixels is told: the cells of the
    /// grid and how large one of them is.
    ///
    /// The backend asks for this to answer `CSI 14 t`, and it is this crate's
    /// to answer because the cell is: the renderer says how large one is
    /// (`set_cell_size`) and the grid says how many there are. A program that
    /// draws pictures asks before it draws one — how many pixels a cell is
    /// worth is how it decides what to send — so a terminal that leaves the
    /// question unanswered is a terminal it sends no pictures to.
    fn window_size(&self) -> alacritty_terminal::event::WindowSize {
        alacritty_terminal::event::WindowSize {
            num_lines: u16::try_from(self.size.rows).unwrap_or(u16::MAX),
            num_cols: u16::try_from(self.size.columns).unwrap_or(u16::MAX),
            cell_width: u16::try_from(self.cell.0).unwrap_or(u16::MAX),
            cell_height: u16::try_from(self.cell.1).unwrap_or(u16::MAX),
        }
    }

    /// What this terminal answers where the backend answers for it.
    ///
    /// The backend says what it is and knows nothing of pictures, so the
    /// primary device attributes are written again here with `4` among them —
    /// the number that says sixel — while pictures are taken at all. A program
    /// that asks what a terminal is and is not told would never send one.
    ///
    /// It is answered here and not where the sequence arrives because the
    /// answer is the backend's: this is the one place it passes through, so
    /// there is one place where what we are is said.
    ///
    /// The backend does not know the private modes read beside it either, and
    /// answers a question of one (`CSI ? {mode} $ p`) with "not known". It is
    /// answered with the mode as it stands, set or reset: 2031 always, the
    /// display mode of sixel while pictures are taken at all.
    fn answer_of(&self, text: &str) -> String {
        let pictures = self.pictures != crate::sixel::PictureAccess::Refused;
        if text == PRIMARY_ATTRIBUTES && pictures {
            return PRIMARY_ATTRIBUTES_WITH_PICTURES.to_string();
        }
        let Some(mode) = unknown_mode(text) else {
            return text.to_string();
        };
        let set = match mode {
            crate::scheme::REPORTS_MODE => self.scheme.wanted(),
            crate::sixel::DISPLAY_MODE if pictures => self.sixel_display,
            _ => return text.to_string(),
        };
        let state = match set {
            true => MODE_SET,
            false => MODE_RESET,
        };
        format!("\u{1b}[?{mode};{state}$y")
    }

    /// Sets or resets the private modes the backend does not read.
    fn set_private_modes(&mut self, set: bool, modes: &[u16]) {
        for mode in modes {
            match *mode {
                crate::scheme::REPORTS_MODE => self.scheme.set_wanted(set),
                crate::sixel::DISPLAY_MODE => self.sixel_display = set,
                _ => {}
            }
        }
    }

    fn pump_events(&mut self) {
        let raw: Vec<Event> = self.collector.events.borrow_mut().drain(..).collect();
        for event in raw {
            match event {
                Event::Title(title) => {
                    self.title = Some(title.clone()).filter(|title| !title.is_empty());
                    self.pending.push(TerminalEvent::Title(title));
                }
                Event::ResetTitle => {
                    self.title = None;
                    self.pending.push(TerminalEvent::ResetTitle);
                }
                Event::Bell => self.pending.push(TerminalEvent::Bell),
                Event::ClipboardStore(_, text) => {
                    self.pending.push(TerminalEvent::ClipboardStore(text))
                }
                Event::Exit | Event::ChildExit(_) => self.pending.push(TerminalEvent::Exit),
                Event::PtyWrite(text) => {
                    let answer = self.answer_of(&text);
                    self.output.extend_from_slice(answer.as_bytes());
                }
                Event::ColorRequest(index, formatter) => {
                    if let Some(color) = self.color_asked_for(index) {
                        let answer = formatter(alacritty_terminal::vte::ansi::Rgb {
                            r: color.r,
                            g: color.g,
                            b: color.b,
                        });
                        self.output.extend_from_slice(answer.as_bytes());
                    }
                }
                Event::TextAreaSizeRequest(formatter) => {
                    let answer = formatter(self.window_size());
                    self.output.extend_from_slice(answer.as_bytes());
                }
                Event::ClipboardLoad(_, formatter) => {
                    self.clipboard_request = Some(formatter);
                    self.pending.push(TerminalEvent::ClipboardRequest);
                }
                _ => {}
            }
        }

        for report in self.reports.drain(..) {
            self.pending.push(match report {
                OscReport::WorkingDirectory(path) => TerminalEvent::WorkingDirectory(path),
                OscReport::Notification { kind, title, body } => {
                    TerminalEvent::Notification { kind, title, body }
                }
                OscReport::Mark(kind) => TerminalEvent::Mark(kind),
                OscReport::Progress(state) => TerminalEvent::Progress(state),
            });
        }
    }

    /// Answers a clipboard request of the program.
    ///
    /// `Some` hands the text over, `None` refuses; a refusal answers nothing at
    /// all, which is what a program sees when the terminal does not support the
    /// request.
    pub fn answer_clipboard(&mut self, text: Option<&str>) {
        let Some(formatter) = self.clipboard_request.take() else {
            return;
        };
        if let Some(text) = text {
            self.output.extend_from_slice(formatter(text).as_bytes());
        }
    }

    /// Changes what a program may do with the clipboard.
    pub fn set_clipboard_access(&mut self, access: ClipboardAccess) {
        if self.clipboard == access {
            return;
        }
        self.clipboard = access;
        self.apply_options(self.scrollback);
    }

    /// Lines the scrollback may keep.
    pub fn scrollback(&self) -> usize {
        self.scrollback
    }

    /// Changes how many lines the scrollback may keep.
    ///
    /// What stands above the new cap is dropped at once and the memory of it is
    /// given back, so a grid told to keep less keeps less from that moment on.
    /// The cap is what the grid costs: a row is stored at its full width whatever
    /// little stands in it, so the price of a line is the width of the window.
    pub fn set_scrollback(&mut self, lines: usize) {
        if self.scrollback == lines {
            return;
        }
        self.scrollback = lines;
        self.apply_options(lines);
    }

    /// Drops what the scrollback holds, keeping the screen and the cap.
    ///
    /// The lines above the screen are gone and their memory with them, and the
    /// grid may fill again up to the same cap: it is the scrollback of a session
    /// that is over, not a smaller scrollback.
    pub fn forget_scrollback(&mut self) {
        if self.history_size() == 0 {
            return;
        }
        self.apply_options(0);
        self.apply_options(self.scrollback);
    }

    /// Hands the options the caller decides down to the emulation.
    ///
    /// Everything of them is said at once, because `set_options` takes the whole
    /// of it: what is not named here is the default of the backend, which is
    /// what this crate leaves it at.
    fn apply_options(&mut self, scrolling_history: usize) {
        self.term.set_options(Config {
            scrolling_history,
            osc52: self.clipboard.to_osc52(),
            ..Config::default()
        });
    }

    /// True while a clipboard request waits for an answer.
    pub fn clipboard_requested(&self) -> bool {
        self.clipboard_request.is_some()
    }

    fn viewport_point(&self, column: usize, row: usize) -> Result<Point> {
        if column >= self.size.columns || row >= self.size.rows {
            return Err(TermError::OutsideGrid { column, row });
        }
        let display_offset = self.term.grid().display_offset();
        Ok(viewport_to_point(
            display_offset,
            Point::new(row, Column(column)),
        ))
    }
}

/// The end of a range further from a place in the text.
///
/// It is where the anchor jumps on a press with `Shift` held, so the selection
/// then runs from it to the press: a press outside the range keeps the end away
/// from it and takes in everything between, and one inside keeps the far end and
/// brings the near one in. Neither turns the selection over.
///
/// The two ends are what the press is measured against, and not a point between
/// them: a middle is a place the selection does not have, and one counted in whole
/// characters answers the cell it falls in for one end and the other.
///
/// Distance is counted over the text and not across the screen — the characters
/// of every line between, and not the space between two corners — so an end a
/// line away is further than one a few columns away. Equally far, the start is the
/// one that stays; a press exactly between the two ends has no far end to name.
fn farther_end(range: SelectionRange, point: Point, columns: usize) -> Point {
    let columns = columns.max(1) as i64;
    let index = |place: Point| place.line.0 as i64 * columns + place.column.0 as i64;
    let (here, start, end) = (index(point), index(range.start), index(range.end));

    match (here - start).abs() >= (here - end).abs() {
        true => range.start,
        false => range.end,
    }
}

/// Where a selection would begin, as a place in the text.
///
/// A point of the grid names a line of the screen and not a line of the
/// output: the lines move under those numbers as the text scrolls, so the
/// number the anchor was put at is worth what it was only while the screen has
/// not moved since. How far it has moved is how much the history has grown,
/// which is what is kept beside the point and taken off it when it is asked
/// for.
///
/// A history that is full drops its oldest lines instead of growing, so a
/// terminal that has scrolled a whole scrollback since the anchor was put down
/// stops taking the difference off it. What that costs is a bar drawn a line
/// out of place on a page nobody has looked at for ten thousand lines of
/// output, and what it saves is a line of the grid counted forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anchor {
    /// The place, as the grid numbered it when it was put down.
    point: Point,
    /// Lines the history held then.
    history: usize,
}

impl Anchor {
    /// Where it stands in the grid now, when the grid still holds that line.
    fn point(self, history: usize) -> Option<Point> {
        let moved = history.saturating_sub(self.history);
        let line = self.point.line.0 - moved as i32;
        if line < -(history as i32) {
            return None;
        }

        Some(Point::new(Line(line), self.point.column))
    }

    /// Where it stands on the page now, when it stands on it at all.
    fn on_page(self, history: usize, display_offset: usize) -> Option<Point<usize>> {
        point_to_viewport(display_offset, self.point(history)?)
    }
}

/// One cell past a match, on the side the search runs towards.
fn step_past<T>(term: &Term<T>, found: &Match, backwards: bool) -> Point {
    if backwards {
        found.start().sub(term, Boundary::Grid, 1)
    } else {
        found.end().add(term, Boundary::Grid, 1)
    }
}

/// Whether a match stands on its own instead of inside a longer word. The
/// cells beside it answer it, so every alphabet is treated alike.
fn whole_word_at<T>(term: &Term<T>, found: &Match) -> bool {
    let start = *found.start();
    let before = start.sub(term, Boundary::Grid, 1);
    if before != start && is_word_character(term.grid()[before].c) {
        return false;
    }
    let end = *found.end();
    let after = end.add(term, Boundary::Grid, 1);
    if after != end && is_word_character(term.grid()[after].c) {
        return false;
    }
    true
}

/// Whether a character continues a word.
fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The far end of the grid, where a wrapped search starts again.
fn grid_edge<T>(term: &Term<T>, backwards: bool) -> Point {
    if backwards {
        Point::new(term.bottommost_line(), term.last_column())
    } else {
        Point::new(term.topmost_line(), Column(0))
    }
}

fn check_size(columns: usize, rows: usize) -> Result<GridSize> {
    if columns < 2 || rows < 1 {
        return Err(TermError::InvalidSize { columns, rows });
    }
    Ok(GridSize { columns, rows })
}

fn selection_type(kind: SelectionKind) -> SelectionType {
    match kind {
        SelectionKind::Simple => SelectionType::Simple,
        SelectionKind::Semantic => SelectionType::Semantic,
        SelectionKind::Lines => SelectionType::Lines,
        SelectionKind::Block => SelectionType::Block,
    }
}

fn modes_of(mode: &TermMode) -> TerminalModes {
    TerminalModes {
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        app_keypad: mode.contains(TermMode::APP_KEYPAD),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        alt_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse_report: mode.intersects(TermMode::MOUSE_MODE),
        sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
        mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
        mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
    }
}

fn cursor_of(
    cursor: &alacritty_terminal::term::RenderableCursor,
    display_offset: usize,
    columns: usize,
    rows: usize,
    mode: &TermMode,
) -> Option<CursorInfo> {
    if !mode.contains(TermMode::SHOW_CURSOR) {
        return None;
    }
    let row = cursor.point.line.0.checked_add(display_offset as i32)?;
    if row < 0 {
        return None;
    }
    let row = row as usize;
    let column = cursor.point.column.0;
    if row >= rows || column >= columns {
        return None;
    }
    let shape = match cursor.shape {
        alacritty_terminal::vte::ansi::CursorShape::Block => CursorShape::Block,
        alacritty_terminal::vte::ansi::CursorShape::Underline => CursorShape::Underline,
        alacritty_terminal::vte::ansi::CursorShape::Beam => CursorShape::Beam,
        alacritty_terminal::vte::ansi::CursorShape::HollowBlock => CursorShape::Hollow,
        alacritty_terminal::vte::ansi::CursorShape::Hidden => CursorShape::Hidden,
    };
    Some(CursorInfo { column, row, shape })
}

/// Places the hyperlink of a cell in the table of the snapshot, so the same
/// address is stored per cell instead of the whole URI.
fn link_id(links: &mut Vec<String>, cell: &alacritty_terminal::term::cell::Cell) -> Option<LinkId> {
    let hyperlink = cell.hyperlink()?;
    let uri = hyperlink.uri();
    if let Some(index) = links.iter().position(|known| known == uri) {
        return u16::try_from(index).ok().map(LinkId);
    }
    let index = u16::try_from(links.len()).ok()?;
    links.push(uri.to_string());
    Some(LinkId(index))
}

/// One text of the grid with every cell of a run of NUL bytes read as the
/// character it was drawn as.
///
/// Nothing is allocated for a text with no such cell in it, which is nearly
/// every text: the cells carry noncharacters, so one of them in the text is what
/// says there is anything to replace.
///
/// The mark that says a cell belongs to a picture is taken out altogether. It
/// is a zerowidth character of the cell, so the text carries it the way it
/// carries a combining character, and it stands for nothing a reader wants
/// pasted anywhere.
fn plain(text: &str) -> String {
    if !text.chars().any(|ch| {
        crate::null::null_part(ch).is_some()
            || crate::gap::gap_part(ch).is_some()
            || crate::images::is_mark(ch)
    }) {
        return text.to_string();
    }
    text.chars()
        .filter(|ch| !crate::images::is_mark(*ch))
        .filter_map(|ch| match crate::gap::gap_text(ch) {
            Some(read) => read,
            None => Some(crate::null::null_text(ch).unwrap_or(ch)),
        })
        .collect()
}

/// How many cells a picture of this many pixels takes, at this cell size, with
/// this much room left on the line.
///
/// A picture takes whole cells, so the count is rounded up and the last row and
/// column of it are part empty. One too wide for the room is fitted to that
/// room — the pixels are untouched and the rectangle is smaller, so the drawing
/// of it is smaller too — because a picture placed cut off at the right edge is
/// a picture nobody can read. A picture is never wider than its own pixels come
/// to, never narrower than one cell and never shorter than one row: something
/// arrived, and a rectangle of nothing would say it did not.
fn fitted_cells(width: usize, height: usize, cell: (usize, usize), room: usize) -> (usize, usize) {
    let (cell_width, cell_height) = (cell.0.max(1), cell.1.max(1));
    let columns = width.div_ceil(cell_width).clamp(1, room.max(1));
    let shown = (columns * cell_width).min(width).max(1);
    let rows = (height * shown)
        .div_ceil(width.max(1))
        .div_ceil(cell_height)
        .max(1);
    (columns, rows)
}

/// The private mode a backend answer says it does not know: `CSI ? {mode} ; 0
/// $ y`.
fn unknown_mode(text: &str) -> Option<u16> {
    text.strip_prefix("\u{1b}[?")?
        .strip_suffix(";0$y")?
        .parse()
        .ok()
}

/// The picture a cell belongs to and the part of it the cell shows, read off
/// the zerowidth characters where the mark of one is kept.
fn picture_of(cell: &alacritty_terminal::term::cell::Cell) -> Option<crate::images::ImagePart> {
    crate::images::part_of(cell.zerowidth()?)
}

fn convert_cell(
    cell: &alacritty_terminal::term::cell::Cell,
    selected: bool,
    link: Option<LinkId>,
    image: Option<crate::images::ImagePart>,
) -> Cell {
    let flags = cell.flags;
    Cell {
        ch: cell.c,
        fg: convert_color(cell.fg),
        bg: convert_color(cell.bg),
        style: CellStyle {
            bold: flags.contains(Flags::BOLD),
            dim: flags.contains(Flags::DIM),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            strikeout: flags.contains(Flags::STRIKEOUT),
            inverse: flags.contains(Flags::INVERSE),
            hidden: flags.contains(Flags::HIDDEN),
            wide: flags.contains(Flags::WIDE_CHAR),
            wide_spacer: flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
        },
        selected,
        matched: false,
        link,
        image,
    }
}

/// The colors of the 256 color table a program painted over.
///
/// The table is left empty while the program has painted over none of it, which
/// is what it stands at until one does: 256 entries of nothing are 256 entries
/// a renderer would ask about for every cell it draws.
fn fill_palette(colors: &alacritty_terminal::term::color::Colors, out: &mut Vec<Option<Rgb>>) {
    out.clear();
    if !(0..PALETTE_COLORS).any(|index| colors[index].is_some()) {
        return;
    }

    out.extend((0..PALETTE_COLORS).map(|index| {
        colors[index].map(|color| Rgb {
            r: color.r,
            g: color.g,
            b: color.b,
        })
    }));
}

/// One of the colors a program set with an operating system command.
fn named_color(colors: &alacritty_terminal::term::color::Colors, name: NamedColor) -> Option<Rgb> {
    colors[name].map(|color| Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    })
}

fn convert_color(color: VteColor) -> Color {
    match color {
        VteColor::Spec(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
        VteColor::Indexed(index) => {
            if index < 16 {
                Color::Palette(index)
            } else {
                Color::Indexed(index)
            }
        }
        VteColor::Named(named) => match named {
            NamedColor::Foreground => Color::Foreground,
            NamedColor::Background => Color::Background,
            NamedColor::Cursor => Color::Cursor,
            NamedColor::BrightForeground => Color::BrightForeground,
            NamedColor::DimForeground => Color::DimForeground,
            other => Color::Palette(palette_index(other)),
        },
    }
}

fn palette_index(named: NamedColor) -> u8 {
    match named {
        NamedColor::Black | NamedColor::DimBlack => 0,
        NamedColor::Red | NamedColor::DimRed => 1,
        NamedColor::Green | NamedColor::DimGreen => 2,
        NamedColor::Yellow | NamedColor::DimYellow => 3,
        NamedColor::Blue | NamedColor::DimBlue => 4,
        NamedColor::Magenta | NamedColor::DimMagenta => 5,
        NamedColor::Cyan | NamedColor::DimCyan => 6,
        NamedColor::White | NamedColor::DimWhite => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => 7,
    }
}
