//! Local console as a byte stream.
//!
//! A shell runs in a pseudo terminal and its bytes reach the caller the same
//! way serial bytes do: whole chunks through a swapped buffer, never a queue.
//! The crate knows nothing about terminals, rendering or configuration.

#![deny(missing_docs)]

mod chunks;
mod error;

pub use error::{PtyError, Result, SourceError};

use chunks::ByteSwap;
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
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            program: None,
            args: Vec::new(),
            columns: 80,
            rows: 24,
        }
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

/// How long the end of the program is waited for once its output is over, and
/// how often it is asked for in that time.
///
/// The output ends when the last handle of the pseudo terminal is closed, which
/// is what a program that leaves does on its way out — but the operating system
/// has its status a moment later, and a program that closed its handles and
/// stayed may never hand one over at all. So it is asked for, again and again,
/// for a second and no longer, and it is asked in the thread that read the
/// output and nowhere near the interface.
/// How long the reading waits for the window to take what it has before it
/// looks at whether it is still wanted.
///
/// It is not a timeout on the holding back: the wait is given up on and taken
/// again, and what it costs meanwhile is one thread standing still.
const HELD_BACK: Duration = Duration::from_millis(250);

const REAP_LIMIT: Duration = Duration::from_secs(1);
/// How long the reaping waits between two tries.
const REAP_STEP: Duration = Duration::from_millis(5);

/// A running local console.
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    rx: Arc<ByteSwap>,
    tx: Arc<ByteSwap>,
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
        let size = PtySize {
            rows: config.rows.max(1),
            cols: config.columns.max(2),
            pixel_width: 0,
            pixel_height: 0,
        };
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
        let running = Arc::new(AtomicBool::new(true));
        let child = Arc::new(Mutex::new(child));
        let code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

        spawn_thread("zyt-pty-read", {
            let rx = rx.clone();
            let running = running.clone();
            let child = child.clone();
            let code = code.clone();
            move || {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    // A full buffer is a window that has not taken what it
                    // already has, so nothing is read: the pipe of the pty fills
                    // behind this, and the program writing into it waits there
                    // instead of here. The wait is given up on now and then so
                    // that a thread nobody is taking from any more can see that
                    // it is not wanted.
                    if !rx.wait_for_room(HELD_BACK) && running.load(Ordering::Relaxed) {
                        continue;
                    }
                    // Only what fits is read, so a chunk never carries the
                    // buffer past its size and it is never grown to hold one.
                    let room = rx.room().min(buffer.len());
                    if room == 0 {
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
            master: pair.master,
            child,
            rx,
            tx,
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

    /// Queues bytes for the shell.
    pub fn write(&self, data: &[u8]) {
        self.tx.push(data);
    }

    /// Bytes still waiting to reach the shell.
    pub fn pending_output(&self) -> usize {
        self.tx.len()
    }

    /// Tells the shell about a new window size.
    pub fn resize(&self, columns: u16, rows: u16) -> Result<()> {
        self.master
            .resize(PtySize {
                rows: rows.max(1),
                cols: columns.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
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

    /// Ends the session.
    pub fn shutdown(&self) -> Result<()> {
        self.running.store(false, Ordering::Relaxed);
        self.tx.wake();
        let mut child = self.child.lock().map_err(|_| PtyError::Ended)?;
        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
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

fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(|_| ())
        .map_err(|source| PtyError::ThreadStart { source })
}
