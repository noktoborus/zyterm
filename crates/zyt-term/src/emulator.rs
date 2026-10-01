//! Terminal emulation driven by a byte stream.

use crate::content::{
    Cell, CellStyle, Color, CursorInfo, CursorShape, LinkId, PALETTE_COLORS, RenderableContent,
    Rgb, TerminalModes,
};
use crate::error::{Result, TermError};
use crate::event::TerminalEvent;
use crate::osc::{MarkKind, OscReport, OscSniffer, SniffedReport};
use crate::search::{SearchDirection, SearchKind, SearchOptions, pattern_for};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{
    Config, Osc52, Term, TermMode, point_to_viewport, viewport_to_point,
};
use alacritty_terminal::vte::ansi::{Color as VteColor, NamedColor, Processor};
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
    parser: Processor,
    collector: EventCollector,
    size: GridSize,
    output: Vec<u8>,
    pending: Vec<TerminalEvent>,
    sniffer: OscSniffer,
    reports: Vec<OscReport>,
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
    dirty: bool,
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
            parser: Processor::new(),
            collector,
            size,
            output: Vec::new(),
            pending: Vec::new(),
            sniffer: OscSniffer::new(),
            reports: Vec::new(),
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
            dirty: true,
        })
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
            self.note_mark(&report);
            self.reports.push(report);
        }
        self.parser.advance(&mut self.term, &bytes[start..]);

        self.marked = marked;
        self.selection_size = None;
        self.dirty = true;
        self.pump_events();
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

    /// True when the visible state changed since the last snapshot.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Changes the grid geometry.
    pub fn resize(&mut self, columns: usize, rows: usize) -> Result<()> {
        let size = check_size(columns, rows)?;
        if size.columns == self.size.columns && size.rows == self.size.rows {
            return Ok(());
        }
        self.size = size;
        self.term.resize(size);
        self.dirty = true;
        Ok(())
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
            self.dirty = true;
        }
    }

    /// Scrolls the viewport to the newest line.
    pub fn scroll_to_bottom(&mut self) {
        if self.display_offset() != 0 {
            self.term.scroll_display(Scroll::Bottom);
            self.dirty = true;
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
            self.dirty = true;
        }
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
    pub fn selection_start(
        &mut self,
        kind: SelectionKind,
        column: usize,
        row: usize,
    ) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        self.set_anchor(point);
        self.set_caret(point);
        self.by_key = false;
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
            self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
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
            self.dirty = true;
        }
        if self.term.selection.take().is_some() {
            self.dirty = true;
        }
    }

    /// Takes the anchor away, so nothing says where a selection would begin.
    pub fn clear_selection_anchor(&mut self) {
        if self.anchor.take().is_some() {
            self.dirty = true;
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
        self.dirty = true;
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
        self.dirty = true;
        true
    }

    /// Drops the search, its current match and the selection that showed it.
    pub fn search_clear(&mut self) {
        self.search = None;
        self.search_current = None;
        self.search_options = SearchOptions::default();
        self.term.selection = None;
        self.dirty = true;
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

    /// Writes the current state into `out`, reusing its buffers, and marks the
    /// terminal as unchanged until the next update.
    pub fn render_into(&mut self, out: &mut RenderableContent) {
        self.dirty = false;
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
            out.cells[row * columns + column] = convert_cell(indexed.cell, selected, link);
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
        self.mark_matches(out, display_offset);
    }

    /// Fresh snapshot of the current state.
    pub fn content(&mut self) -> RenderableContent {
        let mut content = RenderableContent::default();
        self.render_into(&mut content);
        content
    }

    fn pump_events(&mut self) {
        let raw: Vec<Event> = self.collector.events.borrow_mut().drain(..).collect();
        for event in raw {
            match event {
                Event::Title(title) => self.pending.push(TerminalEvent::Title(title)),
                Event::ResetTitle => self.pending.push(TerminalEvent::ResetTitle),
                Event::Bell => self.pending.push(TerminalEvent::Bell),
                Event::ClipboardStore(_, text) => {
                    self.pending.push(TerminalEvent::ClipboardStore(text))
                }
                Event::Exit | Event::ChildExit(_) => self.pending.push(TerminalEvent::Exit),
                Event::PtyWrite(text) => self.output.extend_from_slice(text.as_bytes()),
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
fn plain(text: &str) -> String {
    if !text.chars().any(|ch| crate::null::null_part(ch).is_some()) {
        return text.to_string();
    }
    text.chars()
        .map(|ch| crate::null::null_text(ch).unwrap_or(ch))
        .collect()
}

fn convert_cell(
    cell: &alacritty_terminal::term::cell::Cell,
    selected: bool,
    link: Option<LinkId>,
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
