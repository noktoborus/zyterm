//! The active connection of one window: a serial port or the local console,
//! the screen fed by it in a thread of its own, and a running file transfer.
//!
//! The session is the window side. It holds the source and drives it — the
//! modem lines, the parameters, the stop — and runs the scripts. What arrives
//! is read and fed by the screen (`crate::screen`); the session takes what
//! the screen put on its board once a frame and sends it commands.

use crate::consoles::ConsoleId;
use crate::error::{AppError, Result};
use crate::screen::{ScreenCommand, ScreenHandle, ScreenSettings, Setup, View};
use rust_i18n::t;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_pty::PtySession;
use zyt_script::{Direction, Line as _, ScriptRun};
use zyt_serial::{ControlLines, LineHold, LineParams, PortSupervisor};
use zyt_term::{Terminal, TerminalConfig};

#[cfg(test)]
use zyt_script::Targets;

mod run;
mod source;
mod view;

pub use crate::screen::TmuxLook;
pub use view::Purpose;

/// A moment, kept as the two readings of it the window needs.
///
/// The clock says when something happened and a monotonic reading says how long
/// ago, and neither answers for the other: a clock that was put right while the
/// window stood open would make "ten seconds ago" come out as an hour, and a
/// monotonic reading names no time of day at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// Where it stands on the clock of this machine.
    pub at: jiff::Timestamp,
    /// What the time since is measured from.
    pub since: Instant,
}

impl Moment {
    /// This moment.
    pub fn now() -> Self {
        Self {
            at: jiff::Timestamp::now(),
            since: Instant::now(),
        }
    }

    /// How long ago it was.
    pub fn elapsed(&self) -> Duration {
        self.since.elapsed()
    }
}

/// Where the bytes of the screen come from.
///
/// Each is shared with the screen, which reads and writes it; the session
/// drives the rest of it.
pub enum Source {
    /// Nothing is connected.
    None,
    /// A serial port watched by its supervisor.
    Serial {
        /// Worker of the port.
        supervisor: Arc<PortSupervisor>,
        /// Path the user selected.
        path: String,
    },
    /// A local console.
    Console {
        /// Session of the console.
        session: Arc<PtySession>,
        /// The console this is, which is what it is addressed by.
        id: ConsoleId,
        /// Its name with the values it was opened with put in, which is what
        /// it is called where it is read.
        shown: String,
    },
}

/// What the grids of a session hold, summed over its terminals.
#[derive(Debug, Clone, Copy, Default)]
pub struct Grids {
    /// How many terminals there are.
    pub count: usize,
    /// Lines kept above the screens.
    pub history: usize,
    /// Lines the scrollbacks may keep.
    pub cap: usize,
    /// Bytes the kept lines cost.
    pub memory: usize,
    /// Pictures kept.
    pub pictures: usize,
    /// Bytes the pictures weigh.
    pub held: usize,
    /// Bytes the pictures may weigh.
    pub budget: usize,
}

/// What a session lets go of on the way out.
pub struct Leaving {
    script: Option<ScriptRun>,
    line: Option<Arc<crate::line::RunLine>>,
    source: Source,
}

impl Drop for Leaving {
    /// Lets go of the script first, so its programs stop before the source
    /// they write to closes; then the line, then the source.
    fn drop(&mut self) {
        drop(self.script.take());
        if let Some(line) = self.line.take() {
            line.close();
        }
        self.source = Source::None;
    }
}

/// A desktop notification a program asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Sequence that asked for it.
    pub kind: zyt_term::NotificationKind,
    /// Heading of the notification, empty when none was given.
    pub title: String,
    /// Text of the notification.
    pub body: String,
}

/// A source that keeps no values, which is what the scripts of the tests ask
/// for.
#[cfg(test)]
fn no_values() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::new()
}

/// Where the scripts written for these tests stand.
#[cfg(test)]
fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fish")
        .join("scripts")
}

/// Starts one of those scripts on a session.
#[cfg(test)]
pub(crate) fn start_fixture(session: &mut Session, name: &str, target: Targets) -> Result<()> {
    let roots = vec![
        fixtures(),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts"),
    ];
    let library = zyt_script::Library::load(&roots);
    let entry = library
        .find(name)
        .unwrap_or_else(|error| panic!("the fixture {name} is there: {error}"));

    session.start_script(
        entry,
        &roots,
        Direction::Send,
        target,
        &no_values(),
        std::sync::Arc::new(|| {}),
    )
}

