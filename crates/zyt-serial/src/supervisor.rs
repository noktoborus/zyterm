//! Port worker thread and reconnect state machine.

use crate::backend::{PortBackend, PortHandle};
use crate::enumerate::PortId;
use crate::error::{PortError, Result};
use crate::lines::{ControlLines, LineHold};
use crate::params::LineParams;
use crate::rxbuf::ByteSwap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Connection state of the supervised port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    /// No device; the worker scans for the target.
    Disconnected,
    /// The device was found and is being opened.
    Opening,
    /// The port is open and data flows.
    Connected,
}

/// Control plane message from the worker to the user interface.
#[derive(Debug)]
pub enum PortEvent {
    /// The port was opened at this path.
    Opened {
        /// Operating system path the device appeared at.
        path: String,
    },
    /// The connection was lost; the worker keeps scanning.
    Lost {
        /// Reason the connection ended.
        error: PortError,
    },
    /// An operation failed without ending the connection.
    Failed {
        /// Reason of the failure.
        error: PortError,
    },
    /// New modem line levels.
    Lines(ControlLines),
    /// The worker stopped.
    Stopped,
}

/// Command from the user interface to the worker.
#[derive(Debug)]
enum PortCommand {
    SetParams(LineParams),
    SetTarget(PortId),
    SetRts(LineHold),
    SetDtr(LineHold),
    SetBreak(bool),
    SetLinesInterval(Duration),
    Reopen,
    Stop,
}

/// How long a read held back waits for the window to take what it has before
/// the worker looks at whatever else it has to do.
const HELD_BACK: Duration = Duration::from_millis(100);

/// How long the worker waits between two snapshots of the modem lines, before
/// a caller says otherwise.
///
/// Four readings a second is what a line changing under somebody's hand looks
/// like, and each of them is one call into the driver.
pub const DEFAULT_LINES_INTERVAL: Duration = Duration::from_millis(250);

/// The shortest and the longest wait between two snapshots a caller may ask
/// for.
///
/// Nought is not an interval: a worker polling the lines with no wait at all
/// spends the whole thread on one ioctl, and the reading of the port waits
/// behind it. The far end of the range is a line looked at once a minute, which
/// is a port nobody is watching.
pub const LINES_INTERVAL_RANGE: std::ops::RangeInclusive<Duration> =
    Duration::from_millis(10)..=Duration::from_secs(60);

/// Tuning of the worker.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Device to follow.
    pub target: PortId,
    /// Line parameters to apply on open.
    pub params: LineParams,
    /// What to do with Request To Send, on open and from then on.
    pub rts: LineHold,
    /// What to do with Data Terminal Ready, the same way.
    pub dtr: LineHold,
    /// Whether the transmission line is held in the break condition, on open
    /// and from then on.
    pub held_break: bool,
    /// Delay between scans while disconnected.
    pub scan_interval: Duration,
    /// Delay between modem line snapshots.
    pub lines_interval: Duration,
    /// Size of one read from the driver.
    pub read_chunk: usize,
    /// Initial capacity of the shared buffers.
    pub buffer_capacity: usize,
}

impl SupervisorConfig {
    /// Default tuning for the given target.
    pub fn new(target: PortId) -> Self {
        Self {
            target,
            params: LineParams::default(),
            rts: LineHold::Auto,
            dtr: LineHold::Auto,
            held_break: false,
            scan_interval: Duration::from_millis(500),
            lines_interval: DEFAULT_LINES_INTERVAL,
            read_chunk: 64 * 1024,
            buffer_capacity: 256 * 1024,
        }
    }
}

