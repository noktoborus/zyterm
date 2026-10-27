//! The screen itself: the terminal, fed in a thread of its own.
//!
//! One turn takes the commands of the window, reads the line, feeds the
//! terminal, sends what the terminal answered and puts what changed on the
//! board. A turn that found nothing to do waits for the one signal
//! ([`Wake`]) that bytes, commands and portions all ring.

mod mode;

pub use mode::TmuxLook;

use crate::screen::board::{Board, Said, Told};
use crate::screen::command::{ScreenCommand, ScreenSettings};
use crate::screen::wake::Wake;
use crate::screen::wire::Wire;
use crate::session::{Notification, new_terminal};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zyt_term::{Snapshot, Terminal, TerminalConfig, TerminalEvent};

/// The longest a screen with nothing to do waits before it looks again.
///
/// Every source rings when bytes arrive, so this is only the floor under a
/// ring that was lost.
const IDLE: Duration = Duration::from_millis(250);

/// What the window gave the screen to start with.
pub struct Setup {
    /// The first size of the grid.
    pub columns: usize,
    /// The first size of the grid.
    pub rows: usize,
    /// The settings of the terminal.
    pub settings: ScreenSettings,
}

/// Whether the thread goes on.
#[derive(Debug, PartialEq, Eq)]
pub enum Flow {
    /// It goes on; true when the turn did something.
    Busy(bool),
    /// The window asked it to stop, or went away.
    Stop,
}

/// The screen, owned by its thread.
pub struct Screen {
    terminal: Terminal,
    snapshot: Snapshot,
    settings: ScreenSettings,
    wire: Option<Arc<dyn Wire>>,
    /// What the script of the run shows in the terminal.
    echo: Option<Arc<zyt_script::ByteSwap>>,
    /// How many bytes of that the terminal was fed.
    echo_fed: u64,
    commands: Receiver<ScreenCommand>,
    board: Arc<Mutex<Board>>,
    wake: Arc<Wake>,
    shown: Arc<dyn Fn() + Send + Sync>,
    /// Since when a turn is under way; nothing between two turns.
    turning: Arc<Mutex<Option<std::time::Instant>>>,
    said: Said,
    /// A value of `said` moved since the board last had it.
    said_moved: bool,
    told: Told,
    published: Option<(u64, u64)>,
    /// The panes of tmux drawn beside the one shown: their snapshots, and the
    /// mark of the terminal each was taken of.
    beside: std::collections::BTreeMap<zyt_tmux::PaneId, (Snapshot, (u64, u64))>,
    spare: Vec<u8>,
    /// Whether bytes may be lost on the line.
    lossy: bool,
    /// The size of the read buffer, which bounds what tmux holds.
    read_buffer: Option<usize>,
    /// True while `ESC P1000p` may start tmux in control mode.
    tmux_control: bool,
    /// Finds tmux control mode in the bytes of the line.
    splitter: zyt_tmux::Splitter,
    /// `ESC P1000p` came and its first line is awaited.
    entering: Option<mode::Entering>,
    /// tmux in control mode, from its first line to its end.
    tmux: Option<crate::tmux::Control>,
    /// What stood before the mode and comes back after it.
    plain: mode::Plain,
    /// The size of the area, as the window last asked.
    area: (usize, usize),
    /// How far the screen of a pane of tmux may reach past the area.
    overflow: (usize, usize),
    /// The `pause-after` asked of tmux.
    tmux_pause: Option<u32>,
}

impl Screen {
    /// A screen with a terminal of the size and settings of `setup`.
    pub fn new(
        setup: Setup,
        commands: Receiver<ScreenCommand>,
        board: Arc<Mutex<Board>>,
        wake: Arc<Wake>,
        shown: Arc<dyn Fn() + Send + Sync>,
        turning: Arc<Mutex<Option<std::time::Instant>>>,
    ) -> crate::error::Result<Self> {
        let terminal = terminal(setup.columns, setup.rows, &setup.settings)?;
        Ok(Self {
            terminal,
            snapshot: Snapshot::new(),
            settings: setup.settings,
            wire: None,
            echo: None,
            echo_fed: 0,
            commands,
            board,
            wake,
            shown,
            turning,
            said: Said::default(),
            said_moved: false,
            told: Told::default(),
            published: None,
            beside: std::collections::BTreeMap::new(),
            spare: Vec::new(),
            lossy: false,
            read_buffer: None,
            tmux_control: true,
            splitter: zyt_tmux::Splitter::new(),
            entering: None,
            tmux: None,
            plain: mode::Plain::default(),
            area: (setup.columns, setup.rows),
            overflow: (
                crate::config::DEFAULT_SCREEN_OVERFLOW_COLUMNS,
                crate::config::DEFAULT_SCREEN_OVERFLOW_ROWS,
            ),
            tmux_pause: None,
        })
    }

