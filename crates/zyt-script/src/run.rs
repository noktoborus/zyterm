//! One script, running on a thread of its own.
//!
//! It stands where a transfer program stood: the application feeds it what the
//! device said and takes what it answers, and nothing of the device reaches
//! the terminal meanwhile. What a transfer program was stopped with — a signal
//! to its process group — does not work on a thread, so stopping one is in two
//! steps, and the person decides when the second one comes:
//!
//! ```text
//! cancel ─► the flag: every blocking call of the host refuses
//!        ─► the processes it started, by group
//!        ─► cleanup(), which may still write to the line, for as long as it takes
//! force  ─► the hook of the interpreter raises, and keeps raising
//!        ─► the thread is left, its line shut, its sockets closed
//! ```
//!
//! No clock walks from the first to the second. Whichever way it ends, the
//! sockets it opened are closed with it: after its `cleanup` when the thread
//! comes back, at once when it is forced.

use crate::detached::Outcome;
use crate::engine::{Engine, Start};
use crate::error::{Result, ScriptError};
use crate::host::Line;
use crate::interrupt::Cancel;
use crate::registry::ProcessRegistry;
use crate::runner::JobRunner;
use crate::sockets::SocketRegistry;
use crate::target::Direction;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// How long the stopping of the children of a script is waited for.
const KILLING: Duration = Duration::from_millis(200);

/// A script that is running, or has just stopped.
pub struct ScriptRun {
    name: String,
    direction: Direction,
    line: Arc<dyn Line>,
    cancel: Arc<Cancel>,
    processes: Arc<Mutex<ProcessRegistry>>,
    jobs: Arc<Mutex<JobRunner>>,
    sockets: Arc<Mutex<SocketRegistry>>,
    ended: Arc<AtomicBool>,
    outcome: Arc<Mutex<Option<Outcome>>>,
    failure: Arc<Mutex<Option<ScriptError>>>,
    handle: Option<JoinHandle<()>>,
    abandoned: bool,
}

impl ScriptRun {
    /// Loads the script and starts it on a thread of its own.
    ///
    /// Loading happens here rather than on that thread, so a script that is
    /// not Lua, or says nothing of itself, is an error the caller is handed
    /// instead of one that arrives a frame later.
    pub fn start(start: Start, direction: Direction) -> Result<Self> {
        let name = start.id.clone();
        let line = start.line.clone();
        let cancel = start.cancel.clone();

        let engine = Engine::load(start)?;
        if !engine.manifest().offers(direction) {
            return Err(ScriptError::NoDirection {
                name,
                direction: direction.name().to_string(),
            });
        }

        let processes = engine.processes();
        let jobs = engine.jobs();
        let sockets = engine.sockets();
        let ended = Arc::new(AtomicBool::new(false));
        let outcome: Arc<Mutex<Option<Outcome>>> = Arc::new(Mutex::new(None));
        let failure: Arc<Mutex<Option<ScriptError>>> = Arc::new(Mutex::new(None));

        let handle = std::thread::Builder::new()
            .name(format!("zyt-script-{name}"))
            .spawn({
                let ended = ended.clone();
                let outcome = outcome.clone();
                let failure = failure.clone();
                let processes = processes.clone();
                let jobs = jobs.clone();
                let sockets = sockets.clone();
                let line = line.clone();
                move || {
                    let ran = engine.call(direction);
                    let (ended_as, failed) = match ran {
                        Ok(()) => (Outcome::Done, None),
                        Err(ScriptError::Cancelled) => {
                            if let Err(error) = engine.cleanup() {
                                log::debug!("the script could not tidy up: {error}");
                            }
                            (Outcome::Cancelled, None)
                        }
                        Err(error) => {
                            log::warn!("the script failed: {error}");
                            (Outcome::Failed(None), Some(error))
                        }
                    };

                    if let Ok(mut registry) = processes.lock() {
                        registry.kill_all();
                    }
                    if let Ok(mut running) = jobs.lock() {
                        running.cancel_all();
                    }
                    // After `cleanup`, which may still have something to say
                    // to whoever is connected.
                    if let Ok(mut open) = sockets.lock() {
                        open.close_all();
                    }
                    line.close();
                    settle(&outcome, &failure, ended_as, failed);
                    ended.store(true, Ordering::Release);
                }
            })
            .map_err(|source| ScriptError::ThreadStart { source })?;

        Ok(Self {
            name,
            direction,
            line,
            cancel,
            processes,
            jobs,
            sockets,
            ended,
            outcome,
            failure,
            handle: Some(handle),
            abandoned: false,
        })
    }

    /// What the script is called.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Which way it was asked to go.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// True once it is over, one way or another.
    pub fn is_finished(&self) -> bool {
        self.abandoned || self.ended.load(Ordering::Acquire)
    }