/// Observable state of the port, kept up to date by the worker.
#[derive(Debug, Clone)]
pub struct PortStatus {
    /// Connection state.
    pub state: PortState,
    /// Path the port is open at, when connected.
    pub path: Option<String>,
    /// Last known modem line levels.
    pub lines: ControlLines,
    /// Whether the transmission line is held in the break condition from here.
    pub held_break: bool,
    /// Bytes the driver has taken off the line and nobody has read yet.
    pub input_queue: usize,
    /// Bytes the driver has taken from this side and not put on the line yet.
    pub output_queue: usize,
    /// Line parameters currently in effect.
    pub params: LineParams,
    /// Bytes waiting to go out: our own buffer plus the queue of the driver.
    pub pending_output: usize,
}

/// Callback used to wake the user interface when bytes or events arrive.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// Handle of the port worker thread.
pub struct PortSupervisor {
    rx: Arc<ByteSwap>,
    tx: Arc<ByteSwap>,
    status: Arc<Mutex<PortStatus>>,
    events: Mutex<Receiver<PortEvent>>,
    commands: Sender<PortCommand>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl PortSupervisor {
    /// Starts the worker thread for the given backend.
    pub fn spawn(
        config: SupervisorConfig,
        backend: Box<dyn PortBackend>,
        notify: Option<Notify>,
    ) -> Result<Self> {
        let rx = ByteSwap::with_size(config.buffer_capacity);
        let tx = ByteSwap::with_size(0);
        let status = Arc::new(Mutex::new(PortStatus {
            state: PortState::Disconnected,
            path: None,
            lines: ControlLines::default(),
            held_break: config.held_break,
            input_queue: 0,
            output_queue: 0,
            params: config.params,
            pending_output: 0,
        }));
        let (event_tx, event_rx) = channel();
        let (command_tx, command_rx) = channel();

        let worker_state = Worker {
            config,
            backend,
            rx: rx.clone(),
            tx: tx.clone(),
            status: status.clone(),
            events: event_tx,
            commands: command_rx,
            handle: None,
            pending: std::collections::VecDeque::new(),
            notify,
        };

        let worker = std::thread::Builder::new()
            .name("zyt-serial".to_string())
            .spawn(move || worker_state.run())
            .map_err(|source| PortError::WorkerStart { source })?;

        Ok(Self {
            rx,
            tx,
            status,
            events: Mutex::new(event_rx),
            commands: command_tx,
            worker: Some(worker),
        })
    }

    /// Moves all received bytes into `spare`, which is cleared first.
    pub fn read_into(&self, spare: &mut Vec<u8>) {
        self.rx.take_into(spare);
    }

    /// Gives the buffer of the reading another size, or zero for one with no
    /// limit.
    ///
    /// The buffer is allocated at that size and the port is read no faster
    /// than it empties: a full one stops the reading rather than growing.
    ///
    /// Nothing is thrown away here: the bytes wait in the driver instead, and a
    /// line with flow control tells the device to wait with them. A line
    /// without one says nothing to the device, so what the driver cannot hold
    /// is lost there — which is what a line without flow control does with
    /// anything the other side does not keep up with.
    pub fn set_read_buffer(&self, bytes: usize) {
        self.rx.set_size(bytes);
    }

    /// Bytes waiting to be read into the caller, and whether the port is being
    /// held back because they were not taken.
    pub fn read_buffer(&self) -> (usize, bool) {
        (self.rx.len(), self.rx.is_full())
    }

    /// Queues bytes for transmission.
    pub fn write(&self, data: &[u8]) {
        self.tx.push(data);
    }

    /// Current status snapshot.
    pub fn status(&self) -> PortStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// Next control plane event, if any.
    pub fn try_event(&self) -> Option<PortEvent> {
        let events = self.events.lock().ok()?;
        events.try_recv().ok()
    }

    /// Applies new line parameters.
    pub fn set_params(&self, params: LineParams) -> Result<()> {
        self.send(PortCommand::SetParams(params))
    }

    /// Follows another device.
    pub fn set_target(&self, target: PortId) -> Result<()> {
        self.send(PortCommand::SetTarget(target))
    }

    /// Says what to do with the Request To Send line, now and on every open
    /// from here on.
    pub fn set_rts(&self, hold: LineHold) -> Result<()> {
        self.send(PortCommand::SetRts(hold))
    }

    /// Says what to do with the Data Terminal Ready line, the same way.
    pub fn set_dtr(&self, hold: LineHold) -> Result<()> {
        self.send(PortCommand::SetDtr(hold))
    }

    /// Holds the transmission line in the break condition, or lets it go.
    ///
    /// The state is kept and put back on a port the worker opened again, the
    /// way a hold on a modem line is: a break asked for is a break asked of the
    /// device and not of the handle that happened to be open.
    pub fn set_break(&self, held: bool) -> Result<()> {
        self.send(PortCommand::SetBreak(held))
    }

    /// Says how long the worker waits between two snapshots of the modem
    /// lines.
    ///
    /// The wait is held inside [`LINES_INTERVAL_RANGE`]: a worker polling with
    /// no wait at all reads nothing off the line while it does it.
    pub fn set_lines_interval(&self, interval: Duration) -> Result<()> {
        self.send(PortCommand::SetLinesInterval(interval.clamp(
            *LINES_INTERVAL_RANGE.start(),
            *LINES_INTERVAL_RANGE.end(),
        )))
    }

    /// Closes and opens the port again.
    pub fn reopen(&self) -> Result<()> {
        self.send(PortCommand::Reopen)
    }

    fn send(&self, command: PortCommand) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|_| PortError::WorkerStopped)
    }
}

