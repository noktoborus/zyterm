//! One script, running on a thread of its own.
//!
//! It stands where a transfer program stood: the application feeds it what the
//! device said and takes what it answers, once a frame, and nothing of the
//! device reaches the terminal meanwhile. What a transfer program was stopped
//! with — a signal to its process group — does not work on a thread, so
//! stopping one is a ladder:
//!
//! ```text
//! cancel ─► the flag, every blocking call of the host refuses
//!        ─► the processes it started, by group, newest first
//!        ─► cleanup(), with a budget of CLEANUP_BUDGET
//!        ─► the hook of the interpreter raises, and keeps raising
//!        ─► past ABANDON_AFTER the thread is left and its line is shut
//! ```
//!
//! [`ScriptRun::tick`] is what walks it, and the caller calls that once a
//! frame. The last rung is the one that matters for the window: whatever the
//! script is doing, the session is free of it inside a known time.

use crate::detached::Outcome;
use crate::engine::{Engine, Start};
use crate::error::{Result, ScriptError};
use crate::host::Line;
use crate::interrupt::{ABANDON_AFTER, Cancel};
use crate::registry::ProcessRegistry;
use crate::runner::JobRunner;
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
    ended: Arc<AtomicBool>,
    outcome: Arc<Mutex<Option<Outcome>>>,
    failure: Arc<Mutex<Option<ScriptError>>>,
    handle: Option<JoinHandle<()>>,
    hardened: bool,
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
                move || {
                    let ran = engine.call(direction);
                    let ended_as = match ran {
                        Ok(()) => Outcome::Done,
                        Err(ScriptError::Cancelled) => {
                            if let Err(error) = engine.cleanup() {
                                log::debug!("the script could not tidy up: {error}");
                            }
                            Outcome::Cancelled
                        }
                        Err(error) => {
                            log::warn!("the script failed: {error}");
                            if let Ok(mut slot) = failure.lock() {
                                *slot = Some(error);
                            }
                            Outcome::Failed(None)
                        }
                    };

                    if let Ok(mut registry) = processes.lock() {
                        registry.kill_all();
                    }
                    if let Ok(mut running) = jobs.lock() {
                        running.cancel_all();
                    }
                    if let Ok(mut slot) = outcome.lock() {
                        *slot = Some(ended_as);
                    }
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
            ended,
            outcome,
            failure,
            handle: Some(handle),
            hardened: false,
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
    /// This answers at once: the flag is set, the children are killed, and the
    /// rest of the ladder is walked by [`ScriptRun::tick`]. A caller that
    /// waited here would be the window waiting on the very script it is trying
    /// to be rid of.
    pub fn cancel(&mut self) {
        self.cancel.ask();
        self.stop_children();
    }

    /// Walks the ladder of stopping, and says whether the run is over.
    ///
    /// Called once a frame. It does nothing at all while the script was not
    /// asked to stop.
    pub fn tick(&mut self) -> bool {
        if self.is_finished() {
            return true;
        }
        if !self.cancel.asked() {
            return false;
        }

        if !self.hardened && self.cancel.cleanup_spent() {
            self.cancel.harden();
            self.hardened = true;
        }

        if let Some(since) = self.cancel.since()
            && since.elapsed() >= ABANDON_AFTER
        {
            self.abandon();
            return true;
        }

        false
    }

    /// Gives up on the thread and shuts the line it was writing to.
    ///
    /// The thread is left where it is, because there is no way to take a
    /// thread off a call it is inside; the line is shut instead, so nothing it
    /// does from here reaches the session. Its interpreter is never dropped,
    /// which is the price of a script that answered neither the flag nor the
    /// hook.
    pub fn abandon(&mut self) {
        if self.abandoned {
            return;
        }
        self.abandoned = true;
        self.line.close();
        self.stop_children();

        if let Ok(mut slot) = self.outcome.lock()
            && slot.is_none()
        {
            *slot = Some(Outcome::Cancelled);
        }
        if let Ok(mut slot) = self.failure.lock()
            && slot.is_none()
        {
            *slot = Some(ScriptError::Abandoned {
                name: self.name.clone(),
            });
        }
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
            self.line.close();
        }
    }
}
