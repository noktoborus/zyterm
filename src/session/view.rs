//! What the window reads of the screen, and what it asks of it.
//!
//! The page and the values are what the screen put on its board at the last
//! take, a frame old at most. A question — the text of the selection, a
//! picture, a step of the search — is asked with what the answer is for, and
//! the answer comes back on a later frame ([`Session::take_replies`]).

use super::{Grids, Session, Told};
use crate::screen::{ScreenCommand, ScreenSettings, TmuxCommand, TmuxLook};
use zyt_term::{Answer, Command, RenderableContent};

/// How long one turn of the screen goes on before the window says the screen
/// is busy.
pub const SCREEN_BUSY: std::time::Duration = std::time::Duration::from_secs(2);

/// What the window will do with an answer of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// The text of a selection that was made: it becomes the selection of the
    /// platform.
    Publish,
    /// The text of the selection, for the clipboard.
    Copy,
    /// The text of the selection, pasted into the line.
    Paste,
    /// The text of the selection, saved to a file; and opened, when asked.
    Save {
        /// Opened with the program of the desktop after it is saved.
        open_with: bool,
    },
    /// The text of the selection, written down in the history.
    Remember,
    /// The text of the selection, as the query of the search.
    SearchQuery,
    /// Whether a step of the search found a match.
    SearchStep,
    /// A picture, for the menu of it.
    PictureMenu(zyt_term::ImageId),
    /// A question of a test, which waits for it.
    #[cfg(test)]
    Test,
}

impl Session {
    /// The page of the terminal shown, as the screen put it last.
    pub fn content(&self) -> &RenderableContent {
        self.view.snapshot.page()
    }

    /// The same page, for a caller that holds the session mutably.
    pub fn page(&mut self) -> &RenderableContent {
        self.view.snapshot.page()
    }

    /// The snapshot the widget draws.
    pub fn snapshot(&mut self) -> &zyt_term::Snapshot {
        &self.view.snapshot
    }

    /// Sends what the widget asked of the terminal shown, in order.
    ///
    /// The size of a cell is kept here as well, because the local console is
    /// told the pixels of its window; a copy is asked as a question whose
    /// answer becomes the selection of the platform.
    pub fn apply(&mut self, commands: Vec<Command>) {
        for command in commands {
            match command {
                Command::CellSize { width, height } => {
                    self.cell = (width, height);
                    self.send(ScreenCommand::Terminal(command));
                }
                Command::Copy => self.ask(Purpose::Publish, Command::Copy),
                command => self.send(ScreenCommand::Terminal(command)),
            }
        }
    }

    /// Sends one command for the terminal shown.
    pub fn command(&mut self, command: Command) {
        self.send(ScreenCommand::Terminal(command));
    }

    /// Asks the terminal shown a question; the answer comes back with
    /// `purpose` on a later frame.
    pub fn ask(&mut self, purpose: Purpose, question: Command) {
        let tag = self.next_tag;
        self.next_tag += 1;
        self.asked.insert(tag, purpose);
        self.send(ScreenCommand::Ask { tag, question });
    }

    /// The answers that came since the last call, oldest first.
    pub fn take_replies(&mut self) -> Vec<(Purpose, Answer)> {
        std::mem::take(&mut self.replies)
    }

    /// True while the screen has been inside one turn for [`SCREEN_BUSY`] or
    /// longer: the page shown is the last one it put, and the window does not
    /// wait for the next.
    pub fn screen_busy(&self) -> bool {
        self.screen
            .busy_for()
            .is_some_and(|took| took >= SCREEN_BUSY)
    }

    /// True while the screen is inside a turn.
    pub fn screen_turning(&self) -> bool {
        self.screen.busy_for().is_some()
    }

    /// What the grids of the screen hold.
    pub fn grids(&self) -> Grids {
        self.view.said.grids
    }

    /// The pixels of one cell, as the widget last told the terminal.
    pub fn cell_pixels(&self) -> (usize, usize) {
        self.cell
    }

    /// The chunk the screen last read from the source: what it held, and
    /// what its buffer grew to.
    pub fn read_buffer(&self) -> (usize, usize) {
        self.view.said.chunk
    }