impl Drop for PortSupervisor {
    fn drop(&mut self) {
        let _ = self.commands.send(PortCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Worker {
    config: SupervisorConfig,
    backend: Box<dyn PortBackend>,
    rx: Arc<ByteSwap>,
    tx: Arc<ByteSwap>,
    status: Arc<Mutex<PortStatus>>,
    events: Sender<PortEvent>,
    commands: Receiver<PortCommand>,
    handle: Option<Box<dyn PortHandle>>,
    pending: std::collections::VecDeque<PortCommand>,
    notify: Option<Notify>,
}

impl Worker {
    fn run(mut self) {
        let mut read_buffer = vec![0u8; self.config.read_chunk];
        let mut write_buffer: Vec<u8> = Vec::with_capacity(8 * 1024);
        let mut fresh: Vec<u8> = Vec::with_capacity(8 * 1024);
        let mut last_lines = Instant::now() - self.config.lines_interval;

        loop {
            match self.drain_commands() {
                Flow::Stop => break,
                Flow::Reopen => {
                    self.handle = None;
                    self.set_state(PortState::Disconnected, None);
                }
                Flow::Continue => {}
            }

            let Some(mut port) = self.handle.take() else {
                if !self.try_open() {
                    self.wait(self.config.scan_interval);
                }
                continue;
            };

            if !self.pump(
                port.as_mut(),
                &mut read_buffer,
                &mut write_buffer,
                &mut fresh,
            ) {
                drop(port);
                self.set_state(PortState::Disconnected, None);
                continue;
            }

            if last_lines.elapsed() >= self.config.lines_interval {
                last_lines = Instant::now();
                if !self.poll_lines(port.as_mut()) {
                    drop(port);
                    self.set_state(PortState::Disconnected, None);
                    continue;
                }
            }
            self.handle = Some(port);
        }

        self.handle = None;
        self.set_state(PortState::Disconnected, None);
        let _ = self.events.send(PortEvent::Stopped);
        self.wake();
    }

    /// Opens the target when it is present. Returns true when a port was opened.
    fn try_open(&mut self) -> bool {
        let ports = match self.backend.list() {
            Ok(ports) => ports,
            Err(error) => {
                self.emit(PortEvent::Failed { error });
                return false;
            }
        };
        let Some(found) = ports
            .into_iter()
            .find(|port| port.matches(&self.config.target))
        else {
            return false;
        };

        self.set_state(PortState::Opening, None);
        match self.backend.open(&found.path, &self.config.params) {
            Ok(mut port) => {
                self.hold_lines(port.as_mut());
                self.handle = Some(port);
                self.set_state(PortState::Connected, Some(found.path.clone()));
                self.emit(PortEvent::Opened { path: found.path });
                true
            }
            Err(error) => {
                self.set_state(PortState::Disconnected, None);
                self.emit(PortEvent::Failed { error });
                false
            }
        }
    }

    /// One read/write cycle. Returns false when the connection ended.
    fn pump(
        &self,
        port: &mut dyn PortHandle,
        read_buffer: &mut [u8],
        write_buffer: &mut Vec<u8>,
        fresh: &mut Vec<u8>,
    ) -> bool {
        self.tx.take_into(fresh);
        write_buffer.append(fresh);

        if !write_buffer.is_empty() {
            match port.write_some(write_buffer) {
                Ok(0) => {}
                Ok(count) => {
                    write_buffer.drain(..count);
                }
                Err(error) => return self.report_connection_error(error),
            }
        }

        let taken = match port.pending_write() {
            Ok(pending) => pending,
            Err(error) => return self.report_connection_error(error),
        };
        let waiting = match port.pending_read() {
            Ok(waiting) => waiting,
            Err(error) => return self.report_connection_error(error),
        };
        self.set_queues(waiting, taken, taken + write_buffer.len() + self.tx.len());

        // A full buffer is a window that has not taken what it already has, so
        // the port is not read: the bytes wait in the driver, and a line with
        // flow control tells the device to wait with them. The turn is given up
        // rather than waited out, so the commands of this worker and the modem
        // lines are still answered while it stands.
        if !self.rx.wait_for_room(HELD_BACK) {
            return true;
        }

        // Only what fits is read, so a chunk never carries the buffer past its
        // size and the buffer is never grown to hold one.
        let room = self.rx.room().min(read_buffer.len());
        if room == 0 {
            return true;
        }

        match port.read(&mut read_buffer[..room]) {
            Ok(0) => true,
            Ok(count) => {
                self.rx.push(&read_buffer[..count]);
                self.wake();
                true
            }
            Err(error) => self.report_connection_error(error),
        }
    }

    /// Puts the forced levels back on the lines of a port that has just
    /// opened.
    ///
    /// A driver raises both of them on open, so a hold that is not reapplied
    /// here would last exactly as long as the connection, and a device that was
    /// unplugged would come back with the line the user holds down standing up.
    /// A failure is reported and the port is kept: a line that cannot be driven
    /// is not a connection that ended.
    fn hold_lines(&self, port: &mut dyn PortHandle) {
        if let Some(level) = self.config.rts.level()
            && let Err(error) = port.set_rts(level)
        {
            self.emit(PortEvent::Failed { error });
        }
        if let Some(level) = self.config.dtr.level()
            && let Err(error) = port.set_dtr(level)
        {
            self.emit(PortEvent::Failed { error });
        }
        if self.config.held_break
            && let Err(error) = port.set_break(true)
        {
            self.emit(PortEvent::Failed { error });
        }
    }

    /// Reports an error of an open port. Returns false when the connection ended.
    fn report_connection_error(&self, error: PortError) -> bool {
        if error.is_fatal_for_connection() {
            self.emit(PortEvent::Lost { error });
            false
        } else {
            self.emit(PortEvent::Failed { error });
            true
        }
    }

    /// Refreshes the modem lines. Returns false when the connection ended.
    fn poll_lines(&self, port: &mut dyn PortHandle) -> bool {
        match port.lines() {
            Ok(lines) => {
                let changed = self
                    .status
                    .lock()
                    .map(|mut status| {
                        let changed = status.lines != lines;
                        status.lines = lines;
                        changed
                    })
                    .unwrap_or(false);
                if changed {
                    self.emit(PortEvent::Lines(lines));
                }
                true
            }
            Err(error) => self.report_connection_error(error),
        }
    }

    fn drain_commands(&mut self) -> Flow {
        let mut port = self.handle.take();
        let flow = self.drain_commands_with(&mut port);
        self.handle = port;
        flow
    }

    fn drain_commands_with(&mut self, port: &mut Option<Box<dyn PortHandle>>) -> Flow {
        loop {
            let command = match self.pending.pop_front() {
                Some(command) => Ok(command),
                None => self.commands.try_recv(),
            };
            match command {
                Ok(PortCommand::Stop) => return Flow::Stop,
                Ok(PortCommand::Reopen) => return Flow::Reopen,
                Ok(PortCommand::SetTarget(target)) => {
                    self.config.target = target;
                    return Flow::Reopen;
                }
                Ok(PortCommand::SetParams(params)) => {
                    self.config.params = params;
                    if let Ok(mut status) = self.status.lock() {
                        status.params = params;
                    }
                    if let Some(handle) = port.as_deref_mut()
                        && let Err(error) = handle.set_params(&params)
                    {
                        self.emit(PortEvent::Failed { error });
                        return Flow::Reopen;
                    }
                }
                Ok(PortCommand::SetRts(hold)) => {
                    self.config.rts = hold;
                    if let Some(level) = hold.level()
                        && let Some(handle) = port.as_deref_mut()
                        && let Err(error) = handle.set_rts(level)
                    {
                        self.emit(PortEvent::Failed { error });
                    }
                }
                Ok(PortCommand::SetDtr(hold)) => {
                    self.config.dtr = hold;
                    if let Some(level) = hold.level()
                        && let Some(handle) = port.as_deref_mut()
                        && let Err(error) = handle.set_dtr(level)
                    {
                        self.emit(PortEvent::Failed { error });
                    }
                }
                Ok(PortCommand::SetBreak(held)) => {
                    self.config.held_break = held;
                    if let Ok(mut status) = self.status.lock() {
                        status.held_break = held;
                    }
                    if let Some(handle) = port.as_deref_mut()
                        && let Err(error) = handle.set_break(held)
                    {
                        self.emit(PortEvent::Failed { error });
                    }
                }
                Ok(PortCommand::SetLinesInterval(interval)) => {
                    self.config.lines_interval = interval;
                }
                Err(TryRecvError::Empty) => return Flow::Continue,
                Err(TryRecvError::Disconnected) => return Flow::Stop,
            }
        }
    }

    /// Waits for the given time but returns early on a command, which is then
    /// kept for the next drain cycle.
    fn wait(&mut self, duration: Duration) {
        match self.commands.recv_timeout(duration) {
            Ok(command) => self.pending.push_back(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => self.pending.push_back(PortCommand::Stop),
        }
    }

    /// Publishes what the driver holds in each direction and how much is still
    /// waiting for the line, and wakes the interface when the last of those
    /// changed between something and nothing.
    ///
    /// The two queues of the driver are read but never woken for: they are
    /// numbers a window of numbers shows, and a frame asked for every byte that
    /// moved through a queue would be a frame per byte of the line.
    fn set_queues(&self, input: usize, output: usize, pending: usize) {
        let changed = self
            .status
            .lock()
            .map(|mut status| {
                let changed = (status.pending_output == 0) != (pending == 0);
                status.input_queue = input;
                status.output_queue = output;
                status.pending_output = pending;
                changed
            })
            .unwrap_or(false);
        if changed {
            self.wake();
        }
    }

    fn set_state(&self, state: PortState, path: Option<String>) {
        if let Ok(mut status) = self.status.lock() {
            status.state = state;
            status.path = path;
            if state != PortState::Connected {
                status.lines = ControlLines::default();
                status.pending_output = 0;
                status.input_queue = 0;
                status.output_queue = 0;
            }
        }
    }

    fn emit(&self, event: PortEvent) {
        let _ = self.events.send(event);
        self.wake();
    }

    fn wake(&self) {
        if let Some(notify) = &self.notify {
            notify();
        }
    }
}

enum Flow {
    Continue,
    Reopen,
    Stop,
}