    /// Turns until asked to stop, waiting whenever a turn found nothing to do.
    ///
    /// A turn that panics — a fault in the emulation or in tmux, on bytes the
    /// device sent — does not end the thread: the terminal is made anew and
    /// the window is told ([`Self::recover`]).
    pub fn run(mut self) {
        loop {
            self.mark_turn(true);
            let flow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.turn()));
            self.mark_turn(false);
            match flow {
                Ok(Flow::Stop) => break,
                Ok(Flow::Busy(true)) => {}
                Ok(Flow::Busy(false)) => self.wake.wait(IDLE),
                Err(_) => self.recover(),
            }
        }
    }

    /// Says on the shared mark whether a turn is under way, and since when.
    fn mark_turn(&self, turning: bool) {
        *self
            .turning
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = turning.then(std::time::Instant::now);
    }

    /// Makes the screen anew after a turn panicked: a fresh terminal, no tmux,
    /// and a word to the window. What the device says next is fed to it.
    fn recover(&mut self) {
        log::error!("a turn of the screen panicked; the terminal is made anew");
        self.tmux = None;
        self.entering = None;
        self.splitter = zyt_tmux::Splitter::new();
        self.plain = mode::Plain::default();
        self.reset();
        self.told.view_changed = true;
        self.told
            .messages
            .push(rust_i18n::t!("screen.reset").to_string());
        self.published = None;
        self.publish();
    }

    /// One turn: the commands, the line, the answers, the board.
    pub fn turn(&mut self) -> Flow {
        let mut busy = match self.take_commands() {
            Some(busy) => busy,
            None => return Flow::Stop,
        };
        busy |= self.read();
        self.answer();
        busy |= self.take_echo();
        self.take_events();
        self.publish();
        Flow::Busy(busy)
    }

    /// Applies every command waiting; nothing when the window went away or
    /// asked the screen to stop.
    fn take_commands(&mut self) -> Option<bool> {
        let mut busy = false;
        loop {
            let command = match self.commands.try_recv() {
                Ok(command) => command,
                Err(TryRecvError::Empty) => return Some(busy),
                Err(TryRecvError::Disconnected) => return None,
            };
            busy = true;
            match command {
                ScreenCommand::Stop => return None,
                command => self.apply(command),
            }
        }
    }

    fn apply(&mut self, command: ScreenCommand) {
        match command {
            ScreenCommand::Pane(pane, command) => {
                if let Some(terminal) = self
                    .tmux
                    .as_mut()
                    .and_then(|control| control.pane_view_mut(pane))
                {
                    terminal.apply(command);
                }
            }
            ScreenCommand::Terminal(command) => {
                if matches!(command, zyt_term::Command::AnswerClipboard(_)) {
                    self.said.clipboard_requested = false;
                    self.said_moved = true;
                }
                self.shown_mut().apply(command);
            }
            ScreenCommand::Ask { tag, question } => {
                if let Some(answer) = self.shown_mut().apply(question) {
                    self.told.answers.push((tag, answer));
                }
            }
            ScreenCommand::Resize { columns, rows } => self.resize(columns, rows),
            ScreenCommand::Input(bytes) => self.input(&bytes),
            #[cfg(test)]
            ScreenCommand::Print(bytes) => self.terminal.feed(&bytes),
            ScreenCommand::Echo(echo) => {
                self.take_echo();
                self.echo = echo;
                self.echo_fed = 0;
                self.said_moved = true;
            }
            ScreenCommand::Clear => {
                let shown = self.shown_mut();
                shown.feed(crate::session::CLEAR_SCREEN);
                shown.clear_pictures();
            }
            ScreenCommand::ClearReports => {
                self.said.title = None;
                self.said.progress = None;
                self.said_moved = true;
                self.terminal
                    .feed(b"\x1b]104\x07\x1b]110\x07\x1b]111\x07\x1b]112\x07");
            }
            ScreenCommand::Wire { wire, lossy } => {
                self.drop_tmux();
                self.wire = wire;
                self.lossy = lossy;
                self.said = Said::default();
                self.said_moved = true;
            }
            ScreenCommand::Tmux(command) => self.tmux_command(command),
            ScreenCommand::Reset => self.reset(),
            ScreenCommand::Settings(settings) => self.set(settings),
            ScreenCommand::Stop => {}
            #[cfg(test)]
            ScreenCommand::Panic => panic!("a fault asked for by a test"),
            #[cfg(test)]
            ScreenCommand::Sleep(how_long) => std::thread::sleep(how_long),
        }
    }

    /// Reads the line, unless a selection or a script holds it.
    fn read(&mut self) -> bool {
        let Some(wire) = &self.wire else {
            return false;
        };
        let held = self.shown().selecting() && wire.owned();
        self.said_moved |= held != self.said.held_for_selection;
        self.said.held_for_selection = held;
        if held {
            return false;
        }
        wire.read_into(&mut self.spare);
        if self.spare.is_empty() {
            return false;
        }
        let mut bytes = std::mem::take(&mut self.spare);
        self.take_in(&mut bytes);
        self.spare = bytes;
        true
    }

    /// Sends what the terminal answered a program; while a script owns the
    /// line the line drops it: the script speaks for this side then. Under
    /// tmux the pane shown answers through tmux, the way a key does.
    fn answer(&mut self) {
        let plain = self.terminal.take_output();
        if self.tmux.is_some() {
            let pane = self.shown_mut().take_output();
            self.input(&pane);
            return;
        }
        self.write(&plain);
    }

    /// The terminal shown: the pane of tmux shown, or the plain one.
    fn shown(&self) -> &Terminal {
        self.tmux
            .as_ref()
            .and_then(crate::tmux::Control::shown)
            .unwrap_or(&self.terminal)
    }

    /// The terminal shown, to change.
    fn shown_mut(&mut self) -> &mut Terminal {
        match &mut self.tmux {
            Some(control) if control.shown().is_some() => {
                control.shown_mut().expect("a pane is shown")
            }
            _ => &mut self.terminal,
        }
    }

    /// Prints what the script of the run showed since the last turn.
    fn take_echo(&mut self) -> bool {
        let Some(echo) = &self.echo else {
            return false;
        };
        echo.take_into(&mut self.spare);
        if self.spare.is_empty() {
            return false;
        }
        self.terminal.feed(&self.spare);
        self.echo_fed += self.spare.len() as u64;
        true
    }

    /// Sends bytes to the line; the line counts them (`crate::line`).
    fn write(&mut self, bytes: &[u8]) {
        if let Some(wire) = &self.wire {
            wire.write(bytes);
        }
    }

    fn reset(&mut self) {
        let (columns, rows) = self.terminal.size();
        let cell = self.terminal.cell_size();
        match terminal(columns, rows, &self.settings) {
            Ok(mut fresh) => {
                fresh.set_cell_size(cell.0, cell.1);
                self.terminal = fresh;
                self.said.title = None;
                self.said.working_directory = None;
                self.said.progress = None;
                self.said_moved = true;
            }
            Err(error) => log::warn!("the screen keeps its terminal: {error}"),
        }
    }

    fn set(&mut self, settings: ScreenSettings) {
        self.settings = settings;
        self.terminal.set_clipboard_access(settings.clipboard);
        self.terminal.set_picture_access(settings.pictures);
        self.terminal.set_picture_limit(settings.picture_limit);
        self.terminal.set_picture_weight(settings.picture_weight);
        if self.terminal.pictures_held().2 != settings.picture_budget {
            self.terminal.set_picture_budget(settings.picture_budget);
        }
        self.keep_scrollback();
        self.configure_panes();
    }

    /// The lines of scrollback the memory of the settings comes to at the
    /// width of the grid.
    fn keep_scrollback(&mut self) {
        let columns = self.terminal.size().0;
        self.terminal
            .set_scrollback(crate::config::scrollback_lines(
                self.settings.scrollback_memory,
                columns,
            ));
    }

    fn take_events(&mut self) {
        for event in self.take_terminal_events() {
            self.said_moved = true;
            match event {
                TerminalEvent::Title(title) => self.said.title = Some(title),
                TerminalEvent::ResetTitle => self.said.title = None,
                TerminalEvent::Bell => self.told.bells += 1,
                TerminalEvent::ClipboardStore(text) => self.told.clipboard_store = Some(text),
                TerminalEvent::ClipboardRequest => self.said.clipboard_requested = true,
                TerminalEvent::WorkingDirectory(path) => {
                    self.said.working_directory = Some(std::path::PathBuf::from(path));
                }
                TerminalEvent::Notification { kind, title, body } => {
                    self.told
                        .notifications
                        .push(Notification { kind, title, body });
                }
                TerminalEvent::Mark(_) => {}
                TerminalEvent::Command(line) => {
                    if self.settings.marks {
                        self.told.commands.push(line);
                    }
                }
                TerminalEvent::Progress(state) => {
                    if self.settings.progress {
                        self.said.progress = match state {
                            zyt_term::ProgressState::Removed => None,
                            state => Some(state),
                        };
                    }
                }
                TerminalEvent::Exit => self.said.ended = true,
            }
        }
    }

    /// Puts the page on the board when the terminal changed, and what was said
    /// and told whenever there is any; then tells the window. While tmux is
    /// entered or stands it puts them every turn: what it says moves with the
    /// clock as well.
    fn publish(&mut self) {
        let mark = (self.shown().identity(), self.shown().changes());
        let changed = self.published != Some(mark);
        let in_mode = self.tmux.is_some() || self.entering.is_some();
        if !changed && self.told.is_empty() && !self.said_moved && !in_mode {
            return;
        }
        self.said_moved = false;
        self.said.tmux = self.tmux_said();
        self.said.grids = self.grids();
        self.said.chunk = (self.spare.len(), self.spare.capacity());
        self.said.state = Some((self.terminal.state(), self.echo_fed));
        let (snapshot, shown) = match &mut self.tmux {
            Some(control) if control.shown().is_some() => (
                &mut self.snapshot,
                control.shown_mut().expect("a pane is shown"),
            ),
            _ => (&mut self.snapshot, &mut self.terminal),
        };
        snapshot.take(shown);
        self.published = Some(mark);
        let told = std::mem::take(&mut self.told);
        let shared = std::sync::Arc::clone(&self.board);
        let mut board = shared.lock().unwrap_or_else(|error| error.into_inner());
        board.put(
            self.snapshot.page(),
            self.snapshot.revision(),
            &self.said,
            told,
        );
        self.put_beside(&mut board);
        drop(board);
        (self.shown)();
    }

    /// The pages of the panes of tmux drawn beside the one shown, each put
    /// when its terminal changed since it was put last.
    fn put_beside(&mut self, board: &mut crate::screen::board::Board) {
        let shown = self
            .tmux
            .as_ref()
            .and_then(|control| control.report().shown);
        let panes: Vec<zyt_tmux::PaneId> = self
            .said
            .tmux
            .places
            .panes
            .iter()
            .map(|place| place.pane)
            .filter(|pane| Some(*pane) != shown)
            .collect();
        self.beside.retain(|pane, _| panes.contains(pane));
        board.keep_beside(&panes);
        let Some(control) = self.tmux.as_mut() else {
            return;
        };
        for pane in panes {
            let Some(terminal) = control.pane_view_mut(pane) else {
                continue;
            };
            let mark = (terminal.identity(), terminal.changes());
            let (snapshot, put) = self
                .beside
                .entry(pane)
                .or_insert_with(|| (Snapshot::new(), (u64::MAX, u64::MAX)));
            if *put == mark {
                continue;
            }
            snapshot.take(terminal);
            *put = mark;
            board.put_beside(pane, snapshot.page(), snapshot.revision());
        }
    }
}

