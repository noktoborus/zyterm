//! Runs an external transfer program and moves bytes between its pipes and the
//! caller, which owns the line.

use crate::chunks::ByteSwap;
use crate::error::{Result, XferError};
use crate::process::{shell_command, spawn_thread, stop_group};
use crate::profile::Direction;
use std::io::{Read, Write};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Progress report of a running transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    /// One line the program printed on its diagnostic channel.
    Log(String),
    /// The program ended with the given exit code.
    Finished {
        /// Exit code, `None` when the process was stopped by a signal.
        code: Option<i32>,
    },
}

/// A running external transfer.
///
/// The job owns the child process only. Line bytes are handed over by the
/// caller: [`TransferJob::feed`] passes what the line delivered, and
/// [`TransferJob::take_output`] collects what has to be written to the line.
pub struct TransferJob {
    child: Arc<Mutex<Child>>,
    to_line: Arc<ByteSwap>,
    to_child: Arc<ByteSwap>,
    events: Receiver<TransferEvent>,
    finished: Arc<AtomicBool>,
    drained: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    direction: Direction,
}

impl std::fmt::Debug for TransferJob {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransferJob")
            .field("direction", &self.direction)
            .field("finished", &self.finished.load(Ordering::Relaxed))
            .field("cancelled", &self.cancelled.load(Ordering::Relaxed))
            .finish()
    }
}

impl TransferJob {
    /// Starts one resolved command line.
    ///
    /// The line runs through the shell of the platform, so it may use shell
    /// syntax such as `cd /tmp && rb -vv`. Placeholders are resolved by the
    /// caller with [`crate::CommandLine::resolve`].
    ///
    /// The diagnostic channel is a pipe and its lines arrive as
    /// [`TransferEvent::Log`]: a transfer program says how far along it is
    /// there, and the caller shows those lines where they belong.
    pub fn start(line: &str, direction: Direction) -> Result<Self> {
        if line.trim().is_empty() {
            return Err(XferError::EmptyCommand {
                profile: String::new(),
            });
        }
        let line = line.to_string();
        let mut command = shell_command(&line);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|source| XferError::Spawn {
            command: line.clone(),
            source,
        })?;

        let mut stdin = child.stdin.take().ok_or(XferError::MissingPipes)?;
        let mut stdout = child.stdout.take().ok_or(XferError::MissingPipes)?;
        let stderr = child.stderr.take();

        let to_line = ByteSwap::with_capacity(64 * 1024);
        let to_child = ByteSwap::with_capacity(64 * 1024);
        let finished = Arc::new(AtomicBool::new(false));
        let drained = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (event_tx, event_rx) = channel();

        spawn_thread("zyt-xfer-out", {
            let to_line = to_line.clone();
            let drained = drained.clone();
            move || {
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    match stdout.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => to_line.push(&buffer[..count]),
                    }
                }
                drained.store(true, Ordering::Relaxed);
            }
        })?;

        spawn_thread("zyt-xfer-in", {
            let to_child = to_child.clone();
            let finished = finished.clone();
            move || {
                let mut buffer = Vec::with_capacity(64 * 1024);
                while !finished.load(Ordering::Relaxed) {
                    to_child.take_into_wait(&mut buffer, Duration::from_millis(250));
                    if buffer.is_empty() {
                        continue;
                    }
                    if stdin.write_all(&buffer).is_err() || stdin.flush().is_err() {
                        break;
                    }
                }
            }
        })?;

        if let Some(stderr) = stderr {
            spawn_thread("zyt-xfer-log", {
                let events: Sender<TransferEvent> = event_tx.clone();
                move || {
                    let mut reader = std::io::BufReader::new(stderr);
                    let mut line = Vec::new();
                    let mut byte = [0u8; 1];
                    while let Ok(1) = reader.read(&mut byte) {
                        if byte[0] == b'\n' || byte[0] == b'\r' {
                            if !line.is_empty() {
                                let text = String::from_utf8_lossy(&line).trim().to_string();
                                line.clear();
                                if !text.is_empty()
                                    && events.send(TransferEvent::Log(text)).is_err()
                                {
                                    break;
                                }
                            }
                            continue;
                        }
                        line.push(byte[0]);
                    }
                }
            })?;
        }

        let child = Arc::new(Mutex::new(child));
        spawn_thread("zyt-xfer-wait", {
            let child = child.clone();
            let finished = finished.clone();
            move || {
                loop {
                    let status = {
                        let Ok(mut guard) = child.lock() else {
                            break;
                        };
                        guard.try_wait()
                    };
                    match status {
                        Ok(Some(status)) => {
                            let _ = event_tx.send(TransferEvent::Finished {
                                code: status.code(),
                            });
                            finished.store(true, Ordering::Relaxed);
                            break;
                        }
                        Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                        Err(_) => {
                            finished.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        })?;

        Ok(Self {
            child,
            to_line,
            to_child,
            events: event_rx,
            finished,
            drained,
            cancelled,
            direction,
        })
    }

    /// Group of the transfer, which is the process id of its shell.
    pub fn group_id(&self) -> Option<u32> {
        self.child.lock().ok().map(|child| child.id())
    }

    /// Direction the job runs in.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Hands bytes received from the line to the program.
    pub fn feed(&self, data: &[u8]) {
        self.to_child.push(data);
    }

    /// Moves bytes the program produced into `spare`, which is cleared first.
    pub fn take_output(&self, spare: &mut Vec<u8>) {
        self.to_line.take_into(spare);
    }

    /// Next progress report, if any.
    pub fn try_event(&self) -> Option<TransferEvent> {
        self.events.try_recv().ok()
    }

    /// True when the program ended, its output pipe reached its end and every
    /// byte it produced was handed to the caller.
    ///
    /// The exit of the program says nothing about its output: the last chunks
    /// can still sit in the pipe, so the reader thread reads to the end of the
    /// pipe and reports that separately.
    ///
    /// The exit code is sent before this turns true, so a caller that drops the
    /// job on it finds the code waiting on the channel and nothing about the
    /// end of the program is lost with the job.
    pub fn is_finished(&self) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        self.finished.load(Ordering::Relaxed)
            && self.drained.load(Ordering::Relaxed)
            && self.to_line.is_empty()
    }

    /// Stops the program and everything it started.
    ///
    /// A transfer usually runs through a shell, and the shell starts the
    /// program that talks to the device, so killing the shell alone leaves that
    /// program on the line. The whole group of the transfer is stopped instead,
    /// and the call waits for it to be gone.
    pub fn cancel(&self) -> Result<()> {
        self.cancelled.store(true, Ordering::Relaxed);
        self.finished.store(true, Ordering::Relaxed);
        self.to_child.wake();

        let mut child = self.child.lock().map_err(|_| XferError::Finished)?;
        let pid = child.id();
        stop_group(pid);

        match child.kill() {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(source) => return Err(XferError::Kill { source }),
        }
        let _ = child.wait();
        Ok(())
    }
}

impl Drop for TransferJob {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}
