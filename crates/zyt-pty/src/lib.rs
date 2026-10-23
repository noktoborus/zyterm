//! Local console as a byte stream.
//!
//! A shell runs in a pseudo terminal and its bytes reach the caller the same
//! way serial bytes do: whole chunks through a swapped buffer, never a queue.
//! The crate knows nothing about terminals, rendering or configuration.

#![deny(missing_docs)]

mod chunks;
mod error;
mod hold;

pub use error::{PtyError, Result, SourceError};

use chunks::ByteSwap;
use hold::ReadHold;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Callback used to wake the user interface when bytes arrive.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// Settings of a local console session.
///
/// Where the console starts is not among them: it starts where this process
/// stands, and a caller that wants it elsewhere stands elsewhere itself.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    /// Program to run; the platform shell when empty.
    pub program: Option<String>,
    /// Arguments of the program.
    pub args: Vec<String>,
    /// Initial number of columns.
    pub columns: u16,
    /// Initial number of rows.
    pub rows: u16,
    /// Pixels of one cell: width, then height.
    ///
    /// The window size of a terminal carries pixels beside the columns and the
    /// rows, and a program that draws — a picture, a plot, a thumbnail — reads
    /// them from the kernel (`TIOCGWINSZ`) rather than asking the terminal in a
    /// sequence it may get no answer to. Nought for a caller that does not
    /// know, which is what a terminal that has not been drawn yet answers.
    pub cell: (u16, u16),
    /// Variables the program is started with, over the environment of this
    /// process.
    ///
    /// They are pairs and not a map because the order is the caller's: a name
    /// written twice is answered by the last of them, the way a shell answers
    /// it.
    pub environment: Vec<(String, String)>,
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            program: None,
            args: Vec::new(),
            columns: 80,
            rows: 24,
            cell: (0, 0),
            environment: Vec::new(),
        }
    }
}

/// The size of the window as the kernel is told it: the cells, and the pixels
/// they come to.
///
/// The pixels are the cells multiplied by one of them, because that is the
/// question a program asking has: how large the area it may draw in is. A cell
/// of nought pixels is a caller that does not know, and nought is what the
/// field carries then — which is what every terminal says before anybody has
/// measured a font.
fn window_size(columns: u16, rows: u16, cell: (u16, u16)) -> PtySize {
    let cols = columns.max(2);
    let rows = rows.max(1);
    PtySize {
        rows,
        cols,
        pixel_width: cols.saturating_mul(cell.0),
        pixel_height: rows.saturating_mul(cell.1),
    }
}

/// Shell of the platform, taken from the environment when possible.
pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// How long the reading waits for the window to take what it has before it
/// looks at whether it is still wanted.
///
/// It is not a timeout on the holding back: the wait is given up on and taken
/// again, and what it costs meanwhile is one thread standing still.
const HELD_BACK: Duration = Duration::from_millis(250);

/// How long the end of the program is waited for once its output is over, and
/// how often it is asked for in that time.
///
/// The output ends when the last handle of the pseudo terminal is closed, which
/// is what a program that leaves does on its way out — but the operating system
/// has its status a moment later, and a program that closed its handles and
/// stayed may never hand one over at all. So it is asked for, again and again,
/// for a second and no longer, and it is asked in the thread that read the
/// output and nowhere near the interface.
const REAP_LIMIT: Duration = Duration::from_secs(1);
/// How long the reaping waits between two tries.
const REAP_STEP: Duration = Duration::from_millis(5);

/// A running local console.
pub struct PtySession {
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    rx: Arc<ByteSwap>,
    tx: Arc<ByteSwap>,
    hold: Arc<ReadHold>,
    running: Arc<AtomicBool>,
    code: Arc<Mutex<Option<i32>>>,
    program: String,
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PtySession")
            .field("program", &self.program)
            .field("running", &self.running.load(Ordering::Relaxed))
            .finish()
    }
}

impl PtySession {
    /// Starts a shell in a new pseudo terminal.
    pub fn spawn(config: &PtyConfig, notify: Option<Notify>) -> Result<Self> {
        let size = window_size(config.columns, config.rows, config.cell);
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|source| PtyError::Open {
                source: source.into(),
            })?;

