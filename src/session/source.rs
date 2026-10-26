//! The source of a session: connecting it, driving its lines, and what it
//! says about itself.

use super::{ConnectionState, Session, Source, Told};
use crate::consoles::{Console, ConsoleId};
use crate::error::{AppError, Result};
use crate::screen::{ScreenCommand, TmuxCommand};
use rust_i18n::t;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_pty::{PtyConfig, PtySession};
use zyt_serial::{
    ControlLines, LineHold, LineParams, PortEvent, PortId, PortState, PortSupervisor,
    SupervisorConfig,
};

impl Session {
    /// Connects to a serial port and follows it by its identity.
    pub fn connect_serial(
        &mut self,
        target: PortId,
        path: String,
        params: LineParams,
        holds: zyt_serial::LineHolds,
    ) -> Result<()> {
        self.connect_serial_with(
            target,
            path,
            params,
            holds,
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
        let supervisor = Arc::new(PortSupervisor::spawn(
            config,
            backend,
            Some(self.link.notify()),
        )?);
        self.params = params;
        self.driver_line = params;
        self.rts_hold = holds.rts;
        self.dtr_hold = holds.dtr;
        self.link.attach(Some(supervisor.clone()));
        self.send(ScreenCommand::Wire {
            wire: Some(self.link.screen()),
            lossy: true,
        });
        self.source = Source::Serial { supervisor, path };
        self.apply_read_buffer();
        Ok(())
    }

    /// Starts a local console.
    pub fn connect_console(&mut self, console: &Console) -> Result<()> {
        self.disconnect();
        self.reset_terminal()?;
        let (columns, rows) = self.area;
        let (program, args) = console.command()?;
        let config = PtyConfig {
            program: Some(program),
            args,
            columns: columns as u16,
            rows: rows as u16,
            cell: self.cell_size(),
            environment: console.environment_pairs(),
        };
        let session = Arc::new(PtySession::spawn(&config, Some(self.link.notify()))?);
        self.link.attach(Some(session.clone()));
        self.send(ScreenCommand::Wire {
            wire: Some(self.link.screen()),
            lossy: false,
        });
        self.source = Source::Console {
            session,
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
        self.link.attach(None);
        self.send(ScreenCommand::Wire {
            wire: Some(self.link.screen()),
            lossy: false,
        });
        self.rts_hold = LineHold::default();
        self.dtr_hold = LineHold::default();
        self.held_break = false;
        self.read_hold = false;
        self.rate.clear();
        self.forget_samples();
        self.stop_transfers();
        self.terminal_progress = None;
        self.port_lost = false;
        self.source = Source::None;
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
    /// debug window calls both of them the same thing, and nothing is thrown
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

        self.send(ScreenCommand::Terminal(zyt_term::Command::ForgetOutput));
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
    pub(super) fn forget_samples(&mut self) {
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
    pub(super) fn sample_console(&mut self) {
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

        let sent = self.counters.bytes_out > self.sampled_out;
        let received = self.counters.bytes_in > self.sampled_in;
        self.sampled_out = self.counters.bytes_out;
        self.sampled_in = self.counters.bytes_in;
        let held = self.read_hold || self.held_for_selection() || self.waiting_to_be_read().1;

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

    /// Gives the area a size: the screen gives it to the grid, or under tmux
    /// asks tmux for it; the local console is told the size of its window.
    ///
    /// A size the area already has is not sent again: under tmux it would be
    /// one more command for the server, and the view says the size again
    /// whenever the mode starts or ends.
    pub fn resize(&mut self, columns: usize, rows: usize) {
        if self.area != (columns, rows) {
            self.area = (columns, rows);
            if !self.run_holds_line() {
                self.send(ScreenCommand::Resize { columns, rows });
            }
        }
        self.tell_console();
    }

    /// Tells the local console the size of its window, unless tmux or a run
    /// sets it.
    pub(super) fn tell_console(&mut self) {
        if self.size_held() {
            return;
        }
        self.cell_told = self.cell_size();
        let (columns, rows) = self.area;
        if let Source::Console { session, .. } = &self.source {
            let _ = session.resize(columns as u16, rows as u16, self.cell_told);
        }
    }

    /// Tells the console the size of the window again when a cell has changed
    /// size under it.
    ///
    /// The pixels of the window are the cells multiplied by one of them, so a
    /// font of another size is a window of another size with the same grid in
    /// it — and nothing resizes the grid, so nothing else would say so. It is
    /// asked every frame and says nothing while the two numbers stand.
    pub fn follow_cell_size(&mut self) {
        if self.cell_size() == self.cell_told {
            return;
        }
        self.tell_console();
    }

    /// Pixels of one cell, as the window size of a console carries them.
    ///
    /// The renderer tells the emulation how large a cell is and this reads it
    /// back, so the number a program is told by the kernel is the number it is
    /// told by a sequence.
    pub(super) fn cell_size(&self) -> (u16, u16) {
        let (width, height) = self.cell;
        (
            u16::try_from(width).unwrap_or(u16::MAX),
            u16::try_from(height).unwrap_or(u16::MAX),
        )
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
        self.send(ScreenCommand::Tmux(TmuxCommand::ReadBuffer(bytes)));
    }

    /// Tells the source in hand how large its buffer is.
    pub(super) fn apply_read_buffer(&self) {
        let bytes = self.read_buffer.unwrap_or(0);
        match &self.source {
            Source::Serial { supervisor, .. } => supervisor.set_read_buffer(bytes),
            Source::Console { session, .. } => session.set_read_buffer(bytes),
            Source::None => {}
        }
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

    /// Reads the control information of the source: what happened to the
    /// connection, and where the modem lines stand.
    ///
    /// It is read apart from the bytes because it has to be read on the frames
    /// the bytes are not: a port is lost once and says so once, and a reading
    /// left until later must not be what swallows it.
    ///
    /// A port that opens again after it was lost says so too: the loss went to
    /// the desktop, and nothing else there would say it is over.
    pub(super) fn poll_source(&mut self, errors: &mut Vec<AppError>) {
        let Source::Serial { supervisor, .. } = &self.source else {
            return;
        };

        while let Some(event) = supervisor.try_event() {
            match event {
                PortEvent::Lines(lines) => self.lines = lines,
                PortEvent::Lost { error } => {
                    self.screen.send(ScreenCommand::Tmux(TmuxCommand::LineLost));
                    self.port_lost = true;
                    errors.push(AppError::Port { source: error })
                }
                PortEvent::Failed { error } => {
                    self.screen.send(ScreenCommand::Tmux(TmuxCommand::LineLost));
                    errors.push(AppError::Port { source: error })
                }
                PortEvent::Opened { path } => {
                    self.screen.send(ScreenCommand::Tmux(TmuxCommand::LineBack));
                    if std::mem::take(&mut self.port_lost) {
                        self.told.push(Told {
                            message: t!("lost.back", port = path).to_string(),
                            details: Vec::new(),
                        });
                    }
                }
                PortEvent::LineChanged { params } => {
                    let port = self.port_path().unwrap_or_default().to_string();
                    self.told.push(Told {
                        message: t!("line.changed_outside", port = port, line = params.summary())
                            .to_string(),
                        details: Vec::new(),
                    });
                }
                PortEvent::Stopped => {}
            }
        }
        let status = supervisor.status();
        self.lines = status.lines;
        self.driver_line = status.params;
    }

    /// Whether a selection is what stops the source being read now.
    pub fn held_for_selection(&self) -> bool {
        self.view.said.held_for_selection && !matches!(self.source, Source::None)
    }

    /// Gives the screen a fresh terminal, dropping the page, the scrollback,
    /// the selection and every mode the previous device set.
    pub fn reset_terminal(&mut self) -> Result<()> {
        self.send(ScreenCommand::Reset);
        self.title = None;
        self.lines = ControlLines::default();
        Ok(())
    }
}
