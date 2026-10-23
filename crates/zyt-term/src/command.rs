//! What a window asks of a terminal it does not hold.
//!
//! The window draws a [`crate::Snapshot`] of the page and says what it wants
//! changed as commands. Whoever owns the terminal applies them, in order, with
//! [`crate::Terminal::apply`]. A command that asks a question is answered with
//! an [`Answer`], so the window never reads the terminal itself.

use crate::emulator::{DefaultColors, SelectionKind, SelectionStep, Terminal};
use crate::images::{ImageId, Picture};
use crate::scheme::ColorScheme;
use crate::search::{SearchDirection, SearchOptions};

/// One change a window asks of the terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Scrolls the view by lines, positive is up.
    Scroll(i32),
    /// Scrolls the view to that many lines above the newest one.
    ScrollTo(usize),
    /// Scrolls the view to the newest line.
    ScrollToBottom,
    /// Says how large one cell is in pixels.
    CellSize {
        /// Width of a cell.
        width: usize,
        /// Height of a cell.
        height: usize,
    },
    /// Says the colours the page is painted in.
    Colors(DefaultColors),
    /// Says whether the page is dark or light.
    Scheme(ColorScheme),
    /// Starts a selection on a cell of the page.
    SelectStart {
        /// What the selection takes: characters, words, lines, a block.
        kind: SelectionKind,
        /// Column of the cell.
        column: usize,
        /// Row of the cell.
        row: usize,
    },
    /// Moves the growing end of the selection to a cell of the page.
    SelectUpdate {
        /// Column of the cell.
        column: usize,
        /// Row of the cell.
        row: usize,
    },
    /// Grows the selection from its anchor to a cell of the page.
    SelectExtend {
        /// Column of the cell.
        column: usize,
        /// Row of the cell.
        row: usize,
    },
    /// Puts the place a selection would begin on a cell of the page.
    SelectAnchor {
        /// Column of the cell.
        column: usize,
        /// Row of the cell.
        row: usize,
    },
    /// Moves the selection by a key.
    SelectStep(SelectionStep),
    /// Takes the selection away.
    SelectClear,
    /// Folds a picture, or shows it again.
    FoldPicture {
        /// The picture.
        id: ImageId,
        /// True to fold it.
        folded: bool,
    },
    /// Asks for the text of the selection.
    Copy,
    /// Asks for a picture of the page, with all that is known of it.
    Picture(ImageId),
    /// Answers a clipboard request of the program: `Some` hands the text over,
    /// `None` refuses.
    AnswerClipboard(Option<String>),
    /// Drops the scrollback.
    ForgetScrollback,
    /// Drops what the terminal owes a program and has not sent yet.
    ForgetOutput,
    /// Sets the pattern of the search; an empty query takes it down. It is
    /// answered whether the pattern could be built.
    SearchSet {
        /// What to look for.
        query: String,
        /// How to read it.
        options: SearchOptions,
    },
    /// Walks to the next match, which becomes the selection. It is answered
    /// whether one was found.
    SearchStep(SearchDirection),
    /// Takes the search and its marks down.
    SearchClear,
}

/// What the terminal answers to a command that asks.
#[derive(Debug, Clone)]
pub enum Answer {
    /// The text of the selection, for [`Command::Copy`]; nothing when no
    /// selection stands.
    Copied(Option<String>),
    /// The picture, for [`Command::Picture`]; nothing when it was dropped.
    Picture(Option<std::sync::Arc<Picture>>),
    /// Whether the search went where it was asked: a pattern that could be
    /// built, for [`Command::SearchSet`]; a match, for [`Command::SearchStep`].
    Found(bool),
}

impl Terminal {
    /// Applies one command, and answers when it asks something.
    ///
    /// A command that names a cell off the page does nothing: the page moved
    /// between the frame that drew it and this call.
    pub fn apply(&mut self, command: Command) -> Option<Answer> {
        match command {
            Command::Scroll(lines) => self.scroll(lines),
            Command::ScrollTo(offset) => self.scroll_to(offset),
            Command::ScrollToBottom => self.scroll_to_bottom(),
            Command::CellSize { width, height } => self.set_cell_size(width, height),
            Command::Colors(colors) => self.set_default_colors(colors),
            Command::Scheme(scheme) => self.set_color_scheme(scheme),
            Command::SelectStart { kind, column, row } => {
                let _ = self.selection_start(kind, column, row);
            }
            Command::SelectUpdate { column, row } => {
                let _ = self.selection_update(column, row);
            }
            Command::SelectExtend { column, row } => {
                let _ = self.selection_extend(column, row);
            }
            Command::SelectAnchor { column, row } => {
                let _ = self.set_selection_anchor(column, row);
            }
            Command::SelectStep(step) => self.select_by_key(step),
            Command::SelectClear => self.selection_clear(),
            Command::FoldPicture { id, folded } => self.fold_picture(id, folded),
            Command::Copy => return Some(Answer::Copied(self.selected_text())),
            Command::Picture(id) => return Some(Answer::Picture(self.picture(id))),
            Command::AnswerClipboard(text) => self.answer_clipboard(text.as_deref()),
            Command::ForgetScrollback => self.forget_scrollback(),
            Command::ForgetOutput => self.forget_output(),
            Command::SearchSet { query, options } => {
                return Some(Answer::Found(self.search_set(&query, options).is_ok()));
            }
            Command::SearchStep(direction) => {
                return Some(Answer::Found(self.search_advance(direction)));
            }
            Command::SearchClear => self.search_clear(),
        }
        None
    }
}