/// What is sent into the terminal to clear it: the cursor home, the screen, the
/// scrollback, and the cursor back to the settings of this terminal.
pub(crate) const CLEAR_SCREEN: &[u8] = b"\x1b[H\x1b[2J\x1b[3J\x1b[?25h\x1b[0 q";

/// Bookkeeping of one running script, from the start to the free line.
///
/// It outlives the run itself: a script that ended has still to wait for the
/// last of its bytes to leave the driver, and the key it sends afterwards is
/// sent then and not before.
struct ScriptLife {
    /// The name of its directory, which is what its answers are kept under.
    id: String,
    /// The name a person reads.
    name: String,
    /// True while it has the line of the session to itself.
    holds_line: bool,
    /// When it started.
    started: Instant,
    /// When the script itself ended, before the line drained.
    ended: Option<Instant>,
    /// How far along the script said it is.
    progress: Option<zyt_term::ProgressState>,
    /// Where the script said it stands.
    stage: Option<zyt_script::Stage>,
    /// How many bytes it said went, and of how many.
    bytes: Option<(u64, Option<u64>)>,
    /// How fast those bytes go.
    pace: crate::rate::Pace,
    /// How fast the script said it goes, each way, as it said it: when it says
    /// so, nothing is worked out from its bytes.
    speed: Option<zyt_script::Speed>,
    /// Which way it carries files.
    direction: Direction,
    /// True once it was asked to stop.
    asked: bool,
    /// True once the script said it ends (`zyt.run.ending()`).
    ending: bool,
}

/// Where a run is, from its start to the end of its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    /// The script runs.
    Running,
    /// It was asked to stop and has not said it ends.
    Asked,
    /// It said it ends, asked to or by itself.
    Ending,
    /// It is over, and its last bytes wait for the line.
    Draining,
}

/// What forced a run to end, as the log says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedBy {
    /// The person, on the plate or by its key.
    Person,
    /// The source was let go of: a disconnect, or another source.
    Disconnect,
    /// The window closed.
    WindowClosed,
}

impl std::fmt::Display for ForcedBy {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Self::Person => "the person",
            Self::Disconnect => "a disconnect",
            Self::WindowClosed => "the window closing",
        })
    }
}

/// What a run came to, for the desktop: the session says it, the window
/// shows it ([`Session::take_told`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    /// The heading of the notification.
    pub message: String,
    /// Its body, a line each: why it failed, what was sent after it.
    pub details: Vec<String>,
}

/// State of the connection shown in the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Nothing is connected.
    Idle,
    /// The device is not present; the worker is waiting for it.
    Waiting,
    /// The port is being opened.
    Opening,
    /// Data flows.
    Connected,
}

