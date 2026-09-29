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
use alacritty_terminal::selection::{Selection, SelectionType};
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
    marked: Vec<u8>,
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
            marked: Vec::new(),
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

    /// Starts a selection at the given viewport position.
    ///
    /// A selection begins before the character that was pointed at and not
    /// after it, so the first character of it is the one under the pointer:
    /// what is pointed at is what is taken. Where it ends is another question,
    /// answered by [`Self::selection_update`], where the half of the cell the
    /// pointer stands on decides whether that last character is in or out.
    ///
    /// The place it begins at is kept as the anchor, so the window can show
    /// where the next selection would start.
    pub fn selection_start(
        &mut self,
        kind: SelectionKind,
        column: usize,
        row: usize,
    ) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        self.term.selection = Some(Selection::new(selection_type(kind), point, Side::Left));
        self.anchor = Some(Anchor {
            point,
            history: self.term.grid().history_size(),
        });
        self.dirty = true;
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
        let anchor = Anchor {
            point: self.viewport_point(column, row)?,
            history: self.term.grid().history_size(),
        };
        if self.anchor != Some(anchor) {
            self.anchor = Some(anchor);
            self.dirty = true;
        }
        Ok(())
    }

    /// Extends the running selection to the given viewport position.
    pub fn selection_update(&mut self, column: usize, row: usize, right_half: bool) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        let side = if right_half { Side::Right } else { Side::Left };
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
            self.dirty = true;
        }
        Ok(())
    }

    /// Moves the far end of the selection to the given viewport position, or
    /// starts one at the anchor and moves it there when there is none.
    ///
    /// This is what a press with `Shift` held asks for: the place a selection
    /// began stays where it is and the other end goes to what was pressed, so
    /// one press grows the selection and the next shrinks it, and a press on
    /// either side of where it began works the same way — the side is decided
    /// by where the press landed and not by which end of the selection is
    /// nearer.
    ///
    /// With no selection and no anchor there is nothing to extend, and nothing
    /// is what happens: a press that would otherwise select the whole page up
    /// to itself is a press that selects what nobody asked for.
    pub fn selection_extend(&mut self, column: usize, row: usize, right_half: bool) -> Result<()> {
        let point = self.viewport_point(column, row)?;
        let side = if right_half { Side::Right } else { Side::Left };

        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
            self.dirty = true;
            return Ok(());
        }

        let history = self.term.grid().history_size();
        let Some(anchor) = self.anchor.and_then(|anchor| anchor.point(history)) else {
            return Ok(());
        };

        let mut selection =
            Selection::new(selection_type(SelectionKind::Simple), anchor, Side::Left);
        selection.update(point, side);
        self.term.selection = Some(selection);
        self.dirty = true;
        Ok(())
    }

    /// Drops the current selection. The anchor stands where it stood: it says
    /// where the next one would begin, which a selection that was let go of
    /// does not change.
    pub fn selection_clear(&mut self) {
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