        let program = config.program.clone().unwrap_or_else(default_shell);
        let mut command = CommandBuilder::new(&program);
        command.args(&config.args);
        if let Ok(directory) = std::env::current_dir() {
            command.cwd(directory);
        }
        command.env("TERM", "xterm-256color");
        for (name, value) in &config.environment {
            command.env(name, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|source| PtyError::Spawn {
                program: program.clone(),
                source: source.into(),
            })?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|source| PtyError::Pipes {
                source: source.into(),
            })?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|source| PtyError::Pipes {
                source: source.into(),
            })?;

        let rx = ByteSwap::with_size(256 * 1024);
        let tx = ByteSwap::with_size(0);
        let hold = Arc::new(ReadHold::new());
        let running = Arc::new(AtomicBool::new(true));
        let child = Arc::new(Mutex::new(child));
        let code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let ready = Readiness::of(&*pair.master);
        let master = Arc::new(Mutex::new(pair.master));

        spawn_thread("zyt-pty-read", {
            let master = master.clone();
            let rx = rx.clone();
            let hold = hold.clone();
            let running = running.clone();
            let child = child.clone();
            let code = code.clone();
            move || {
                let _master = master;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    // The reading held on purpose is the same thing as a full
                    // buffer and is waited on the same way.
                    if hold.wait(HELD_BACK) {
                        continue;
                    }
                    // A full buffer is a window that has not taken what it
                    // already has, so nothing is read: the pipe of the pty fills
                    // behind this, and the program writing into it waits there
                    // instead of here. The wait is given up on now and then so
                    // that a thread nobody is taking from any more can see that
                    // it is not wanted.
                    if !rx.wait_for_room(HELD_BACK) {
                        continue;
                    }
                    // Only what fits is read, so a chunk never carries the
                    // buffer past its size and it is never grown to hold one.
                    let room = rx.room().min(buffer.len());
                    if room == 0 || !ready.wait(HELD_BACK) {
                        continue;
                    }
                    match reader.read(&mut buffer[..room]) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            rx.push(&buffer[..count]);
                            if let Some(notify) = &notify {
                                notify();
                            }
                        }
                    }
                }
                if let Ok(reaped) = reap(&child) {
                    *code.lock().unwrap_or_else(|error| error.into_inner()) = reaped;
                }
                running.store(false, Ordering::Relaxed);
                if let Some(notify) = &notify {
                    notify();
                }
            }
        })?;

        spawn_thread("zyt-pty-write", {
            let tx = tx.clone();
            let running = running.clone();
            move || {
                let mut buffer = Vec::with_capacity(8 * 1024);
                while running.load(Ordering::Relaxed) {
                    tx.take_into_wait(&mut buffer, Duration::from_millis(250));
                    if buffer.is_empty() {
                        continue;
                    }
                    if writer.write_all(&buffer).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
            }
        })?;

        Ok(Self {
            master,
            child,
            rx,
            tx,
            hold,
            running,
            code,
            program,
        })
    }

    /// Program running in the session.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Moves received bytes into `spare`, which is cleared first.
    pub fn read_into(&self, spare: &mut Vec<u8>) {
        self.rx.take_into(spare);
    }

    /// Gives the buffer of the reading another size, or zero for one with no
    /// limit.
    ///
    /// The buffer is allocated at that size and the pty is read no faster than
    /// it empties: a full one stops the reading rather than growing.
    ///
    /// Nothing is thrown away: the reading stops, the pipe of the pty fills,
    /// and the program that is writing into it waits there. A program pouring
    /// out more than the window takes is therefore slowed to the pace of the
    /// window rather than kept in its memory.
    pub fn set_read_buffer(&self, bytes: usize) {
        self.rx.set_size(bytes);
    }

    /// Bytes waiting to be read into the caller, and whether the shell is being
    /// held back because they were not taken.
    pub fn read_buffer(&self) -> (usize, bool) {
        (self.rx.len(), self.rx.is_full())
    }

    /// Stops the pty being read, or lets the reading begin again.
    ///
    /// It is the state a full read buffer reaches by itself, asked for on
    /// purpose: nothing is thrown away, the pipe of the pseudo terminal fills,
    /// and the program waits at its next write. What it is for is a program
    /// pouring out text somebody wants to read a page of.
    ///
    /// The read already waiting on the pty is answered once more. The flag is
    /// looked at between two reads and not inside one, and a read of a pty waits
    /// for as long as the program is quiet, so a hold cannot take that one back.
    /// Everything after it waits.
    ///
    /// A session that has ended reads no more whatever this says.
    pub fn set_read_hold(&self, held: bool) {
        self.hold.set(held);
    }

    /// Whether the reading is held from this side.
    pub fn read_hold(&self) -> bool {
        self.hold.held()
    }

    /// Queues bytes for the shell.
    pub fn write(&self, data: &[u8]) {
        self.tx.push(data);
    }

    /// Bytes still waiting to reach the shell.
    pub fn pending_output(&self) -> usize {
        self.tx.len()
    }

    /// Tells the shell about a new window size.
    pub fn resize(&self, columns: u16, rows: u16, cell: (u16, u16)) -> Result<()> {
        self.master
            .lock()
            .map_err(|_| PtyError::Ended)?
            .resize(window_size(columns, rows, cell))
            .map_err(|source| PtyError::Resize {
                columns,
                rows,
                source: source.into(),
            })
    }

    /// True while the shell is running or output is still buffered.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) || !self.rx.is_empty()
    }

    /// Code the program ended with, once it has ended.
    ///
    /// Nothing is waited for here: the thread that read the output waited for
    /// the status when the output stopped, and this is what it found. A program
    /// that is still running, and one that never handed a status over, answer
    /// `None`. The code is what tells an ordinary end from a failure, so it is
    /// asked for after [`Self::is_running`] has turned false.
    pub fn exit_code(&self) -> Option<i32> {
        *self.code.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Ends the session and waits for nothing.
    ///
    /// The program and its process group are killed. The thread that read the
    /// output reaps the program, so no caller waits for it to be gone.
    pub fn shutdown(&self) -> Result<()> {
        self.running.store(false, Ordering::Relaxed);
        self.hold.set(false);
        self.tx.wake();
        let mut child = self.child.lock().map_err(|_| PtyError::Ended)?;
        if matches!(child.try_wait(), Ok(None)) {
            kill(child.as_mut());
        }
        Ok(())
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Kills the program and its process group.
///
/// The program leads a session of its own, so its process id is the id of its
/// group, and the group holds what it started in the foreground.
#[cfg(unix)]
fn kill(child: &mut (dyn portable_pty::Child + Send + Sync)) {
    use rustix::process::{Pid, Signal, kill_process_group};

    let Some(group) = child.process_id().and_then(|id| Pid::from_raw(id as i32)) else {
        return;
    };
    let _ = kill_process_group(group, Signal::KILL);
}

/// Kills the program. It returns at once: the system ends the process.
#[cfg(not(unix))]
fn kill(child: &mut (dyn portable_pty::Child + Send + Sync)) {
    let _ = child.kill();
}

/// Asks the operating system for the status of the program until it has one,
/// and gives up after [`REAP_LIMIT`].
///
/// The lock on the child is taken and given back around every try, so a shutdown
/// that wants to end the program never waits on this for longer than one try.
fn reap(child: &Mutex<Box<dyn portable_pty::Child + Send + Sync>>) -> Result<Option<i32>> {
    let deadline = std::time::Instant::now() + REAP_LIMIT;
    loop {
        {
            let mut child = child.lock().map_err(|_| PtyError::Ended)?;
            if let Ok(Some(status)) = child.try_wait() {
                return Ok(Some(status.exit_code() as i32));
            }
        }
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(REAP_STEP);
    }
}

/// Whether the pty has something to read, asked before every read.
///
/// A read of a pty ends only when the last handle of its other side is closed.
/// A program that leaves a daemon holding that side — a server of tmux keeps
/// the terminal of its client — leaves a read that never ends, and the thread
/// in it never sees that the session is over. So the reading waits here, a
/// while at a time, and looks at the session between two waits.
///
/// The descriptor is the master's, which the reading thread holds as long as it
/// runs, so the number stays the master's while it is asked.
struct Readiness {
    #[cfg(unix)]
    fd: Option<std::os::unix::io::RawFd>,
}

impl Readiness {
    #[cfg(unix)]
    fn of(master: &dyn portable_pty::MasterPty) -> Self {
        Self {
            fd: master.as_raw_fd(),
        }
    }

    #[cfg(not(unix))]
    fn of(_master: &dyn portable_pty::MasterPty) -> Self {
        Self {}
    }

    /// True when a read would not wait, or when the wait cannot be asked and
    /// the read is all there is.
    #[cfg(unix)]
    fn wait(&self, timeout: Duration) -> bool {
        let Some(fd) = self.fd else {
            return true;
        };
        let mut wanted = [filedescriptor::pollfd {
            fd,
            events: filedescriptor::POLLIN,
            revents: 0,
        }];
        filedescriptor::poll(&mut wanted, Some(timeout)).map_or(true, |ready| ready > 0)
    }

    #[cfg(not(unix))]
    fn wait(&self, _timeout: Duration) -> bool {
        true
    }
}

fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(|_| ())
        .map_err(|source| PtyError::ThreadStart { source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window size carries the pixels the cells come to, and nought while
    /// nobody has measured a cell.
    #[test]
    fn the_window_size_carries_the_pixels_of_its_cells() {
        let told = window_size(80, 24, (8, 17));
        assert_eq!((told.cols, told.rows), (80, 24));
        assert_eq!((told.pixel_width, told.pixel_height), (640, 408));

        let unknown = window_size(80, 24, (0, 0));
        assert_eq!(
            (unknown.pixel_width, unknown.pixel_height),
            (0, 0),
            "a caller that does not know says nothing"
        );

        let least = window_size(0, 0, (8, 17));
        assert_eq!(
            (least.cols, least.rows),
            (2, 1),
            "a grid is never narrower than two columns or shorter than one row"
        );
    }
}