/// Connection, screen and transfer of one window.
pub struct Session {
    /// The screen, fed in its thread.
    screen: ScreenHandle,
    /// What the window holds of the screen between two takes.
    view: View,
    /// The settings the screen was last sent.
    settings: ScreenSettings,
    /// Modem lines of the serial source.
    pub lines: ControlLines,
    /// What this side does with Request To Send.
    pub rts_hold: LineHold,
    /// What this side does with Data Terminal Ready.
    pub dtr_hold: LineHold,
    /// True while the transmission line is held in the break condition.
    pub held_break: bool,
    /// True while the port is left unread on purpose.
    pub read_hold: bool,
    /// Line parameters of the serial source, as they were asked for.
    pub params: LineParams,
    /// The line as the driver of the serial source has it; what was asked
    /// while the port is not open.
    pub driver_line: LineParams,
    /// Title reported by the program.
    pub title: Option<String>,
    source: Source,
    /// The size of the area, as the window last asked.
    area: (usize, usize),
    /// How far the grid may reach past the area (`screen_overflow`).
    overflow: (usize, usize),
    /// Pixels of one cell, as the widget last told the terminal.
    cell: (usize, usize),
    /// Pixels of one cell the source was last told about.
    ///
    /// The window size of a console carries them, and nothing resizes a grid
    /// because a font changed size, so this is what says the far side is
    /// holding numbers that have gone stale.
    cell_told: (u16, u16),
    script: Option<ScriptRun>,
    /// The line of the run standing.
    line: Option<Arc<crate::line::RunLine>>,
    /// The line of the session: the screen or a run owns it, and it counts.
    link: Arc<crate::line::Line>,
    /// The features of the terminal the run switched off.
    off: Arc<std::sync::Mutex<crate::scripts::Off>>,
    /// What the run changed on the port, not taken in yet.
    port_changed: Arc<std::sync::Mutex<crate::port::Changed>>,
    /// A run changed the parameters of the line, not written down yet.
    params_of_run: bool,
    heard: Option<crate::scripts::Heard>,
    waiting: Option<crate::scripts::Waiting>,
    run: Option<ScriptLife>,
    /// How many runs were started: the number of the one standing.
    runs: u64,
    read_buffer: Option<usize>,
    lines_interval: Duration,
    rate: crate::rate::RateMeter,
    /// The bytes that came, as far as the rate counted them.
    rated: u64,
    console_history: zyt_serial::LineHistory,
    last_sample: Option<Instant>,
    sampled_in: u64,
    sampled_out: u64,
    busy_since: Option<Instant>,
    last_busy: Option<Duration>,
    last_transfer: Option<Duration>,
    /// Text a program asked to put into the clipboard.
    pub clipboard_store: Option<String>,
    /// True while a program waits for the clipboard.
    pub clipboard_requested: bool,
    /// Whether the screen said a program waits for the clipboard, last frame.
    clipboard_said: bool,
    /// True while a bell nobody has answered yet has rung.
    pub bell: bool,
    /// True once a run ended and the window has not looked yet: it asks for
    /// attention the way a bell does.
    pub run_ended: bool,
    /// What a run came to, waiting for the window to say it.
    told: Vec<Told>,
    /// True from the loss of the port until it opens again.
    port_lost: bool,
    /// Directory a console reported last.
    pub working_directory: Option<std::path::PathBuf>,
    /// Notifications a program asked for.
    pub notifications: Vec<Notification>,
    /// How far along a program of the device said it is (OSC 9;4), until it
    /// says it is done. A script says it elsewhere: [`Session::run_progress`].
    terminal_progress: Option<zyt_term::ProgressState>,
    /// True when the program of the source ended.
    pub ended: bool,
    /// Commands the shell of the source marked, waiting to be written down.
    commands: Vec<String>,
    /// True while the marks of a shell are honoured.
    pub marks_enabled: bool,
    /// True while a program may say how far along it is.
    pub progress_enabled: bool,
    /// What went over the line, as the line counted it at the last frame.
    pub counters: crate::line::Counters,
    /// The questions asked of the screen and not answered yet, by their tags.
    asked: BTreeMap<u64, Purpose>,
    /// The tag of the next question.
    next_tag: u64,
    /// Answers the window has not acted on yet.
    replies: Vec<(Purpose, zyt_term::Answer)>,
    /// The terminal shown changed since the window last asked.
    view_changed: bool,
    /// The lists of tmux came back since the window last asked.
    lists_changed: bool,
}

impl Session {
    /// A session with no connection. `shown` is called from the thread of the
    /// screen when it has something new to show.
    pub fn new(shown: Arc<dyn Fn() + Send + Sync>) -> Result<Self> {
        let settings = ScreenSettings::default();
        let screen = ScreenHandle::spawn(
            Setup {
                columns: 80,
                rows: 24,
                settings,
            },
            shown,
        )?;
        let link = crate::line::Line::new(screen.waker());
        screen.send(ScreenCommand::Wire {
            wire: Some(link.screen()),
            lossy: false,
        });
        Ok(Self {
            screen,
            view: View::default(),
            settings,
            lines: ControlLines::default(),
            rts_hold: LineHold::default(),
            dtr_hold: LineHold::default(),
            held_break: false,
            read_hold: false,
            params: LineParams::default(),
            driver_line: LineParams::default(),
            title: None,
            source: Source::None,
            area: (80, 24),
            overflow: (0, 0),
            cell: (0, 0),
            cell_told: (0, 0),
            script: None,
            line: None,
            link,
            off: Default::default(),
            port_changed: Default::default(),
            params_of_run: false,
            heard: None,
            waiting: None,
            run: None,
            runs: 0,
            read_buffer: None,
            lines_interval: zyt_serial::DEFAULT_LINES_INTERVAL,
            rate: crate::rate::RateMeter::new(),
            rated: 0,
            console_history: zyt_serial::LineHistory::new(),
            last_sample: None,
            sampled_in: 0,
            sampled_out: 0,
            busy_since: None,
            last_busy: None,
            last_transfer: None,
            clipboard_store: None,
            clipboard_requested: false,
            clipboard_said: false,
            bell: false,
            run_ended: false,
            told: Vec::new(),
            port_lost: false,
            working_directory: None,
            notifications: Vec::new(),
            terminal_progress: None,
            ended: false,
            commands: Vec::new(),
            marks_enabled: true,
            progress_enabled: true,
            counters: Default::default(),
            asked: BTreeMap::new(),
            next_tag: 1,
            replies: Vec::new(),
            view_changed: false,
            lists_changed: false,
        })
    }