impl Screen {
    /// What the grids hold, summed over every terminal: the plain one and,
    /// under tmux, each pane with one.
    fn grids(&self) -> crate::session::Grids {
        let panes = self
            .tmux
            .as_ref()
            .into_iter()
            .flat_map(crate::tmux::Control::terminals);
        let mut grids = crate::session::Grids::default();
        for terminal in std::iter::once(&self.terminal).chain(panes) {
            let width = terminal.size().0;
            let (pictures, held, budget) = terminal.pictures_held();
            grids.count += 1;
            grids.history += terminal.history_size();
            grids.cap += terminal.scrollback();
            grids.memory += terminal.history_size() * width * zyt_term::GRID_CELL_BYTES;
            grids.pictures += pictures;
            grids.held += held;
            grids.budget += budget;
        }
        grids
    }
}

/// A terminal of the size and settings given.
fn terminal(
    columns: usize,
    rows: usize,
    settings: &ScreenSettings,
) -> crate::error::Result<Terminal> {
    let mut terminal = new_terminal(TerminalConfig {
        columns,
        rows,
        scrollback: crate::config::scrollback_lines(settings.scrollback_memory, columns),
        clipboard: settings.clipboard,
    })?;
    terminal.set_picture_access(settings.pictures);
    terminal.set_picture_limit(settings.picture_limit);
    terminal.set_picture_weight(settings.picture_weight);
    terminal.set_picture_budget(settings.picture_budget);
    Ok(terminal)
}
