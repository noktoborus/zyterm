//! The terminal of the screen as a test reaches it.
//!
//! The terminal lives in the thread of the screen, so a test cannot hold it.
//! This stands in for it with the calls the tests make: each one sends what
//! it asks and waits until the screen answered a question sent after it,
//! which is when everything sent before has been applied and put on the
//! board.

use super::{Purpose, Session};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_term::{Answer, Command, Picture, RenderableContent, SelectionKind, SelectionStep};

/// How long a test waits for the screen to answer.
const ANSWER_LIMIT: Duration = Duration::from_secs(5);

impl Session {
    /// The terminal of the screen, for a test.
    pub(crate) fn test_terminal(&mut self) -> TestTerminal<'_> {
        TestTerminal { session: self }
    }

    /// Asks the screen and waits for its answer, taking the board meanwhile.
    pub(crate) fn answer_now(&mut self, question: Command) -> Answer {
        self.ask(Purpose::Test, question);
        let deadline = Instant::now() + ANSWER_LIMIT;
        loop {
            let _ = self.pump();
            let replies = self.take_replies();
            let mut answer = None;
            for (purpose, reply) in replies {
                match purpose {
                    Purpose::Test => answer = Some(reply),
                    other => self.replies.push((other, reply)),
                }
            }
            if let Some(answer) = answer {
                return answer;
            }
            assert!(Instant::now() < deadline, "the screen answers");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits until the screen applied everything sent so far.
    pub(crate) fn settle(&mut self) {
        self.answer_now(Command::Copy);
    }
}

/// The terminal shown, reached through the screen.
pub(crate) struct TestTerminal<'a> {
    session: &'a mut Session,
}

impl TestTerminal<'_> {
    fn command(&mut self, command: Command) {
        self.session.command(command);
        self.session.settle();
    }

    fn page(&mut self) -> &RenderableContent {
        self.session.settle();
        self.session.content()
    }

    /// Feeds bytes to the plain terminal, as if the device said them.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.session
            .send(crate::screen::ScreenCommand::Print(bytes.to_vec()));
        self.session.settle();
    }

    /// The page as it stands.
    pub(crate) fn content(&mut self) -> RenderableContent {
        self.page().clone()
    }

    /// Whether a selection stands.
    pub(crate) fn selecting(&mut self) -> bool {
        self.page().selecting
    }

    /// The text of the selection.
    pub(crate) fn selected_text(&mut self) -> Option<String> {
        match self.session.answer_now(Command::Copy) {
            Answer::Copied(text) => text,
            _ => None,
        }
    }

    /// A picture of the terminal shown.
    pub(crate) fn picture(&mut self, id: zyt_term::ImageId) -> Option<Arc<Picture>> {
        match self.session.answer_now(Command::Picture(id)) {
            Answer::Picture(picture) => picture,
            _ => None,
        }
    }

    /// Pictures kept, what they weigh, and what they may weigh.
    pub(crate) fn pictures_held(&mut self) -> (usize, usize, usize) {
        self.session.settle();
        let grids = self.session.grids();
        (grids.pictures, grids.held, grids.budget)
    }

    /// Lines kept above the page.
    pub(crate) fn history_size(&mut self) -> usize {
        self.page().history_size
    }

    /// Columns and rows of the page.
    pub(crate) fn size(&mut self) -> (usize, usize) {
        let page = self.page();
        (page.columns, page.rows)
    }

    /// Lines the view is scrolled back.
    pub(crate) fn display_offset(&mut self) -> usize {
        self.page().display_offset
    }

    /// Moves the selection by a key.
    pub(crate) fn select_by_key(&mut self, step: SelectionStep) {
        self.command(Command::SelectStep(step));
    }

    /// Starts a selection on a cell of the page.
    pub(crate) fn selection_start(
        &mut self,
        kind: SelectionKind,
        column: usize,
        row: usize,
    ) -> zyt_term::Result<()> {
        self.command(Command::SelectStart { kind, column, row });
        Ok(())
    }

    /// Takes the selection away.
    pub(crate) fn selection_clear(&mut self) {
        self.command(Command::SelectClear);
    }

    /// Scrolls the view by lines, positive is up.
    pub(crate) fn scroll(&mut self, lines: i32) {
        self.command(Command::Scroll(lines));
    }

    /// Scrolls the view to the newest line.
    pub(crate) fn scroll_to_bottom(&mut self) {
        self.command(Command::ScrollToBottom);
    }

    /// Says how large one cell is in pixels.
    pub(crate) fn set_cell_size(&mut self, width: usize, height: usize) {
        self.session
            .apply(vec![Command::CellSize { width, height }]);
        self.session.settle();
    }

    /// The budget all pictures may weigh.
    pub(crate) fn set_picture_budget(&mut self, budget: usize) {
        let settings = self.session.settings;
        self.session
            .set_picture_limits(settings.picture_limit, settings.picture_weight, budget);
        self.session.settle();
    }
}