    /// Sends one command to the screen.
    fn send(&self, command: ScreenCommand) {
        self.screen.send(command);
    }

    /// Takes out all that holds a port, a program or a thread, for the way
    /// out, and waits for nothing.
    ///
    /// A dialog a script waits on is dismissed here, because it is drawn by
    /// the window that goes. The screen lets go of its share of the source.
    pub fn leave(&mut self) -> Leaving {
        if let Some(waiting) = self.waiting.take() {
            waiting.dismiss();
        }
        if let (Some(life), Some(phase)) = (self.run.as_ref(), self.run_phase()) {
            log::warn!(
                "the run {} was forced to end by {}, in phase {phase:?}",
                life.name,
                ForcedBy::WindowClosed
            );
        }
        self.send(ScreenCommand::Wire {
            wire: None,
            lossy: false,
        });
        Leaving {
            script: self.script.take(),
            line: self.line.take(),
            source: std::mem::replace(&mut self.source, Source::None),
        }
    }

    /// Takes in what the screen and the source said since the last frame, and
    /// drives the run.
    pub fn pump(&mut self) -> Vec<AppError> {
        let mut errors = Vec::new();
        self.take_screen();
        self.take_port_changes();
        self.sample_console();
        self.tick_script();
        self.drain_script();
        self.poll_source(&mut errors);
        self.tell_settings();
        self.track_line_busy();
        self.end_when_line_is_free();
        errors
    }

    /// Sends the settings of the screen when the window changed them.
    fn tell_settings(&mut self) {
        let mut settings = self.settings;
        settings.marks = self.marks_enabled;
        settings.progress = self.progress_enabled;
        self.set_screen(settings);
    }

    /// Sends settings to the screen when they differ from the last sent.
    fn set_screen(&mut self, settings: ScreenSettings) {
        if settings != self.settings {
            self.settings = settings;
            self.send(ScreenCommand::Settings(settings));
        }
    }
}

/// A terminal of this application: the emulation, and what this program puts
/// on a page for what the emulation does not draw.
///
/// Every terminal of a session goes through here, the first one and the one a
/// reconnection replaces it with, because a terminal built anywhere else would
/// be one that silently dropped a picture.
pub(crate) fn new_terminal(config: TerminalConfig) -> Result<Terminal> {
    let mut terminal = Terminal::new(config)?;
    terminal.set_sixel_placeholder(Some(std::sync::Arc::new(sixel_placeholder)));
    Ok(terminal)
}

/// The line a picture the terminal does not draw leaves in its place.
///
/// It says what the payload was — the size it declared and what it came to on
/// the line — because that is the whole of what is left of a picture nobody
/// drew, and a reader looking at a gap wants to know whether anything arrived
/// at all.
///
/// A payload that declared no size is a question mark and not a sentence: the
/// standard lets a payload leave the raster attribute out, and until the data
/// is read the size of it is not known. A mark says that in every language and
/// keeps the line short enough to stand between two words of output.
///
/// A picture the terminal refused says why — ignored, or heavier than one may
/// be — because it is gone for a reason the reader can change: it names no
/// picture and has no menu, and a setting is what would have kept it.
pub fn sixel_placeholder(report: &zyt_term::SixelReport) -> String {
    let size = match report.declared {
        Some((width, height)) => format!("{width}x{height}"),
        None => "?".to_string(),
    };
    let key = match report.refused {
        Some(zyt_term::SixelRefusal::Ignored) => "sixel.ignored",
        Some(zyt_term::SixelRefusal::TooLarge) => "sixel.too_large",
        None => "sixel.placeholder",
    };
    t!(
        key,
        size = size,
        volume = crate::format::size(report.bytes as u64)
    )
    .to_string()
}

#[cfg(test)]
mod test_terminal;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tmux_tests;

#[cfg(all(test, unix))]
mod slow_tests;

#[cfg(test)]
pub(crate) mod transfer_tests;
