//! A command line that runs beside the line instead of on it.
//!
//! A program that needs no terminal of its own is handed no line and no keys:
//! its standard input is closed, and everything it says on either output
//! channel goes into a file of its own, which outlives the program so whoever
//! started it can read it afterwards. Nothing here opens that file again — the
//! caller is told where it stands and decides what to do with it.

use crate::error::{Result, XferError};
use crate::process::{shell_command, spawn_thread, stop_group};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// How a program that ran beside the line ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It ended by itself and said nothing went wrong.
    Done,
    /// It ended by itself with this exit code, or none when a signal ended it.
    Failed(Option<i32>),
    /// It was stopped from here.
    Cancelled,
}

impl Outcome {
    /// True when the program did what it was started for.
    pub fn is_done(self) -> bool {
        matches!(self, Self::Done)
    }
}

/// One program running beside the line, with its output in a file.
#[derive(Debug)]
pub struct DetachedJob {
    child: Arc<Mutex<Child>>,
    log: PathBuf,
    outcome: Arc<Mutex<Option<Outcome>>>,
    cancelled: Arc<AtomicBool>,
}

impl DetachedJob {
    /// Starts one resolved command line with its output in `log`.
    ///
    /// The line runs through the shell of the platform, so it may use shell
    /// syntax. Both output channels are joined into the one file, in the order
    /// the program wrote them, because a line of a diagnostic belongs beside
    /// the output it explains.
    ///
    /// `notify` is called when the program ends, so a caller that sleeps
    /// between frames knows to wake up.
    pub fn start(
        line: &str,
        log: &Path,
        notify: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self> {
        if line.trim().is_empty() {
            return Err(XferError::EmptyCommand {
                profile: String::new(),
            });
        }

        let file = std::fs::File::create(log).map_err(|source| XferError::Log {
            path: log.to_path_buf(),
            source,
        })?;
        let errors = file.try_clone().map_err(|source| XferError::Log {
            path: log.to_path_buf(),
            source,
        })?;

        let mut command = shell_command(line);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::from(errors));

        let child = command.spawn().map_err(|source| XferError::Spawn {
            command: line.to_string(),
            source,
        })?;

        let child = Arc::new(Mutex::new(child));
        let outcome: Arc<Mutex<Option<Outcome>>> = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));

        spawn_thread("zyt-xfer-detached", {
            let child = child.clone();
            let outcome = outcome.clone();
            let cancelled = cancelled.clone();
            move || {
                let status = wait_for(&child);
                let ended = if cancelled.load(Ordering::Relaxed) {
                    Outcome::Cancelled
                } else {
                    match status {
                        Some(0) => Outcome::Done,
                        code => Outcome::Failed(code),
                    }
                };
                if let Ok(mut slot) = outcome.lock() {
                    *slot = Some(ended);
                }
                if let Some(notify) = notify {
                    notify();
                }
            }
        })?;

        Ok(Self {
            child,
            log: log.to_path_buf(),
            outcome,
            cancelled,
        })
    }

    /// File the program writes everything it says into.
    pub fn log(&self) -> &Path {
        &self.log
    }

    /// How it ended, or nothing while it is still running.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome.lock().ok().and_then(|slot| *slot)
    }

    /// True once the program is gone.
    pub fn is_finished(&self) -> bool {
        self.outcome().is_some()
    }

    /// True once it was asked to stop, whether or not it has.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Stops the program and everything it started.
    ///
    /// The line runs through a shell, so killing that shell alone would leave
    /// what it started behind: the whole group is signalled and then killed,
    /// and the call waits for it to be gone.
    pub fn cancel(&self) -> Result<()> {
        if self.is_finished() {
            return Ok(());
        }
        self.cancelled.store(true, Ordering::Relaxed);

        let mut child = self.child.lock().map_err(|_| XferError::Finished)?;
        stop_group(child.id());
        match child.kill() {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(source) => return Err(XferError::Kill { source }),
        }
        let _ = child.wait();
        Ok(())
    }
}

impl Drop for DetachedJob {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

/// Waits for the child and answers with its exit code.
///
/// The handle is shared with whoever may cancel the job, so it is held for the
/// one question and let go again between two of them: a lock held across the
/// wait would keep a cancel waiting for the very program it wants to stop.
fn wait_for(child: &Arc<Mutex<Child>>) -> Option<i32> {
    loop {
        let status = {
            let Ok(mut guard) = child.lock() else {
                return None;
            };
            guard.try_wait()
        };
        match status {
            Ok(Some(status)) => return status.code(),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}