    /// Sends what is typed to the device: in tmux control mode to the pane
    /// shown, through `send-keys`, and held while the mode is entered or left.
    pub fn write(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.send(ScreenCommand::Input(data.to_vec()));
        }
    }

    /// Writes a key typed: by `name` to tmux in control mode, which encodes it
    /// for the terminal of its pane; as `bytes` everywhere else.
    pub fn write_key(&mut self, name: &str, bytes: &[u8]) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Key {
            name: name.to_string(),
            bytes: bytes.to_vec(),
        }));
    }

    /// Clears the page of the terminal shown and its pictures, and puts away
    /// what a program dressed the window in.
    pub fn clear_screen(&mut self) {
        self.send(ScreenCommand::Clear);
        self.clear_reports();
    }

    /// Puts away everything a program dressed the window in through OSC.
    ///
    /// Clearing the terminal takes away everything a program printed, and what
    /// it set around that output is no more current than the output itself: a
    /// title naming a command that is gone, a palette a program painted for a
    /// screen that is now empty, a share of work that will never be reported
    /// again. So the title goes back to the name of the program, the colors go
    /// back to the palette of the settings — `OSC 104` for the sixteen and the
    /// 256, `110`, `111` and `112` for the text, the background and the cursor —
    /// and the share goes away with them.
    ///
    /// The times of the plate go with them. They are readings of output that
    /// has been thrown away — when it began, when it ended, how long the other
    /// side took — and a track drawn over an empty screen measures something
    /// nobody can look at any more.
    ///
    /// Where the shell stands (OSC 7) is not touched. It is not a decoration
    /// somebody put on the window: it says where the shell really is, and a
    /// window that forgot it would be a window that lies about it.
    pub fn clear_reports(&mut self) {
        self.title = None;
        self.terminal_progress = None;
        self.link.forget_times();
        self.counters = self.link.counters();
        self.send(ScreenCommand::ClearReports);
    }

    /// Commands the shell of the source marked since this was last asked.
    ///
    /// The session holds them for a moment and nothing longer: where a command
    /// is written down is a question about configuration files, which this
    /// knows nothing of.
    pub fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }

    /// How fast the source is talking, in kibibytes a second over the last
    /// second.
    ///
    /// It is what the ladder of waits is read with, and what the window of
    /// numbers shows. A source that has said nothing for a second reads as
    /// nought.
    pub fn byte_rate(&mut self) -> f32 {
        self.rate.per_second()
    }

    /// How far along a program of the device says it is (OSC 9;4), when it
    /// says: what the status bar shows.
    ///
    /// A script holding the line drops it when it starts, because the program
    /// it was about no longer reaches the window; clearing the screen and a
    /// disconnect drop it too.
    pub fn terminal_progress(&self) -> Option<zyt_term::ProgressState> {
        self.terminal_progress
    }
    /// Changes what a program may do with the clipboard.
    pub fn set_clipboard_access(&mut self, access: zyt_term::ClipboardAccess) {
        let settings = ScreenSettings {
            clipboard: access,
            ..self.settings
        };
        self.set_screen(settings);
    }

    /// Changes what a picture a program sends may do.
    pub fn set_picture_access(&mut self, access: zyt_term::PictureAccess) {
        let settings = ScreenSettings {
            pictures: access,
            ..self.settings
        };
        self.set_screen(settings);
    }

    /// Gives every terminal of the screen the limits of its pictures: the
    /// largest one, the weight a picture is measured by, and the budget of
    /// all it keeps.
    pub fn set_picture_limits(
        &mut self,
        limit: zyt_term::PictureLimit,
        weight: usize,
        budget: usize,
    ) {
        let settings = ScreenSettings {
            picture_limit: limit,
            picture_weight: weight,
            picture_budget: budget,
            ..self.settings
        };
        self.set_screen(settings);
    }

    /// Gives every terminal of the screen the lines of scrollback that
    /// `memory` comes to at its width.
    pub fn set_scrollback_memory(&mut self, memory: usize) {
        let settings = ScreenSettings {
            scrollback_memory: memory,
            ..self.settings
        };
        self.set_screen(settings);
    }

    /// Whether `ESC P1000p` may start tmux in control mode.
    pub fn set_tmux_control(&mut self, on: bool) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Allowed(on)));
    }

    /// The button of tmux was pressed.
    pub fn press_tmux(&mut self) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Press));
    }

    /// `tmux.pause` as it stands.
    pub fn set_tmux_pause(&mut self, pause: Option<u32>) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Pause(pause)));
    }

    /// How far the screen of a pane may reach past the area, columns and rows.
    pub fn set_screen_overflow(&mut self, overflow: (usize, usize)) {
        self.overflow = overflow;
        self.send(ScreenCommand::Tmux(TmuxCommand::Overflow(overflow)));
    }

    /// The next screen of history of the pane shown, from tmux.
    pub fn tmux_load_history(&mut self) {
        self.send(ScreenCommand::Tmux(TmuxCommand::LoadHistory));
    }

    /// `RX` pressed: what piled up for the pane shown is skipped.
    pub fn tmux_jump(&mut self) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Jump));
    }

    /// Does what was chosen in the menu of tmux.
    pub fn tmux_choose(&mut self, choice: crate::tmux::Choice) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Choose(choice)));
    }

    /// The menu of tmux opened: the lists and the notice are asked again.
    pub fn tmux_refresh(&mut self) {
        self.send(ScreenCommand::Tmux(TmuxCommand::Refresh));
    }

    /// Whether a key typed goes to tmux by its name.
    pub fn tmux_takes_names(&self) -> bool {
        self.view.said.tmux.takes_names
    }

    /// What the button of tmux shows; `None` without the mode.
    pub fn tmux_look(&self) -> Option<TmuxLook> {
        self.view.said.tmux.look.clone()
    }

    /// What the menu of tmux is made from; `None` while the mode does not
    /// stand.
    pub fn tmux_menu(&self) -> Option<crate::tmux::Menu> {
        self.view.said.tmux.menu.clone()
    }

    /// How far the screen shown is behind the other end, and whether output
    /// of it was thrown away.
    pub fn tmux_lag(&self) -> Option<(std::time::Duration, bool)> {
        self.view.said.tmux.lag
    }

    /// What row −1 of the pane shown says.
    pub fn tmux_above_top(&self) -> Option<String> {
        self.view.said.tmux.above_top.clone()
    }

    /// Whether a pane chosen is on its way while the old one stays shown.
    pub fn tmux_switching(&self) -> bool {
        self.view.said.tmux.switching
    }

    /// What the debug window says of the mode; `None` without it.
    pub fn tmux_report(&self) -> Option<crate::tmux::Report> {
        self.view.said.tmux.report.clone()
    }

    /// Whether tmux or a run that holds the line sets the size of the grid:
    /// the window does not, and draws a grid of another size.
    pub fn size_held(&self) -> bool {
        self.view.said.tmux.sets_size || self.run_holds_line()
    }

    /// True once since the terminal shown changed: a pane shown, the mode
    /// over.
    pub fn take_view_changed(&mut self) -> bool {
        std::mem::take(&mut self.view_changed)
    }

    /// True once since the lists of tmux came back.
    pub fn take_tmux_lists_changed(&mut self) -> bool {
        std::mem::take(&mut self.lists_changed)
    }

    /// The features of the terminal the run standing switched off.
    pub fn off(&self) -> crate::scripts::Off {
        *self.off.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Takes what the screen put on its board: the page, the values, and
    /// what it told once.
    pub(super) fn take_screen(&mut self) {
        self.screen.take(&mut self.view);
        let said = &self.view.said;
        self.title = said.title.clone();
        self.working_directory = said.working_directory.clone();
        self.terminal_progress = said.progress;
        self.ended = said.ended;
        self.counters = self.link.counters();
        if said.clipboard_requested && !self.clipboard_said {
            self.clipboard_requested = true;
        }
        self.clipboard_said = said.clipboard_requested;
        let fresh = self.counters.bytes_in.saturating_sub(self.rated);
        if fresh > 0 {
            self.rate.push(fresh as usize);
        }
        self.rated = self.counters.bytes_in;

        let told = std::mem::take(&mut self.view.told);
        self.bell |= told.bells > 0 && !self.off().bell;
        if told.clipboard_store.is_some() {
            self.clipboard_store = told.clipboard_store;
        }
        self.notifications.extend(told.notifications);
        self.commands.extend(told.commands);
        self.view_changed |= told.view_changed;
        self.lists_changed |= told.lists_changed;
        for message in told.messages {
            self.told.push(Told {
                message,
                details: Vec::new(),
            });
        }
        for (tag, answer) in told.answers {
            if let Some(purpose) = self.asked.remove(&tag) {
                self.replies.push((purpose, answer));
            }
        }
    }
}
