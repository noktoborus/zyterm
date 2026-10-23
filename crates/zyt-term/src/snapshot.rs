//! The page a window draws, taken from the terminal when it changed.

use crate::content::{RenderableContent, SnapshotPicture};
use crate::emulator::Terminal;

/// The page of a terminal, kept for the window, with the number of its
/// revision.
///
/// A terminal that was touched is not a terminal that looks different: a key
/// that was pressed, a selection dragged over the same cells, a row repainted
/// with what already stood there all mark the terminal changed while the
/// page is the same. The revision moves only when the page differs, so a
/// renderer that keeps a picture of the page builds it again only then.
///
/// Two buffers swap, so a page is never copied.
#[derive(Debug, Default)]
pub struct Snapshot {
    page: RenderableContent,
    spare: RenderableContent,
    revision: u64,
    /// The terminal the page was taken from, and its count of changes then.
    taken: Option<(u64, u64)>,
}

impl Snapshot {
    /// An empty snapshot, taken from nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the page again when the terminal changed since it was last taken,
    /// when it is another terminal, or when its size is not the size of the
    /// page.
    ///
    /// The change is told by [`Terminal::changes`], which nobody else resets: a
    /// page read elsewhere in between is not a page missed here.
    pub fn take(&mut self, terminal: &mut Terminal) {
        let size = terminal.size();
        let taken = (terminal.identity(), terminal.changes());
        if self.taken == Some(taken) && size == (self.page.columns, self.page.rows) {
            return;
        }
        terminal.render_into(&mut self.spare);
        if self.taken.is_none() || !same_page(&self.spare, &self.page) {
            self.revision = self.revision.wrapping_add(1);
        }
        self.taken = Some(taken);
        std::mem::swap(&mut self.spare, &mut self.page);
    }

    /// The page.
    pub fn page(&self) -> &RenderableContent {
        &self.page
    }

    /// The page and its revision, for a snapshot handed over from where it was
    /// taken: the caller puts both in, and the next [`Self::take`] takes the
    /// page again whatever it holds.
    pub fn parts_mut(&mut self) -> (&mut RenderableContent, &mut u64) {
        self.taken = None;
        (&mut self.page, &mut self.revision)
    }

    /// How many pages other than the one before them were taken.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Whether two pages would be painted into the same picture.
///
/// The cells of the page and the colours they are resolved against are the
/// whole of it. What a page carries beside them — the cursor, the offset into
/// the history, how much history there is, the modes and the addresses of the
/// links — is drawn beside the picture or not drawn at all, and a page whose
/// cells agree carries the same link in the same cell either way.
fn same_page(left: &RenderableContent, right: &RenderableContent) -> bool {
    left.columns == right.columns
        && left.rows == right.rows
        && left.background == right.background
        && left.foreground == right.foreground
        && left.cells == right.cells
        && same_pictures(&left.pictures, &right.pictures)
}

/// Whether two pages carry the same pictures, in the same place and the same
/// way.
///
/// The pixels are not compared: a picture is kept by its number and the number
/// is in the cells. What is compared is what the text is laid out against —
/// which cells a picture stands in, and whether it is drawn at all.
fn same_pictures(left: &[SnapshotPicture], right: &[SnapshotPicture]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.id == right.id && left.cells == right.cells && left.folded == right.folded
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminalConfig;

    fn terminal_showing(text: &str) -> Terminal {
        let mut terminal = Terminal::new(TerminalConfig {
            columns: 10,
            rows: 2,
            ..TerminalConfig::default()
        })
        .expect("the terminal is created");
        terminal.feed(text.as_bytes());
        terminal
    }

    #[test]
    fn another_page_is_another_revision() {
        let mut terminal = terminal_showing("ab");
        let mut snapshot = Snapshot::new();
        snapshot.take(&mut terminal);
        let first = snapshot.revision();

        terminal.feed(b"\r\nc");
        snapshot.take(&mut terminal);

        assert_ne!(snapshot.revision(), first);
        assert_eq!(snapshot.page().cell(0, 1).map(|cell| cell.ch), Some('c'));
    }

    #[test]
    fn the_same_page_written_again_is_the_same_revision() {
        let mut terminal = terminal_showing("ab");
        let mut snapshot = Snapshot::new();
        snapshot.take(&mut terminal);
        let first = snapshot.revision();

        terminal.feed(b"\rab");
        snapshot.take(&mut terminal);

        assert_eq!(snapshot.revision(), first);
    }

    #[test]
    fn a_cursor_that_moved_alone_is_the_same_revision() {
        let mut terminal = terminal_showing("ab");
        let mut snapshot = Snapshot::new();
        snapshot.take(&mut terminal);
        let (first, cursor) = (snapshot.revision(), snapshot.page().cursor);

        terminal.feed(b"\r");
        snapshot.take(&mut terminal);

        assert_ne!(snapshot.page().cursor, cursor);
        assert_eq!(snapshot.revision(), first);
    }

    #[test]
    fn a_page_read_elsewhere_is_not_missed() {
        let mut terminal = terminal_showing("ab");
        let mut snapshot = Snapshot::new();
        snapshot.take(&mut terminal);

        terminal.feed(b" and d");
        terminal.render_into(&mut RenderableContent::default());
        snapshot.take(&mut terminal);

        assert!(snapshot.page().cells.iter().any(|cell| cell.ch == 'd'));
    }

    #[test]
    fn the_page_of_another_terminal_is_taken() {
        let mut terminal = terminal_showing("ab");
        let mut other = terminal_showing("xy");
        let mut snapshot = Snapshot::new();
        snapshot.take(&mut terminal);

        snapshot.take(&mut other);

        assert_eq!(snapshot.page().cell(0, 0).map(|cell| cell.ch), Some('x'));
    }
}
