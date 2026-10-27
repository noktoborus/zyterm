//! What the window asks of the screen.

use crate::screen::wire::Wire;
use std::sync::Arc;

/// One thing the window asks of the screen. They are applied in order.
pub enum ScreenCommand {
    /// A command for the terminal shown.
    Terminal(zyt_term::Command),
    /// A command for a pane of tmux drawn beside the one shown.
    Pane(zyt_tmux::PaneId, zyt_term::Command),
    /// A question for the terminal shown; the answer comes back on the board
    /// with the tag, on a later frame.
    Ask {
        /// What the window will do with the answer, in its own words.
        tag: u64,
        /// The question: [`zyt_term::Command::Copy`] or
        /// [`zyt_term::Command::Picture`].
        question: zyt_term::Command,
    },
    /// The size of the grid.
    Resize {
        /// Columns.
        columns: usize,
        /// Rows.
        rows: usize,
    },
    /// What the person typed, pasted or picked, for the line.
    Input(Vec<u8>),
    /// Bytes fed to the plain terminal by a test, as if the device said them.
    #[cfg(test)]
    Print(Vec<u8>),
    /// What the script of a run shows in the terminal (`to_screen`); the
    /// screen takes it in each turn. Nothing once the run is over: what was
    /// left is printed first.
    Echo(Option<Arc<zyt_script::ByteSwap>>),
    /// Clears the page of the terminal shown and its pictures.
    Clear,
    /// Puts away what a program dressed the window in: the title, the share
    /// of work, the colours, and the times of the output.
    ClearReports,
    /// The line the screen reads and writes; nothing while no source stands.
    /// A line that loses bytes — a serial port — is read by tmux with care.
    Wire {
        /// The line.
        wire: Option<Arc<dyn Wire>>,
        /// Whether bytes may be lost on it.
        lossy: bool,
    },
    /// A command for tmux in control mode.
    Tmux(TmuxCommand),
    /// A terminal of its own again, of the same size: a new source starts on a
    /// clean page.
    Reset,
    /// The settings of the terminal.
    Settings(ScreenSettings),
    /// Ends the thread of the screen.
    Stop,
    /// A fault in the middle of a turn, for a test of what comes after it.
    #[cfg(test)]
    Panic,
    /// A turn that takes this long, for a test of a busy screen.
    #[cfg(test)]
    Sleep(std::time::Duration),
}

/// What the window asks of tmux in control mode.
pub enum TmuxCommand {
    /// A key typed, by its name for tmux and its bytes for a plain terminal.
    Key {
        /// The name tmux knows the key by.
        name: String,
        /// The bytes of this terminal for it.
        bytes: Vec<u8>,
    },
    /// Whether `ESC P1000p` may start the mode.
    Allowed(bool),
    /// The button of tmux was pressed.
    Press,
    /// `pause-after`, from `tmux.pause`.
    Pause(Option<u32>),
    /// How far the screen of a pane may reach past the area.
    Overflow((usize, usize)),
    /// The size of the read buffer, which bounds what is held.
    ReadBuffer(Option<usize>),
    /// The next screen of history of the pane shown.
    LoadHistory,
    /// What piled up for the pane shown is skipped.
    Jump,
    /// What was chosen in the menu of tmux.
    Choose(crate::tmux::Choice),
    /// What the mouse reports in a pane drawn beside the one shown.
    InputTo(zyt_tmux::PaneId, Vec<u8>),
    /// The menu of tmux opened: the lists are asked again.
    Refresh,
    /// The port was lost.
    LineLost,
    /// The port opened again.
    LineBack,
}

/// The settings the screen holds for its terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSettings {
    /// What a program may do with the clipboard.
    pub clipboard: zyt_term::ClipboardAccess,
    /// Whether pictures are drawn.
    pub pictures: zyt_term::PictureAccess,
    /// Bytes of history the scrollback may keep.
    pub scrollback_memory: usize,
    /// Whether the marks of a shell are honoured.
    pub marks: bool,
    /// Whether a program may say how far along it is.
    pub progress: bool,
    /// The largest picture, and what a program asking is told.
    pub picture_limit: zyt_term::PictureLimit,
    /// The weight a picture is measured by.
    pub picture_weight: usize,
    /// Bytes all pictures may weigh.
    pub picture_budget: usize,
}

impl Default for ScreenSettings {
    fn default() -> Self {
        Self {
            clipboard: zyt_term::ClipboardAccess::default(),
            pictures: zyt_term::PictureAccess::default(),
            scrollback_memory: crate::config::Settings::default().scrollback_memory,
            marks: true,
            progress: true,
            picture_limit: zyt_term::PictureLimit::default(),
            picture_weight: zyt_term::DEFAULT_PICTURE_WEIGHT,
            picture_budget: zyt_term::DEFAULT_PICTURE_BUDGET,
        }
    }
}
