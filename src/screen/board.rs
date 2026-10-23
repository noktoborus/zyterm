//! What the screen hands to the window, and how the window takes it.
//!
//! The screen puts a copy of the page and what the terminal said on the
//! board, under one short lock. The window swaps the page out, so neither
//! side works under the lock and the page is copied once.

use crate::session::Notification;
use std::path::PathBuf;
use zyt_term::{Answer, ProgressState, RenderableContent};

/// The values the terminal said last: each one stands until it says another.
#[derive(Debug, Clone, Default)]
pub struct Said {
    /// The title a program set.
    pub title: Option<String>,
    /// The directory a shell reported.
    pub working_directory: Option<PathBuf>,
    /// How far along a program said it is.
    pub progress: Option<ProgressState>,
    /// True once the program of the source said it ended.
    pub ended: bool,
    /// True while a program waits for the clipboard.
    pub clipboard_requested: bool,
    /// True while a selection holds the line unread.
    pub held_for_selection: bool,
    /// The state of the plain terminal, and how many bytes of the echo of
    /// the run it was fed (`zyt.term.state`).
    pub state: Option<(zyt_term::TermState, u64)>,
    /// What tmux in control mode stands at.
    pub tmux: TmuxSaid,
    /// What the grids hold, summed over every terminal of the screen.
    pub grids: crate::session::Grids,
    /// The chunk last read from the line, and what its buffer grew to.
    pub chunk: (usize, usize),
}

/// What the window reads of tmux in control mode.
#[derive(Debug, Clone, Default)]
pub struct TmuxSaid {
    /// What the button of tmux shows; nothing without the mode.
    pub look: Option<crate::screen::TmuxLook>,
    /// What the menu of tmux is made from, while the mode stands.
    pub menu: Option<crate::tmux::Menu>,
    /// How far the pane shown is behind, and whether output was thrown away.
    pub lag: Option<(std::time::Duration, bool)>,
    /// What row −1 of the pane shown says.
    pub above_top: Option<String>,
    /// A pane chosen is on its way while the old one stays shown.
    pub switching: bool,
    /// What the debug window says of the mode.
    pub report: Option<crate::tmux::Report>,
    /// tmux sets the size of what is shown.
    pub sets_size: bool,
    /// A key typed goes to tmux by its name.
    pub takes_names: bool,
}

/// What happened once, for the window to act on once.
#[derive(Debug, Default)]
pub struct Told {
    /// How many times the bell rang.
    pub bells: usize,
    /// Text a program asked to put into the clipboard.
    pub clipboard_store: Option<String>,
    /// Notifications a program asked for.
    pub notifications: Vec<Notification>,
    /// Commands a shell marked.
    pub commands: Vec<String>,
    /// Answers to the questions of the window, with their tags.
    pub answers: Vec<(u64, Answer)>,
    /// The terminal shown changed: a pane shown, the mode over.
    pub view_changed: bool,
    /// The lists of tmux came back: a menu of tmux standing is refilled.
    pub lists_changed: bool,
    /// What tmux had to say, for the desktop.
    pub messages: Vec<String>,
}

impl Told {
    /// Adds what was told later to what was told before.
    fn append(&mut self, later: Told) {
        self.bells += later.bells;
        if later.clipboard_store.is_some() {
            self.clipboard_store = later.clipboard_store;
        }
        self.notifications.extend(later.notifications);
        self.commands.extend(later.commands);
        self.answers.extend(later.answers);
        self.view_changed |= later.view_changed;
        self.lists_changed |= later.lists_changed;
        self.messages.extend(later.messages);
    }

    /// True when nothing was told.
    pub fn is_empty(&self) -> bool {
        self.bells == 0
            && self.clipboard_store.is_none()
            && self.notifications.is_empty()
            && self.commands.is_empty()
            && self.answers.is_empty()
            && !self.view_changed
            && !self.lists_changed
            && self.messages.is_empty()
    }
}

/// The board the two sides meet at.
#[derive(Debug, Default)]
pub struct Board {
    page: RenderableContent,
    revision: u64,
    /// The page was put and not taken yet; after a take the board holds the
    /// old buffer of the window.
    fresh: bool,
    said: Said,
    told: Told,
}

impl Board {
    /// Puts the page and the values on the board, and what was told beside
    /// what the window has not taken yet. The page is copied into a buffer the
    /// board keeps, so in a steady state nothing is allocated.
    pub fn put(&mut self, page: &RenderableContent, revision: u64, said: &Said, told: Told) {
        self.page.clone_from(page);
        self.fresh = true;
        self.revision = revision;
        self.said.clone_from(said);
        self.told.append(told);
    }

    /// The state of the plain terminal as it was put last.
    pub fn state(&self) -> Option<(zyt_term::TermState, u64)> {
        self.said.state.clone()
    }

    /// Hands the page to the window when one was put since the last take, by
    /// a swap with the buffer of the window, and the values and what was told.
    pub fn take(
        &mut self,
        page: &mut RenderableContent,
        revision: &mut u64,
        said: &mut Said,
        told: &mut Told,
    ) {
        if std::mem::take(&mut self.fresh) {
            std::mem::swap(page, &mut self.page);
            *revision = self.revision;
        }
        said.clone_from(&self.said);
        told.append(std::mem::take(&mut self.told));
    }
}
