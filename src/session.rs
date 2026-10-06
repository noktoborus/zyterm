//! The active connection: a serial port or the local console, the terminal fed
//! by it, and a running file transfer.

use crate::consoles::{Console, ConsoleId};
use crate::error::{AppError, Result};
use rust_i18n::t;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_pty::{PtyConfig, PtySession};
use zyt_script::{Direction, LineChannel, Outcome, ScriptEvent, ScriptRun, Targets};
use zyt_serial::{
    ControlLines, LineHold, LineParams, PortEvent, PortId, PortState, PortSupervisor,
    SupervisorConfig,
};
use zyt_term::{RenderableContent, Terminal, TerminalConfig, TerminalEvent};

/// A moment, kept as the two readings of it the window needs.
///
/// The clock says when something happened and a monotonic reading says how long
/// ago, and neither answers for the other: a clock that was put right while the
/// window stood open would make "ten seconds ago" come out as an hour, and a
/// monotonic reading names no time of day at all.
#[derive(Debug, Clone, Copy)]
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

/// Where the bytes of the terminal come from.
pub enum Source {
    /// Nothing is connected.
    None,
    /// A serial port watched by its supervisor.
    Serial {
        /// Worker of the port.
        supervisor: PortSupervisor,
        /// Path the user selected.
        path: String,
    },
    /// A local console.
    Console {
        /// Session of the console.
        session: PtySession,
        /// The console this is, which is what it is addressed by.
        id: ConsoleId,
        /// Its name with the values it was opened with put in, which is what
        /// it is called where it is read.
        shown: String,
    },
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
fn start_fixture(session: &mut Session, name: &str, target: Targets) -> Result<()> {
    let roots = vec![fixtures()];
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
const CLEAR_SCREEN: &[u8] = b"\x1b[H\x1b[2J\x1b[3J\x1b[?25h\x1b[0 q";

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
    /// Key sent to the device once the line is empty.
    finish: Vec<u8>,
    /// Text of that key for the message.
    finish_label: Option<String>,
    /// When it started.
    started: Instant,
    /// When the script itself ended, before the line drained.
    ended: Option<Instant>,
}

/// Weight of a message the application prints into the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// Something failed.
    Error,
    /// Something worth knowing happened.
    Info,
}