    /// How it ended, or nothing while it runs.
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome.lock().ok().and_then(|slot| *slot)
    }

    /// Why it failed, once and then no more.
    pub fn take_failure(&self) -> Option<ScriptError> {
        self.failure.lock().ok().and_then(|mut slot| slot.take())
    }

    /// The jobs it started beside the line.
    pub fn jobs(&self) -> Arc<Mutex<JobRunner>> {
        self.jobs.clone()
    }

    /// Asks it to stop, and stops everything it started.
    ///
    /// This answers at once: the flag is set and the children are killed. The
    /// script goes on to its `cleanup` and ends when it ends; [`Self::force`]
    /// is what ends it when it does not.
    pub fn cancel(&mut self) {
        self.cancel.ask();
        self.stop_children();
    }

    /// Says whether the run is over. Called once a frame.
    pub fn tick(&mut self) -> bool {
        self.is_finished()
    }

    /// Ends the run at once, whatever the script is doing.
    ///
    /// The hook of the interpreter raises from here on, so a script running in
    /// the interpreter stops at its next instruction; one held inside a call of
    /// the host is left there. Either way the run is over for the caller: the
    /// line is shut, the programs are killed and the sockets closed now
    /// ([`Self::abandon`]).
    pub fn force(&mut self) {
        self.cancel.harden();
        self.abandon();
    }

    /// Gives up on the thread and shuts the line it was writing to.
    ///
    /// The thread is left where it is, because there is no way to take a
    /// thread off a call it is inside; the line is shut instead, so nothing it
    /// does from here reaches the session. Its interpreter is never dropped,
    /// which is the price of a script that answered neither the flag nor the
    /// hook.
    ///
    /// The run is said to be cancelled before the line is shut: a script that
    /// waits on the line wakes on its shutting and fails, and that failure is
    /// what the giving up caused, not what the run came to.
    pub fn abandon(&mut self) {
        if self.abandoned {
            return;
        }
        self.abandoned = true;
        settle(
            &self.outcome,
            &self.failure,
            Outcome::Cancelled,
            Some(ScriptError::Abandoned {
                name: self.name.clone(),
            }),
        );
        self.line.close();
        self.stop_children();
        self.close_sockets();
        let _ = self.handle.take();
        log::warn!("the script {} was abandoned", self.name);
    }

    /// Stops every program the script started, without waiting on its thread.
    ///
    /// The list may be held by the script at this moment — it is starting
    /// something, or reaping what ended — so it is asked for rather than
    /// waited on, and asked again for as long as that is worth doing.
    fn stop_children(&mut self) {
        let deadline = std::time::Instant::now() + KILLING;
        loop {
            match self.processes.try_lock() {
                Ok(mut registry) => {
                    registry.kill_all();
                    break;
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        log::warn!("the processes of {} are still held", self.name);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => break,
            }
        }

        if let Ok(mut jobs) = self.jobs.try_lock() {
            jobs.cancel_all();
        }
    }
}

impl ScriptRun {
    /// Closes every socket the script opened, without waiting on its thread:
    /// a call of the script holds the list for a moment at most, and a port
    /// left listening by a run that is over is a port the next run cannot
    /// take.
    fn close_sockets(&mut self) {
        let deadline = std::time::Instant::now() + KILLING;
        loop {
            match self.sockets.try_lock() {
                Ok(mut open) => {
                    open.close_all();
                    break;
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        log::warn!("the sockets of {} are still held", self.name);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => break,
            }
        }
    }
}

impl std::fmt::Debug for ScriptRun {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ScriptRun")
            .field("name", &self.name)
            .field("direction", &self.direction)
            .field("finished", &self.is_finished())
            .field("outcome", &self.outcome())
            .finish()
    }
}

impl Drop for ScriptRun {
    /// A run that is dropped stops: the window is not there to tick it any
    /// more, so the flag, the children and the line are all settled at once.
    fn drop(&mut self) {
        if !self.is_finished() {
            self.cancel.harden();
            self.stop_children();
            self.close_sockets();
            self.line.close();
        }
    }
}

/// Says what the run came to, and why it failed, unless that was said already.
///
/// Both the thread and [`ScriptRun::abandon`] say it, and the first one stands:
/// the thread of a run given up on still ends, later, in whatever the giving up
/// left it to. The failure is kept under the lock of the outcome, so the two
/// are always of the same saying.
fn settle(
    outcome: &Mutex<Option<Outcome>>,
    failure: &Mutex<Option<ScriptError>>,
    ended_as: Outcome,
    failed: Option<ScriptError>,
) {
    let Ok(mut said) = outcome.lock() else {
        return;
    };
    if said.is_some() {
        return;
    }
    if let Some(error) = failed
        && let Ok(mut slot) = failure.lock()
    {
        *slot = Some(error);
    }
    *said = Some(ended_as);
}