/// Breaks one line into chunks that fit the given width.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let characters: Vec<char> = text.chars().collect();
    if characters.is_empty() {
        return vec![String::new()];
    }
    characters
        .chunks(width.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
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

/// Connection, terminal and transfer of one window.
pub struct Session {
    /// Terminal fed by the source.
    pub terminal: Terminal,
    /// Snapshot reused by the widget.
    pub content: RenderableContent,
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
    /// Line parameters of the serial source.
    pub params: LineParams,
    /// Title reported by the program.
    pub title: Option<String>,
    source: Source,
    script: Option<ScriptRun>,
    line: Option<std::sync::Arc<LineChannel>>,
    heard: Option<crate::scripts::Heard>,
    waiting: Option<crate::scripts::Pending>,
    run: Option<ScriptLife>,
    read_interval: Option<Duration>,
    read_buffer: Option<usize>,
    lines_interval: Duration,
    rate: crate::rate::RateMeter,
    last_read: Option<Instant>,
    last_data: Option<Moment>,
    first_data: Option<Moment>,
    last_written: Option<Moment>,
    read_held_back: bool,
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
    /// True while a bell nobody has answered yet has rung.
    pub bell: bool,
    /// Directory a console reported last.
    pub working_directory: Option<std::path::PathBuf>,
    /// Notifications a program asked for.
    pub notifications: Vec<Notification>,
    /// How far along a program said it is, until it says it is done.
    progress: Option<zyt_term::ProgressState>,
    /// True when the program of the source ended.
    pub ended: bool,
    /// Commands the shell of the source marked, waiting to be written down.
    commands: Vec<String>,
    /// True while the marks of a shell are honoured.
    pub marks_enabled: bool,
    /// True while a program may say how far along it is.
    pub progress_enabled: bool,
    /// Bytes that came from the source since it was opened.
    pub bytes_in: u64,
    /// Bytes that came from the source since this side last wrote to it.
    ///
    /// It is the size of the answer and not of the session: a write puts it back
    /// to nothing, the way it does [`Session::first_data`], so what it counts is
    /// the stretch those two moments bound. A source that talks without being
    /// asked anything counts from the last thing it was asked.
    answered: u64,
    /// Bytes that went to the source since it was opened.
    pub bytes_out: u64,
    spare: Vec<u8>,
    scrollback: usize,
    clipboard: zyt_term::ClipboardAccess,
}

impl Session {
    /// Session without a connection.
    pub fn new(scrollback: usize) -> Result<Self> {
        let terminal = Terminal::new(TerminalConfig {
            columns: 80,
            rows: 24,
            scrollback,
            clipboard: zyt_term::ClipboardAccess::default(),
        })?;
        Ok(Self {
            read_interval: None,
            read_buffer: None,
            lines_interval: zyt_serial::DEFAULT_LINES_INTERVAL,
            rate: crate::rate::RateMeter::new(),
            last_read: None,
            read_held_back: false,
            console_history: zyt_serial::LineHistory::new(),
            last_sample: None,
            sampled_in: 0,
            sampled_out: 0,
            terminal,
            content: RenderableContent::default(),
            lines: ControlLines::default(),
            rts_hold: LineHold::default(),
            dtr_hold: LineHold::default(),
            held_break: false,
            read_hold: false,
            params: LineParams::default(),
            title: None,
            source: Source::None,
            script: None,
            line: None,
            heard: None,
            waiting: None,
            run: None,
            busy_since: None,
            last_busy: None,
            last_transfer: None,
            clipboard_store: None,
            clipboard_requested: false,
            bell: false,
            working_directory: None,
            notifications: Vec::new(),
            progress: None,
            ended: false,
            commands: Vec::new(),
            marks_enabled: true,
            progress_enabled: true,
            bytes_in: 0,
            answered: 0,
            bytes_out: 0,
            last_data: None,
            first_data: None,
            last_written: None,
            spare: Vec::with_capacity(64 * 1024),
            scrollback,
            clipboard: zyt_term::ClipboardAccess::default(),
        })
    }

    /// Changes what a program may do with the clipboard.
    pub fn set_clipboard_access(&mut self, access: zyt_term::ClipboardAccess) {
        self.clipboard = access;
        self.terminal.set_clipboard_access(access);
    }

    /// Replaces the terminal with a fresh one, dropping the screen, the
    /// scrollback, the selection and every mode the previous device set.
    pub fn reset_terminal(&mut self) -> Result<()> {
        let (columns, rows) = self.terminal.size();
        let clipboard = self.clipboard;
        self.terminal = Terminal::new(TerminalConfig {
            columns,
            rows,
            scrollback: self.scrollback,
            clipboard,
        })?;
        self.content = RenderableContent::default();
        self.title = None;
        self.lines = ControlLines::default();
        Ok(())
    }

    /// Connects to a serial port and follows it by its identity.
    pub fn connect_serial(
        &mut self,
        target: PortId,
        path: String,
        params: LineParams,
        holds: zyt_serial::LineHolds,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<()> {
        self.connect_serial_with(
            target,
            path,
            params,
            holds,
            notify,
            Box::new(zyt_serial::SystemBackend::new()),
        )
    }

    /// Connects through the given port backend, which lets a test drive a
    /// device that is not there.
    pub fn connect_serial_with(
        &mut self,
        target: PortId,
        path: String,
        params: LineParams,
        holds: zyt_serial::LineHolds,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
        backend: Box<dyn zyt_serial::PortBackend>,
    ) -> Result<()> {
        self.disconnect();
        self.reset_terminal()?;
        let mut config = SupervisorConfig::new(target);
        config.params = params;
        config.rts = holds.rts;
        config.dtr = holds.dtr;
        config.lines_interval = self.lines_interval;
        config.scan_interval = std::time::Duration::from_millis(20);
        let supervisor = PortSupervisor::spawn(config, backend, notify)?;
        self.params = params;
        self.rts_hold = holds.rts;
        self.dtr_hold = holds.dtr;
        self.source = Source::Serial { supervisor, path };
        self.apply_read_buffer();
        Ok(())
    }

    /// Starts a local console.
    pub fn connect_console(
        &mut self,
        console: &Console,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<()> {
        self.disconnect();
        self.reset_terminal()?;
        let (columns, rows) = self.terminal.size();
        let (program, args) = console.command()?;
        let config = PtyConfig {
            program: Some(program),
            args,
            columns: columns as u16,
            rows: rows as u16,
        };
        self.source = Source::Console {
            session: PtySession::spawn(&config, notify)?,
            id: console.id,
            shown: console.display_name(),
        };
        self.apply_read_buffer();
        Ok(())
    }

    /// Ends the connection and any running transfer.
    ///
    /// A hold on a modem line goes with it: it was asked of one device, and the
    /// next one is opened by whoever drives it.
    pub fn disconnect(&mut self) {
        self.rts_hold = LineHold::default();
        self.dtr_hold = LineHold::default();
        self.held_break = false;
        self.read_hold = false;
        self.bytes_in = 0;
        self.answered = 0;
        self.bytes_out = 0;
        self.rate.clear();
        self.forget_samples();
        self.stop_transfers();
        self.source = Source::None;
    }

    /// Stops the script and waits for what it started to be gone.
    ///
    /// A script talks to the device, so it has to end before the source does:
    /// otherwise a program of its own keeps writing into a port that is
    /// already being closed, or stays behind holding it. The wait is bounded —
    /// a script that answers neither the flag nor the hook is given up on, and
    /// the line it was writing to is shut under it.
    pub fn stop_transfers(&mut self) {
        if let Some(pending) = self.waiting.take() {
            pending.answer(None);
        }

        if let Some(script) = self.script.as_mut() {
            script.cancel();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !script.tick() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            script.abandon();
        }

        self.script = None;
        self.run = None;
        self.line = None;
        self.heard = None;
    }

    /// True when a source is set, even while its device is absent.
    pub fn has_source(&self) -> bool {
        !matches!(self.source, Source::None)
    }

    /// True when a serial port is the source.
    pub fn is_serial(&self) -> bool {
        matches!(self.source, Source::Serial { .. })
    }

    /// Path of the serial source.
    pub fn port_path(&self) -> Option<&str> {
        match &self.source {
            Source::Serial { path, .. } => Some(path),
            _ => None,
        }
    }

    /// True while the console of the source still runs or has output left.
    pub fn console_running(&self) -> bool {
        match &self.source {
            Source::Console { session, .. } => session.is_running(),
            _ => false,
        }
    }

    /// Code the console ended with, when it has ended and reported one.
    pub fn console_exit_code(&self) -> Option<i32> {
        match &self.source {
            Source::Console { session, .. } => session.exit_code(),
            _ => None,
        }
    }

    /// The console this session is running, when it is running one.
    pub fn console_id(&self) -> Option<ConsoleId> {
        match &self.source {
            Source::Console { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// What that console is called where it is read: its name with the values
    /// it was opened with put in.
    ///
    /// It is kept as it was at the moment the console started and not worked
    /// out again when it is wanted, because what it says is what is running:
    /// the file the values came from may have been answered differently since,
    /// and a console named after an answer given after it started would be a
    /// console named after something else.
    pub fn console_shown(&self) -> Option<&str> {
        match &self.source {
            Source::Console { shown, .. } => Some(shown),
            _ => None,
        }
    }

    /// State shown in the status bar.
    pub fn state(&self) -> ConnectionState {
        match &self.source {
            Source::None => ConnectionState::Idle,
            Source::Console { session, .. } => {
                if session.is_running() {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Idle
                }
            }
            Source::Serial { supervisor, .. } => match supervisor.status().state {
                PortState::Connected => ConnectionState::Connected,
                PortState::Opening => ConnectionState::Opening,
                PortState::Disconnected => ConnectionState::Waiting,
            },
        }
    }

    /// Writes what the terminal answered straight to the device.
    ///
    /// Answers to device queries usually leave with the next cycle; a clipboard
    /// answer should not wait that long, because the program on the other end
    /// is blocked until it arrives.
    pub fn flush_terminal_output(&mut self) {
        let answer = self.terminal.take_output();
        if !answer.is_empty() && !self.script_holds_line() {
            self.write(&answer);
        }
    }

    /// Prints one progress line of a running program into the terminal, marked
    /// so it is not mistaken for device output.
    pub fn print_line(&mut self, kind: NoticeKind, text: &str) {
        let color = match kind {
            NoticeKind::Error => "\x1b[31m",
            NoticeKind::Info => "\x1b[36m",
        };
        let line = format!("{color}│\x1b[0m {text}\r\n");
        let follow = self.follows_output();
        self.terminal.feed(line.as_bytes());
        if follow {
            self.terminal.scroll_to_bottom();
        }
    }

    /// True while the view stands at the end of the output.
    ///
    /// The scrollbar at the bottom means "show me what arrives"; anywhere else
    /// means "leave me where I am", and nothing the application prints may pull
    /// the view away from what is being read.
    pub fn follows_output(&self) -> bool {
        self.terminal.display_offset() == 0
    }

    /// Prints a message of the application itself into the terminal, framed by
    /// a box so it stands out from device output while the text itself keeps
    /// the normal colors and stays readable.
    pub fn print_block(&mut self, kind: NoticeKind, lines: &[String]) {
        let (columns, _) = self.terminal.size();
        let width = columns.clamp(8, 120);
        let text_width = width.saturating_sub(4).max(4);
        let color = match kind {
            NoticeKind::Error => "\x1b[31m",
            NoticeKind::Info => "\x1b[36m",
        };
        let reset = "\x1b[0m";
        let bar: String = "─".repeat(width - 2);

        let mut out = String::from("\r\n");
        out.push_str(&format!("{color}┌{bar}┐{reset}\r\n"));
        for line in lines {
            for chunk in wrap(line, text_width) {
                let padding = " ".repeat(text_width - chunk.chars().count());
                out.push_str(&format!(
                    "{color}│{reset} {chunk}{padding} {color}│{reset}\r\n"
                ));
            }
        }
        out.push_str(&format!("{color}└{bar}┘{reset}\r\n"));
        out.push_str("\r\n");

        let follow = self.follows_output();
        self.terminal.feed(out.as_bytes());
        if follow {
            self.terminal.scroll_to_bottom();
        }
    }

    /// Sends bytes to the device.
    pub fn write(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.bytes_out += data.len() as u64;
        self.last_written = Some(Moment::now());
        self.first_data = None;
        self.answered = 0;
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.write(data),
            Source::Console { session, .. } => session.write(data),
            Source::None => {}
        }
    }

    /// Applies new line parameters to a serial source.
    pub fn set_params(&mut self, params: LineParams) -> Result<()> {
        self.params = params;
        match &self.source {
            Source::Serial { supervisor, .. } => Ok(supervisor.set_params(params)?),
            _ => Ok(()),
        }
    }

    /// Says what to do with the Request To Send line.
    ///
    /// The hold is kept here as well as in the worker, the way the line
    /// parameters are: the window draws what was asked for, and the worker is
    /// what puts it back on a port it opened again.
    pub fn set_rts(&mut self, hold: LineHold) -> Result<()> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.set_rts(hold)?;
                self.rts_hold = hold;
                Ok(())
            }
            _ => Err(AppError::NotConnected),
        }
    }

    /// Says what to do with the Data Terminal Ready line.
    pub fn set_dtr(&mut self, hold: LineHold) -> Result<()> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.set_dtr(hold)?;
                self.dtr_hold = hold;
                Ok(())
            }
            _ => Err(AppError::NotConnected),
        }
    }

    /// Holds the transmission line in the break condition, or lets it go.
    ///
    /// The state is kept here as well as in the worker, the way a hold on a
    /// modem line is: the window draws what was asked for, and the worker is
    /// what puts it back on a port it opened again.
    pub fn set_break(&mut self, held: bool) -> Result<()> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.set_break(held)?;
                self.held_break = held;
                Ok(())
            }
            _ => Err(AppError::NotConnected),
        }
    }

    /// Stops reading the source, or begins again.
    ///
    /// It is the reading of a full buffer asked for on purpose, which is why the
    /// window of numbers calls both of them the same thing, and nothing is thrown
    /// away by either. Where the bytes gather instead is what the source is: they
    /// stand in the driver of a port, and a line with flow control tells the
    /// device to wait; they stand in the pipe of a console, and the program waits
    /// at its next write.
    ///
    /// Both kinds of source hold, because the reason for it is the same for both:
    /// a program pouring out text somebody wants to read a page of.
    pub fn set_read_hold(&mut self, held: bool) -> Result<()> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.set_read_hold(held)?;
                self.read_hold = held;
                Ok(())
            }
            Source::Console { session, .. } => {
                session.set_read_hold(held);
                self.read_hold = held;
                Ok(())
            }
            Source::None => Err(AppError::NotConnected),
        }
    }

    /// Throws away everything on its way to the device that has not left yet.
    ///
    /// What reached the line is gone and cannot be recalled; what is still in a
    /// buffer of this side or of the driver is. It is the way out of a paste
    /// nobody meant to make on a line too slow to carry it.
    ///
    /// Five buffers stand on that way and all five are emptied:
    ///
    /// | buffer | emptied by |
    /// | --- | --- |
    /// | answers the terminal has not handed over | `Terminal::forget_output` |
    /// | output of a running transfer | dropping the job it belongs to |
    /// | what this side pushed for the worker | the worker, on the command below |
    /// | what the driver would not take in | the worker, on the same command |
    /// | the queue of the driver | the worker, on the same command |
    ///
    /// `Session::spare` is not among them. It is scratch that is filled and
    /// emptied inside one pass and holds nothing between two, so there is nothing
    /// in it to give up on.
    ///
    /// A running transfer is stopped rather than left running, because it is the
    /// one of the five that refills the others: a transfer whose bytes were
    /// dropped and which goes on producing more would fill the queue again on the
    /// next pass, and the action would have cleared nothing for longer than a
    /// frame. It would also be a transfer the far end waits on for ever.
    ///
    /// The answers of the terminal go with it. They are replies to what a program
    /// asked, so the program is left waiting for one that never comes — which is
    /// the cost of asking for everything, and everything is what this is for.
    pub fn discard_output(&mut self) -> Result<()> {
        if !self.is_serial() {
            return Err(AppError::NotConnected);
        }

        self.terminal.forget_output();
        self.cancel_transfer()?;

        match &self.source {
            Source::Serial { supervisor, .. } => Ok(supervisor.discard_output()?),
            _ => Err(AppError::NotConnected),
        }
    }

    /// Says how long the port worker waits between two readings of the modem
    /// lines.
    ///
    /// It is kept for the ports opened after this one as well, because a port is
    /// opened and let go of while the window stands: a wait that only reached
    /// the one that happened to be open would go away with it.
    ///
    /// The wait is held inside the range the port worker takes, here and not
    /// only in the worker: the worker holds what is sent to it, and the value
    /// kept here is what the next port is opened with.
    pub fn set_lines_interval(&mut self, interval: Duration) -> Result<()> {
        let range = zyt_serial::LINES_INTERVAL_RANGE;
        let interval = interval.clamp(*range.start(), *range.end());
        self.lines_interval = interval;
        match &self.source {
            Source::Serial { supervisor, .. } => Ok(supervisor.set_lines_interval(interval)?),
            _ => Ok(()),
        }
    }

    /// What the driver of the port holds in each direction: bytes read off the
    /// line that nobody has taken, and bytes taken from this side that are not
    /// on the line yet.
    ///
    /// Nothing at all where the source is no port: a console is a pipe and a
    /// pipe has no queue anybody can ask the size of.
    pub fn driver_queues(&self) -> Option<(usize, usize)> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                let status = supervisor.status();
                Some((status.input_queue, status.output_queue))
            }
            _ => None,
        }
    }

    /// Copies the newest samples of the signals of the source into `out`, oldest
    /// first, and answers the scale the queues of them are read against.
    ///
    /// A port is sampled by its own worker and a console by this one — see
    /// [`Self::sample_console`] — so both answer a history. A window on nothing
    /// has none, and `out` comes back empty.
    pub fn line_history(
        &self,
        count: usize,
        out: &mut Vec<zyt_serial::LineSample>,
    ) -> zyt_serial::LineScale {
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.history(count, out),
            Source::Console { .. } => {
                self.console_history.newest_into(count, out);
                self.console_history.scale()
            }
            Source::None => {
                out.clear();
                zyt_serial::LineScale::default()
            }
        }
    }

    /// Throws away the samples of the source that is ending.
    ///
    /// The history of a port lives with its worker and goes when the worker
    /// does; this is the one kept here, and a session that kept it would draw
    /// the traffic of the console before this one behind the traffic of this
    /// one.
    fn forget_samples(&mut self) {
        self.console_history.clear();
        self.last_sample = None;
        self.sampled_in = 0;
        self.sampled_out = 0;
    }

    /// Puts one sample of a console into its history for every step that has
    /// passed since the last one.
    ///
    /// A console has no lines to poll and no worker watching it, so the window is
    /// what samples it: what crossed in each direction, and whether it is being
    /// held — by a hold somebody set, or by a buffer of what it said that nobody
    /// has taken. One sample is one
    /// step of `lines_interval`, the step a port is polled at, so the span written
    /// under the tracks means the same for both sources.
    ///
    /// A window drawing no frames takes no readings, and the steps it missed are
    /// filled with what was true through them: nothing crossed, because a byte
    /// crossing is what asks for a frame. What did cross goes into the newest of
    /// the filled steps, which is the one this frame is standing in.
    ///
    /// The fill is capped at the depth of the history, so a window left alone for
    /// an hour costs one pass over it rather than one step per millisecond of the
    /// hour.
    fn sample_console(&mut self) {
        if !matches!(self.source, Source::Console { .. }) {
            return;
        }

        let now = Instant::now();
        let last = *self.last_sample.get_or_insert(now);
        let interval = self.lines_interval.max(Duration::from_millis(1));
        let steps = now.saturating_duration_since(last).as_nanos() / interval.as_nanos();
        if steps == 0 {
            return;
        }
        let steps = (steps as usize).min(zyt_serial::LINE_HISTORY_SAMPLES);

        let sent = self.bytes_out > self.sampled_out;
        let received = self.bytes_in > self.sampled_in;
        self.sampled_out = self.bytes_out;
        self.sampled_in = self.bytes_in;
        let held = self.read_hold || self.waiting_to_be_read().1;

        let sample = |sent, received| {
            zyt_serial::LineSample::new(&ControlLines::default(), false, held, sent, received)
        };
        for _ in 1..steps {
            self.console_history.push(sample(false, false));
        }
        self.console_history.push(sample(sent, received));
        self.last_sample = Some(now);
    }

    /// How long the worker waits between two readings of the modem lines, which
    /// is the span one sample of the history covers.
    ///
    /// It is the value the worker was handed and not the one the settings hold:
    /// the worker keeps what it was sent inside the range it will take, and the
    /// span a window writes under a track has to be the span the samples were
    /// actually taken at.
    pub fn lines_interval(&self) -> Duration {
        self.lines_interval
    }

    /// Closes and opens the port again with a fresh terminal.
    pub fn reopen(&mut self) -> Result<()> {
        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.reopen()?;
                self.reset_terminal()
            }
            _ => Err(AppError::NotConnected),
        }
    }

    /// Tells the local console about a new window size.
    pub fn resize(&self, columns: usize, rows: usize) {
        if let Source::Console { session, .. } = &self.source {
            let _ = session.resize(columns as u16, rows as u16);
        }
    }

    /// Starts a script on the line of this session.
    ///
    /// One script runs at a time and it owns the line while it does: nothing
    /// of the device reaches the terminal, the keyboard writes nothing, and
    /// the answers the terminal owes a program wait. A script that says it
    /// does not hold the line is given one that is already shut, so what it
    /// writes there is refused rather than quietly dropped.
    pub fn start_script(
        &mut self,
        entry: &zyt_script::Entry,
        roots: &[std::path::PathBuf],
        direction: Direction,
        target: Targets,
        variables: &std::collections::BTreeMap<String, String>,
        wake: std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Result<()> {
        if matches!(self.source, Source::None) {
            return Err(AppError::NotConnected);
        }
        if self.is_transferring() {
            return Err(AppError::TransferRunning);
        }
        self.progress = None;

        let holds_line = entry.manifest.hold_line;
        let finish = entry.manifest.finish(direction);
        let channel = LineChannel::new();
        if !holds_line {
            channel.close();
        }

        let (talker, heard) = crate::scripts::talker(wake);
        let start = zyt_script::Start::of_entry(
            entry,
            roots,
            direction,
            target,
            variables.clone(),
            zyt_script::Talking {
                line: channel.clone(),
                prompt: talker,
                cancel: zyt_script::Cancel::new(),
            },
        );

        let life = ScriptLife {
            id: entry.id.clone(),
            name: entry.manifest.name.clone(),
            holds_line,
            finish: zyt_script::finish_bytes(finish)?,
            finish_label: zyt_script::finish_label(finish),
            started: Instant::now(),
            ended: None,
        };

        let run = ScriptRun::start(start, direction)?;
        self.print_block(
            NoticeKind::Info,
            &[t!("script.started", script = entry.manifest.name.as_str()).to_string()],
        );

        self.script = Some(run);
        self.line = Some(channel);
        self.heard = Some(heard);
        self.run = Some(life);
        Ok(())
    }

    /// True while a script has the line of the session to itself.
    fn script_holds_line(&self) -> bool {
        self.script.is_some() && self.run.as_ref().is_some_and(|life| life.holds_line)
    }

    /// What the running script is called, while one runs.
    pub fn script_name(&self) -> Option<&str> {
        self.run.as_ref().map(|life| life.name.as_str())
    }

    /// The directory of the running script, which is what it is known by.
    pub fn script_id(&self) -> Option<&str> {
        self.run.as_ref().map(|life| life.id.as_str())
    }

    /// The dialog a script is waiting on, handed over once.
    pub fn take_ask(&mut self) -> Option<crate::scripts::Pending> {
        self.waiting.take()
    }

    /// Walks the ladder of stopping, so a script asked to stop stops.
    fn tick_script(&mut self) {
        let Some(script) = self.script.as_mut() else {
            return;
        };
        if !script.tick() {
            return;
        }

        let outcome = script.outcome();
        let failure = script.take_failure();
        self.script = None;
        self.report_script_end(outcome, failure);
    }

    /// Takes in everything the script said since the last frame.
    fn drain_script(&mut self) {
        let mut said: Vec<ScriptEvent> = Vec::new();
        let mut asked = None;
        if let Some(heard) = &self.heard {
            while let Ok(event) = heard.events.try_recv() {
                said.push(event);
            }
            while let Ok(pending) = heard.asks.try_recv() {
                asked = Some(pending);
            }
        }

        for event in said {
            match event {
                ScriptEvent::Note { kind, text } => {
                    let kind = match kind {
                        zyt_script::NoticeKind::Info => NoticeKind::Info,
                        zyt_script::NoticeKind::Error => NoticeKind::Error,
                    };
                    log::info!("script: {text}");
                    self.print_line(kind, &text);
                }
                ScriptEvent::Echo(bytes) => self.terminal.feed(&bytes),
                ScriptEvent::Progress(progress) => {
                    self.progress = crate::scripts::progress_of(progress);
                }
                ScriptEvent::Finished { .. } => {}
            }
        }

        if let Some(pending) = asked
            && let Some(dropped) = self.waiting.replace(pending)
        {
            dropped.answer(None);
        }
    }

    /// Bytes of the buffer a read moves the chunk of a source into: what stands
    /// in it, and what it grew to.
    ///
    /// It is empty between two frames — the bytes go into the terminal as they
    /// arrive — so what it holds is the largest chunk a source handed over at
    /// once.
    pub fn read_buffer(&self) -> (usize, usize) {
        (self.spare.len(), self.spare.capacity())
    }

    /// Bytes still waiting to leave this side: the buffer of the source and,
    /// for a serial port, the queue of the driver.
    pub fn pending_output(&self) -> usize {
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.status().pending_output,
            Source::Console { session, .. } => session.pending_output(),
            Source::None => 0,
        }
    }

    /// True while bytes of this side are still on their way out.
    pub fn line_busy(&self) -> bool {
        self.pending_output() > 0
    }

    /// How long the running transfer has been going on.
    pub fn transfer_elapsed(&self) -> Option<Duration> {
        self.run.as_ref().map(|run| run.started.elapsed())
    }

    /// How long the last finished transfer took.
    pub fn last_transfer(&self) -> Option<Duration> {
        self.last_transfer
    }

    /// Reports how the script ended and starts waiting for the line.
    fn report_script_end(
        &mut self,
        outcome: Option<Outcome>,
        failure: Option<zyt_script::ScriptError>,
    ) {
        let Some(life) = self.run.as_mut() else {
            return;
        };
        if life.ended.is_some() {
            return;
        }
        life.ended = Some(Instant::now());

        let name = life.name.clone();
        let took = crate::format::duration(life.started.elapsed());
        log::info!("the script {name} ended: {outcome:?}");

        let said = failure.map(|error| crate::error::said(&error));
        let (kind, text) = match outcome {
            Some(Outcome::Done) => (
                NoticeKind::Info,
                t!("script.finished", script = name, took = took).to_string(),
            ),
            Some(Outcome::Cancelled) => (
                NoticeKind::Error,
                t!("script.cancelled", script = name, took = took).to_string(),
            ),
            _ => (
                NoticeKind::Error,
                t!("script.failed", script = name, took = took).to_string(),
            ),
        };

        let mut lines = vec![text];
        if let Some(said) = said {
            lines.push(said);
        }
        self.print_block(kind, &lines);
    }

    /// Ends the run once the last byte left the port.
    ///
    /// The script being over is not the run being over: on a slow line the
    /// driver still holds bytes, and the key that ends a transfer is sent to a
    /// device that has had all of them.
    fn finish_when_line_is_free(&mut self) {
        let Some(life) = self.run.as_ref() else {
            return;
        };
        let Some(ended) = life.ended else {
            return;
        };
        if self.script.is_some() || self.line_busy() {
            return;
        }
        if self.line.as_ref().is_some_and(|line| line.has_output()) {
            return;
        }

        let span = life.started.elapsed();
        self.last_transfer = Some(span);
        let total = crate::format::duration(span);
        let drained = crate::format::duration(ended.elapsed());
        let finish = life.finish.clone();
        let label = life.finish_label.clone();
        self.run = None;
        self.line = None;
        self.heard = None;
        if let Some(pending) = self.waiting.take() {
            pending.answer(None);
        }
        self.progress = None;

        let mut lines = vec![t!("transfer.finished", total = total, drained = drained).to_string()];
        if let Some(label) = label {
            lines.push(t!("transfer.finish_key", key = label).to_string());
        }
        self.print_block(NoticeKind::Info, &lines);

        if !finish.is_empty() {
            self.write(&finish);
        }
    }

    /// How long the line was busy the last time, when that was worth naming.
    pub fn last_busy(&self) -> Option<Duration> {
        self.last_busy
    }

    /// Tracks how long the line stays busy, so the interface can name the last
    /// period after it ended.
    fn track_line_busy(&mut self) {
        let busy = self.pending_output() > 0;
        match (busy, self.busy_since) {
            (true, None) => self.busy_since = Some(Instant::now()),
            (false, Some(since)) => {
                let span = since.elapsed();
                self.busy_since = None;
                if span >= Duration::from_secs(1) {
                    self.last_busy = Some(span);
                }
            }
            _ => {}
        }
    }

    /// Stops the running script, and everything it started.
    ///
    /// It answers at once rather than waiting: the flag is set and the
    /// programs of the script are killed, and the rest of the ladder is walked
    /// a frame at a time. A window that waited here would be waiting on the
    /// very script it is trying to be rid of.
    pub fn cancel_transfer(&mut self) -> Result<()> {
        if let Some(pending) = self.waiting.take() {
            pending.answer(None);
        }
        if let Some(script) = self.script.as_mut() {
            script.cancel();
            self.tick_script();
            return Ok(());
        }

        self.run = None;
        self.line = None;
        self.heard = None;
        self.progress = None;
        Ok(())
    }

    /// How far along the program of the session says it is, when it says.
    ///
    /// It is dropped when a transfer starts and when one ends, so a share left
    /// over from the last one never stands beside the next.
    pub fn progress(&self) -> Option<zyt_term::ProgressState> {
        self.progress
    }

    /// Clears the screen, the scrollback and everything that stood around them.
    ///
    /// The cursor goes back to the settings of this terminal with them — shown
    /// (`DECTCEM`) and in the shape the terminal starts in (`CSI 0 SP q`) —
    /// because the cursor a program left behind is a setting of the output
    /// that is gone: a program that hid the cursor and died leaves a screen
    /// with nothing on it and no place to type, and one that asked for a beam
    /// leaves that beam in front of a shell that never asked for it.
    pub fn clear_screen(&mut self) {
        self.terminal.feed(CLEAR_SCREEN);
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
        self.progress = None;
        self.last_data = None;
        self.first_data = None;
        self.last_written = None;
        self.terminal
            .feed(b"\x1b]104\x07\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    }

    /// When bytes last came from the source.
    ///
    /// It is the moment the reading ended and not the moment it began: every
    /// read that brought something moves it, so a source still pouring out text
    /// keeps moving it and one that has fallen silent leaves it where the last
    /// byte arrived.
    pub fn last_data(&self) -> Option<Moment> {
        self.last_data
    }

    /// When this side last sent bytes to the source.
    ///
    /// Every write moves it, so what it names is the moment the input stopped:
    /// the last byte of what was typed, pasted or sent, and not the first.
    /// Together with [`Self::first_data`] and [`Self::last_data`] it says how
    /// long the other side took to begin answering and how long it went on,
    /// which is what a line that has gone quiet is read by.
    pub fn last_written(&self) -> Option<Moment> {
        self.last_written
    }

    /// Bytes that came from the source since this side last wrote to it.
    ///
    /// The size of the answer, which is what the stretch between
    /// [`Self::last_written`] and [`Self::last_data`] carried. A write puts it
    /// back to nothing, so it never counts two answers as one.
    pub fn answered(&self) -> u64 {
        self.answered
    }

    /// When the first bytes since the last write came from the source.
    ///
    /// A write puts it away, so what it names is the beginning of the answer to
    /// what was sent last: the first byte back, where [`Self::last_data`] is
    /// the last one. A source that talks without being asked anything still
    /// sets it — it is the first of this stretch of talking, and what makes it
    /// an answer is a write standing before it.
    pub fn first_data(&self) -> Option<Moment> {
        self.first_data
    }

    /// Commands the shell of the source marked since this was last asked.
    ///
    /// The session holds them for a moment and nothing longer: where a command
    /// is written down is a question about configuration files, which this
    /// knows nothing of.
    pub fn take_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.commands)
    }

    /// True while a script runs, or waits for the line to drain.
    pub fn is_transferring(&self) -> bool {
        self.script.is_some() || self.run.is_some()
    }

    /// Moves bytes between the source, the terminal and a running transfer.
    /// The shortest time between two readings of the source.
    ///
    /// Bytes that arrive between two readings wait where the reading thread put
    /// them, and the next reading takes all of them at once. `None` takes them
    /// whenever a frame asks.
    pub fn set_read_interval(&mut self, interval: Option<Duration>) {
        self.read_interval = interval;
    }

    /// The size of the buffer a source is read into, or `None` for one with no
    /// limit.
    ///
    /// It is kept for the sources opened after it as well, because a source is
    /// opened and let go of while the window stands and a size that only
    /// reached the one that happened to be open would be a setting that went
    /// away with it.
    pub fn set_read_buffer(&mut self, bytes: Option<usize>) {
        self.read_buffer = bytes;
        self.apply_read_buffer();
    }

    /// Tells the source in hand how large its buffer is.
    fn apply_read_buffer(&self) {
        let bytes = self.read_buffer.unwrap_or(0);
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.set_read_buffer(bytes),
            Source::Console { session, .. } => session.set_read_buffer(bytes),
            Source::None => {}
        }
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

    /// Bytes of the source waiting to be read, and whether the source is being
    /// held back because they have not been taken.
    pub fn waiting_to_be_read(&self) -> (usize, bool) {
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.read_buffer(),
            Source::Console { session, .. } => session.read_buffer(),
            Source::None => (0, false),
        }
    }

    /// How long is left before the source may be read again, while a reading
    /// was held back.
    ///
    /// The caller asks for a frame after this, because what arrived is waiting
    /// and nothing else is going to ask for it. A reading that was not held
    /// back asks for nothing: the next frame will take whatever has come by
    /// then, and a window nothing writes to still costs no frames at all.
    pub fn next_read(&self) -> Option<Duration> {
        if !self.read_held_back {
            return None;
        }
        let interval = self.read_interval?;
        let waited = self.last_read?.elapsed();
        (waited < interval).then(|| interval - waited)
    }

    /// Whether the source may be read now.
    ///
    /// A script that holds the line is never held back: its protocol answers
    /// a block and waits for the next one, so bytes left lying are a transfer
    /// standing still.
    fn read_due(&mut self) -> bool {
        let held_back = self
            .read_interval
            .filter(|_| !self.script_holds_line())
            .zip(self.last_read)
            .is_some_and(|(interval, last)| last.elapsed() < interval);
        self.read_held_back = held_back;
        if held_back {
            return false;
        }

        self.last_read = Some(Instant::now());
        true
    }

    pub fn pump(&mut self) -> Vec<AppError> {
        let mut errors = Vec::new();

        self.sample_console();
        self.tick_script();
        self.drain_script();
        if !self.read_due() {
            self.flush_terminal_output();
            return errors;
        }

        match &self.source {
            Source::Serial { supervisor, .. } => {
                supervisor.read_into(&mut self.spare);
                while let Some(event) = supervisor.try_event() {
                    match event {
                        PortEvent::Lines(lines) => self.lines = lines,
                        PortEvent::Lost { error } | PortEvent::Failed { error } => {
                            errors.push(AppError::Port { source: error })
                        }
                        PortEvent::Opened { .. } | PortEvent::Stopped => {}
                    }
                }
                self.lines = supervisor.status().lines;
            }
            Source::Console { session, .. } => session.read_into(&mut self.spare),
            Source::None => self.spare.clear(),
        }

        if !self.spare.is_empty() {
            self.bytes_in += self.spare.len() as u64;
            self.answered += self.spare.len() as u64;
            self.rate.push(self.spare.len());
            let arrived = Moment::now();
            self.last_data = Some(arrived);
            self.first_data.get_or_insert(arrived);
            match self.line.as_ref().filter(|_| self.script_holds_line()) {
                Some(channel) => channel.feed(&self.spare),
                None => {
                    let bytes = std::mem::take(&mut self.spare);
                    self.terminal.feed(&bytes);
                    self.spare = bytes;
                }
            }
        }

        self.flush_terminal_output();

        for event in self.terminal.take_events() {
            match event {
                TerminalEvent::Title(title) => self.title = Some(title),
                TerminalEvent::ResetTitle => self.title = None,
                TerminalEvent::Bell => self.bell = true,
                TerminalEvent::ClipboardStore(text) => self.clipboard_store = Some(text),
                TerminalEvent::ClipboardRequest => self.clipboard_requested = true,
                TerminalEvent::WorkingDirectory(path) => {
                    self.working_directory = Some(std::path::PathBuf::from(path))
                }
                TerminalEvent::Notification { kind, title, body } => {
                    self.notifications.push(Notification { kind, title, body })
                }
                TerminalEvent::Mark(_) => {}
                TerminalEvent::Command(line) => {
                    if self.marks_enabled {
                        self.commands.push(line);
                    }
                }
                // A program that says it is done is a program with no share to
                // show, so the report is put away rather than kept as a state
                // of its own: everything that shows it would have to know that
                // one of the five means nothing.
                TerminalEvent::Progress(state) => {
                    if self.progress_enabled {
                        self.progress = match state {
                            zyt_term::ProgressState::Removed => None,
                            state => Some(state),
                        };
                    }
                }
                TerminalEvent::Exit => self.ended = true,
            }
        }

        if let Some(channel) = self.line.clone() {
            channel.set_pending_output(self.pending_output());
            channel.take_output(&mut self.spare);
            if !self.spare.is_empty() {
                self.bytes_out += self.spare.len() as u64;
                match &self.source {
                    Source::Serial { supervisor, .. } => supervisor.write(&self.spare),
                    Source::Console { session, .. } => session.write(&self.spare),
                    Source::None => {}
                }
            }
        }
        self.drain_script();

        self.track_line_busy();
        self.finish_when_line_is_free();

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use zyt_term::Color;

    fn row_text(content: &zyt_term::RenderableContent, row: usize) -> String {
        (0..content.columns)
            .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
            .collect()
    }

    #[test]
    fn nothing_is_taken_from_a_source_before_the_wait_is_over() {
        let mut session = Session::new(100).expect("session is created");
        session.set_read_interval(Some(Duration::from_millis(50)));

        assert!(session.read_due(), "the first reading waits for nothing");
        assert!(
            !session.read_due(),
            "a reading inside the wait is held back"
        );
        let left = session.next_read().expect("the wait says how long is left");
        assert!(left <= Duration::from_millis(50));

        std::thread::sleep(Duration::from_millis(60));
        assert!(session.read_due(), "the wait is over");
        assert_eq!(
            session.next_read(),
            None,
            "a reading that was not held back asks for no frame"
        );
    }

    #[test]
    fn without_a_wait_a_source_is_read_whenever_a_frame_asks() {
        let mut session = Session::new(100).expect("session is created");
        session.set_read_interval(None);

        assert!(session.read_due());
        assert!(session.read_due());
        assert_eq!(session.next_read(), None);
    }

    #[test]
    fn a_notice_does_not_pull_a_reader_back_to_the_end() {
        let mut session = Session::new(200).expect("session is created");
        for line in 0..40 {
            session.terminal.feed(format!("line{line}\r\n").as_bytes());
        }

        session.terminal.scroll(5);
        assert!(!session.follows_output());
        session.print_line(NoticeKind::Info, "something happened");
        assert!(
            session.terminal.display_offset() > 0,
            "the view stays in the history"
        );

        session.terminal.scroll_to_bottom();
        assert!(session.follows_output());
        session.print_line(NoticeKind::Info, "and again");
        assert_eq!(
            session.terminal.display_offset(),
            0,
            "at the end, the end is followed"
        );
    }

    #[test]
    fn errors_are_framed_and_keep_readable_text() {
        let mut session = Session::new(100).expect("session is created");
        session.print_block(NoticeKind::Error, &["port busy".to_string()]);

        let content = session.terminal.content();
        let top = (0..content.rows)
            .find(|row| row_text(&content, *row).starts_with('\u{250c}'))
            .expect("the block has a top border");
        let text = row_text(&content, top + 1);
        assert!(text.starts_with('│'));
        assert!(text.contains("port busy"));
        assert!(row_text(&content, top + 2).starts_with('\u{2514}'));

        let border = content.cell(0, top).expect("border cell exists");
        assert_eq!(border.fg, Color::Palette(1));
        assert_eq!(border.bg, Color::Background);

        let message = content.cell(2, top + 1).expect("text cell exists");
        assert_eq!(message.fg, Color::Foreground);
        assert_eq!(message.bg, Color::Background);
    }

    #[test]
    fn long_messages_are_wrapped_inside_the_frame() {
        let mut session = Session::new(100).expect("session is created");
        session.print_block(NoticeKind::Error, &["x".repeat(200)]);

        let content = session.terminal.content();
        let body = (0..content.rows)
            .filter(|row| row_text(&content, *row).starts_with('│'))
            .count();
        assert!(body >= 3);
    }

    /// A share stands for as long as the program reports one, and the report
    /// that says it is done leaves nothing standing: the bar of the status bar
    /// and the button of the taskbar both go by whether there is a share at
    /// all, so `Removed` is no report rather than a report of its own.
    #[test]
    fn a_share_stands_until_the_program_says_it_is_done() {
        let mut session = Session::new(100).expect("session is created");

        session.terminal.feed(b"\x1b]9;4;1;40\x07");
        session.pump();
        assert_eq!(session.progress(), Some(zyt_term::ProgressState::Set(40)));

        session.terminal.feed(b"\x1b]9;4;0\x07");
        session.pump();
        assert_eq!(
            session.progress(),
            None,
            "the report that it is done takes the share away"
        );
    }

    /// Clearing the screen clears what the program said about its progress,
    /// which is `AppCommand::Clear` reaching this.
    #[test]
    fn clearing_the_screen_takes_the_share_with_it() {
        let mut session = Session::new(100).expect("session is created");

        session.terminal.feed(b"\x1b]9;4;1;70\x07");
        session.pump();
        assert_eq!(session.progress(), Some(zyt_term::ProgressState::Set(70)));

        session.clear_reports();
        assert_eq!(session.progress(), None);
    }

    /// The times of the plate are readings of output that is gone, so they go
    /// with it: what is left standing would measure a screen nobody can look
    /// at any more.
    #[test]
    fn clearing_the_screen_takes_the_times_with_it() {
        let mut session = Session::new(100).expect("session is created");

        session.write(b"a command\r");
        // Nothing is connected here, so the answer is put where a read would
        // have put it: what is under test is what clearing does with the times
        // and not the path they came by.
        session.last_data = Some(Moment::now());
        session.first_data = session.last_data;
        assert!(session.last_written().is_some(), "something was sent");

        session.clear_reports();
        assert!(session.last_written().is_none());
        assert!(session.first_data().is_none());
        assert!(session.last_data().is_none());
    }

    /// The cursor a program set goes with the screen: one that was hidden is
    /// shown again and one that was asked for as a beam is the block this
    /// terminal starts with.
    #[test]
    fn clearing_the_screen_puts_the_cursor_back() {
        let mut session = Session::new(100).expect("session is created");

        session.terminal.feed(b"\x1b[?25l\x1b[5 q");
        session.pump();
        assert_eq!(
            session.terminal.content().cursor,
            None,
            "the program hid the cursor"
        );

        session.clear_screen();
        session.pump();
        let cursor = session
            .terminal
            .content()
            .cursor
            .expect("the cursor is shown again");
        assert_eq!(cursor.shape, zyt_term::CursorShape::Block);
    }

    /// Everything a program dressed the window in goes with the screen: the
    /// title it set and the colors it painted, not only the share.
    #[test]
    fn clearing_the_screen_takes_the_title_and_the_colors_with_it() {
        let mut session = Session::new(100).expect("session is created");

        session
            .terminal
            .feed(b"\x1b]0;a title\x07\x1b]11;#102030\x07");
        session.pump();
        assert_eq!(session.title.as_deref(), Some("a title"));
        assert!(
            session.terminal.content().background.is_some(),
            "the program painted the background"
        );

        session.clear_reports();
        session.pump();
        assert_eq!(session.title, None, "the title is the program's again");
        assert_eq!(
            session.terminal.content().background,
            None,
            "the background is the palette of the settings again"
        );
    }

    #[test]
    fn progress_lines_are_marked_and_readable() {
        let mut session = Session::new(100).expect("session is created");
        session.print_line(NoticeKind::Info, "Bytes Sent: 1024/4096");

        let content = session.terminal.content();
        let row = (0..content.rows)
            .find(|row| row_text(&content, *row).starts_with('│'))
            .expect("the line is marked");
        assert!(row_text(&content, row).contains("Bytes Sent: 1024/4096"));
        assert_eq!(
            content.cell(0, row).expect("marker cell exists").fg,
            Color::Palette(6)
        );
        assert_eq!(
            content.cell(2, row).expect("text cell exists").fg,
            Color::Foreground
        );
    }

    #[test]
    fn a_second_script_is_refused_while_one_holds_the_line() {
        let mut session = Session::new(100).expect("session is created");
        session
            .connect_console(
                &crate::consoles::Console {
                    name: "test".to_string(),
                    program: "cat".to_string(),
                    ..crate::consoles::default_console()
                },
                None,
            )
            .expect("the local console starts");

        start_fixture(&mut session, "holds", Targets::none()).expect("the first one starts");
        assert!(matches!(
            start_fixture(&mut session, "holds", Targets::none()),
            Err(AppError::TransferRunning)
        ));

        session.cancel_transfer().expect("the script stops");
    }

    #[test]
    fn a_script_is_announced_by_name_and_takes_the_line() {
        let mut session = Session::new(100).expect("session is created");
        session
            .connect_console(
                &crate::consoles::Console {
                    name: "test".to_string(),
                    program: "cat".to_string(),
                    ..crate::consoles::default_console()
                },
                None,
            )
            .expect("the local console starts");

        start_fixture(&mut session, "holds", Targets::none()).expect("the script starts");

        assert!(session.is_transferring());
        assert_eq!(
            session.script_name(),
            Some("Holds the line"),
            "what is said about a run is the name a person reads"
        );

        let content = session.terminal.content();
        let text: String = (0..content.rows)
            .map(|row| row_text(&content, row))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Holds the line"), "{text}");

        session.cancel_transfer().expect("the script stops");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline && session.is_transferring() {
            session.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!session.is_transferring());
    }

    #[test]
    fn a_script_that_ended_does_not_hold_the_keyboard() {
        let mut session = Session::new(100).expect("session is created");
        session
            .connect_console(
                &crate::consoles::Console {
                    name: "test".to_string(),
                    program: "cat".to_string(),
                    ..crate::consoles::default_console()
                },
                None,
            )
            .expect("the local console starts");

        start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline && session.is_transferring() {
            session.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!session.is_transferring());
        let text: String = {
            let content = session.terminal.content();
            (0..content.rows)
                .map(|row| row_text(&content, row))
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert!(text.contains("quick"), "what the script printed: {text}");
    }

    #[test]
    fn notices_use_their_own_border_color() {
        let mut session = Session::new(100).expect("session is created");
        session.print_block(NoticeKind::Info, &["saved".to_string()]);

        let content = session.terminal.content();
        let top = (0..content.rows)
            .find(|row| row_text(&content, *row).starts_with('\u{250c}'))
            .expect("the block has a top border");
        assert_eq!(
            content.cell(0, top).expect("border cell exists").fg,
            Color::Palette(6)
        );
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use zyt_script::Targets;
    use zyt_serial::{
        ControlLines, LineParams, PortBackend, PortHandle, PortId, PortInfo, PortKind,
    };

    /// A device that records what the application writes and hands back what
    /// the test injects.
    #[derive(Default)]
    struct Device {
        written: Vec<u8>,
        incoming: Vec<u8>,
        pending_write: usize,
        held_break: bool,
    }

    #[derive(Clone)]
    struct FakeBackend {
        device: Arc<Mutex<Device>>,
    }

    impl PortBackend for FakeBackend {
        fn list(&self) -> zyt_serial::Result<Vec<PortInfo>> {
            Ok(vec![PortInfo {
                path: "/dev/fake".to_string(),
                kind: PortKind::Builtin,
                usb: None,
                accessible: true,
            }])
        }

        fn open(
            &self,
            _path: &str,
            _params: &LineParams,
        ) -> zyt_serial::Result<Box<dyn PortHandle>> {
            Ok(Box::new(FakeHandle {
                device: self.device.clone(),
            }))
        }
    }

    struct FakeHandle {
        device: Arc<Mutex<Device>>,
    }

    impl PortHandle for FakeHandle {
        fn read(&mut self, buf: &mut [u8]) -> zyt_serial::Result<usize> {
            std::thread::sleep(Duration::from_millis(5));
            let mut device = self.device.lock().unwrap();
            if device.incoming.is_empty() {
                return Ok(0);
            }
            let count = device.incoming.len().min(buf.len());
            buf[..count].copy_from_slice(&device.incoming[..count]);
            device.incoming.drain(..count);
            Ok(count)
        }

        fn write_some(&mut self, data: &[u8]) -> zyt_serial::Result<usize> {
            self.device.lock().unwrap().written.extend_from_slice(data);
            Ok(data.len())
        }

        fn lines(&mut self) -> zyt_serial::Result<ControlLines> {
            Ok(ControlLines::default())
        }

        fn set_rts(&mut self, _level: bool) -> zyt_serial::Result<()> {
            Ok(())
        }

        fn set_dtr(&mut self, _level: bool) -> zyt_serial::Result<()> {
            Ok(())
        }

        fn discard_output(&mut self) -> zyt_serial::Result<()> {
            self.device.lock().unwrap().pending_write = 0;
            Ok(())
        }

        fn line_changes(&mut self) -> Option<zyt_serial::LineEdges> {
            None
        }

        fn set_break(&mut self, held: bool) -> zyt_serial::Result<()> {
            self.device.lock().unwrap().held_break = held;
            Ok(())
        }

        fn set_params(&mut self, _params: &LineParams) -> zyt_serial::Result<()> {
            Ok(())
        }

        fn pending_write(&mut self) -> zyt_serial::Result<usize> {
            Ok(self.device.lock().unwrap().pending_write)
        }

        fn pending_read(&mut self) -> zyt_serial::Result<usize> {
            Ok(self.device.lock().unwrap().incoming.len())
        }
    }

    fn connected() -> (Session, Arc<Mutex<Device>>) {
        let device = Arc::new(Mutex::new(Device::default()));
        let mut session = Session::new(1000).expect("session is created");
        session
            .connect_serial_with(
                PortId::Path("/dev/fake".to_string()),
                "/dev/fake".to_string(),
                LineParams::default(),
                zyt_serial::LineHolds::default(),
                None,
                Box::new(FakeBackend {
                    device: device.clone(),
                }),
            )
            .expect("the fake port opens");

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && session.state() != ConnectionState::Connected {
            session.pump();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(session.state(), ConnectionState::Connected);
        (session, device)
    }

    /// A source that says more than the window takes fills the buffer and is
    /// read again the moment the window has taken what was waiting: nothing is
    /// thrown away, and the bytes wait where they were said.
    #[test]
    fn a_source_is_held_back_at_a_full_buffer_and_let_on_again() {
        let (mut session, device) = connected();
        session.set_read_interval(None);
        session.set_read_buffer(Some(4096));

        // The worker may stand inside a read that began before the size
        // changed, and that one reads into the slice it already has. The device
        // is given something to say only once such a read has ended, so what it
        // says meets the size that is being tested.
        std::thread::sleep(Duration::from_millis(50));

        // Many times what the buffer holds, so what is left over is what the
        // device still has to say while the source stands still.
        device.lock().unwrap().incoming = vec![b'x'; 64 * 1024];
        // Nothing is pumped here: what is under test is the source standing
        // still while the window takes nothing, and pumping would be the window
        // taking.
        assert!(
            waited_for(|| session.waiting_to_be_read().1),
            "the source stands still while nothing takes from it"
        );
        let (waiting, _) = session.waiting_to_be_read();
        assert_eq!(
            waiting, 4096,
            "the buffer holds what it was given room for and no more"
        );
        assert!(
            !device.lock().unwrap().incoming.is_empty(),
            "and the rest of what the device has to say is still the device's"
        );

        session.pump();
        assert!(
            waited_for(|| session.waiting_to_be_read().0 > 0),
            "the window took what was waiting, so the source is read again"
        );
        assert!(
            pump_until(&mut session, |_| device.lock().unwrap().incoming.is_empty()),
            "and everything the device had to say arrives, none of it thrown away"
        );
    }

    /// Waits for something to become true of another thread, within reason.
    fn waited_for(mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn pump_until(session: &mut Session, mut done: impl FnMut(&mut Session) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            session.pump();
            if done(session) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn terminal_text(session: &mut Session) -> String {
        let content = session.terminal.content();
        (0..content.rows)
            .map(|row| {
                (0..content.columns)
                    .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn what_a_program_of_a_script_writes_reaches_the_device() {
        let (mut session, device) = connected();
        let file = std::env::temp_dir().join(format!("zyterm-send-{}.bin", std::process::id()));
        std::fs::write(&file, b"file-payload").expect("the file is written");

        start_fixture(
            &mut session,
            "sends",
            Targets {
                kind: zyt_script::TargetKind::File,
                paths: vec![file.clone()],
            },
        )
        .expect("the script starts");

        assert!(pump_until(&mut session, |_| {
            let written = device.lock().unwrap().written.clone();
            String::from_utf8_lossy(&written).contains("file-payload")
        }));

        session.cancel_transfer().expect("the script stops");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn what_the_device_says_reaches_the_script_and_not_the_terminal() {
        let (mut session, device) = connected();

        start_fixture(&mut session, "mirrors", Targets::none()).expect("the script starts");

        device
            .lock()
            .unwrap()
            .incoming
            .extend_from_slice(b"from-the-device");

        assert!(pump_until(&mut session, |_| {
            let written = device.lock().unwrap().written.clone();
            String::from_utf8_lossy(&written).contains("from-the-device")
        }));

        assert!(
            !terminal_text(&mut session).contains("from-the-device"),
            "the terminal is not fed while a script holds the line"
        );
        session.cancel_transfer().expect("the script stops");
    }

    #[test]
    fn what_a_script_types_reaches_the_device_as_a_person_would_type_it() {
        let (mut session, device) = connected();

        start_fixture(&mut session, "types", Targets::none()).expect("the script starts");

        assert!(pump_until(&mut session, |_| {
            let written = device.lock().unwrap().written.clone();
            String::from_utf8_lossy(&written).contains("cat > 'payload.bin'\r")
        }));

        session.cancel_transfer().expect("the script stops");
    }

    #[test]
    fn the_run_ends_only_when_the_line_is_free() {
        let (mut session, device) = connected();
        device.lock().unwrap().pending_write = 8192;

        start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");

        assert!(pump_until(&mut session, |session| {
            terminal_text(session).contains("quick")
        }));
        assert!(session.is_transferring(), "the run waits for the line");

        device.lock().unwrap().pending_write = 0;
        assert!(pump_until(&mut session, |session| !session.is_transferring()));

        let text = terminal_text(&mut session);
        assert!(text.contains("Transfer finished"), "{text}");
    }

    #[test]
    fn the_finish_key_goes_out_when_the_line_is_free() {
        let (mut session, device) = connected();
        device.lock().unwrap().pending_write = 4096;

        start_fixture(&mut session, "keyed", Targets::none()).expect("the script starts");

        assert!(pump_until(&mut session, |session| {
            terminal_text(session).contains("keyed")
        }));
        assert!(
            !device.lock().unwrap().written.contains(&0x03),
            "the key waits for the line"
        );

        device.lock().unwrap().pending_write = 0;
        assert!(pump_until(&mut session, |_| {
            device.lock().unwrap().written.contains(&0x03)
        }));

        let text = terminal_text(&mut session);
        assert!(text.contains("ctrl+c"), "{text}");
    }

    #[test]
    fn a_program_can_store_and_read_the_clipboard() {
        let (mut session, device) = connected();
        session.set_clipboard_access(zyt_term::ClipboardAccess::CopyPaste);

        session.terminal.feed(b"\x1b]52;c;aGVsbG8=\x07");
        session.pump();
        assert_eq!(session.clipboard_store.take(), Some("hello".to_string()));

        session.terminal.feed(b"\x1b]52;c;?\x07");
        session.pump();
        assert!(session.clipboard_requested);

        session
            .terminal
            .answer_clipboard(Some("from the clipboard"));
        session.flush_terminal_output();
        session.clipboard_requested = false;

        assert!(pump_until(&mut session, |_| {
            let written = device.lock().unwrap().written.clone();
            let text = String::from_utf8_lossy(&written).to_string();
            text.contains("52;c;") && text.contains("ZnJvbSB0aGUgY2xpcGJvYXJk")
        }));
    }

    #[test]
    fn disconnecting_stops_a_running_script_first() {
        let (mut session, _device) = connected();

        start_fixture(&mut session, "sends", Targets::none()).expect("the script starts");
        assert!(pump_until(&mut session, |session| session.is_transferring()));

        session.disconnect();

        assert!(!session.is_transferring());
        assert!(!session.has_source());
    }

    #[test]
    fn the_marks_of_a_shell_hand_over_the_command_that_was_typed() {
        let (mut session, _device) = connected();

        session
            .terminal
            .feed(b"$ \x1b]133;B\x07ls -la\r\n\x1b]133;C\x07");
        session.pump();

        assert_eq!(session.take_commands(), vec!["ls -la".to_string()]);
        assert!(session.take_commands().is_empty());
    }

    #[test]
    fn a_bell_is_kept_until_somebody_answers_it() {
        let (mut session, _device) = connected();
        assert!(!session.bell);

        session.terminal.feed(b"ring\x07");
        session.pump();

        assert!(session.bell, "the bell rang and nobody has answered yet");

        session.bell = false;
        session.pump();
        assert!(!session.bell, "an answered bell does not ring again");
    }

    /// The bell that ends an operating system command is the terminator of that
    /// command and not a bell, so a program that sets the title rings nothing.
    #[test]
    fn the_bell_that_closes_a_sequence_is_not_a_bell() {
        let (mut session, _device) = connected();

        session.terminal.feed(b"\x1b]0;a title\x07");
        session.pump();

        assert!(!session.bell);
    }

    #[test]
    fn a_shell_whose_marks_are_refused_hands_over_nothing() {
        let (mut session, _device) = connected();
        session.marks_enabled = false;

        session
            .terminal
            .feed(b"$ \x1b]133;B\x07ls -la\r\n\x1b]133;C\x07");
        session.pump();

        assert!(session.take_commands().is_empty());
    }

    #[test]
    fn a_long_busy_line_is_remembered() {
        let (mut session, device) = connected();
        assert!(session.last_busy().is_none());

        device.lock().unwrap().pending_write = 4096;
        assert!(pump_until(&mut session, |session| session
            .busy_since
            .is_some()));
        session.busy_since = Some(Instant::now() - Duration::from_secs(3));

        device.lock().unwrap().pending_write = 0;
        assert!(pump_until(&mut session, |session| session
            .last_busy()
            .is_some()));
        assert!(session.last_busy().expect("a span") >= Duration::from_secs(3));
    }

    #[test]
    fn what_a_script_says_lands_in_the_terminal_in_a_frame() {
        let (mut session, _device) = connected();

        start_fixture(&mut session, "says", Targets::none()).expect("the script starts");

        assert!(pump_until(&mut session, |session| {
            terminal_text(session).contains("│ Bytes Sent: 1024")
        }));

        session.cancel_transfer().expect("the script stops");
    }
}
