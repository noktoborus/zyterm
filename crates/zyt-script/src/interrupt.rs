//! Stopping a script that is running.
//!
//! Two mechanisms, because neither covers the whole of it. A flag is what
//! every blocking call of the host asks about, so a script waiting for the
//! device is let go at once and its `cleanup` may still send a `ctrl+c` or the
//! cancel of a protocol. A hook of the interpreter is what catches a script
//! that is not waiting for anything — `while true do end` asks the host
//! nothing, and no flag would ever be read.
//!
//! No clock moves from one to the other. The flag is the ask; the hook raises
//! once the person forces the end ([`Cancel::harden`]), and from then on it
//! raises on every trigger, because a script could otherwise catch the one
//! error with `pcall` and carry on.

use crate::error::{Result, ScriptError};
use mlua::{HookTriggers, Lua, VmState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How many instructions of the interpreter run between two looks at the flag.
///
/// Low enough that a tight loop is caught in no time a person can notice, high
/// enough that the hook costs nothing worth measuring on a script that is
/// mostly waiting for a line.
const HOOK_EVERY: u32 = 10_000;

/// The ask to stop, shared by everything that might have to answer it.
#[derive(Debug, Default)]
pub struct Cancel {
    asked: AtomicBool,
    hard: AtomicBool,
    woken: Condvar,
    sleeping: Mutex<()>,
}

impl Cancel {
    /// An ask that has not been made.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Asks the script to stop, and wakes it if it is sleeping.
    pub fn ask(&self) {
        self.asked.store(true, Ordering::Release);
        self.woken.notify_all();
    }

    /// Asks a second time: the end is forced.
    ///
    /// From here the hook of the interpreter raises on every trigger, so a
    /// script cannot swallow the refusal and carry on.
    pub fn harden(&self) {
        self.hard.store(true, Ordering::Release);
        self.ask();
    }

    /// True once stopping was asked for.
    pub fn asked(&self) -> bool {
        self.asked.load(Ordering::Acquire)
    }

    /// True once the end is forced.
    pub fn is_hard(&self) -> bool {
        self.hard.load(Ordering::Acquire)
    }

    /// An error when stopping was asked for, and nothing when it was not.
    ///
    /// Every blocking call of the host asks this, which is what makes a script
    /// waiting for a silent device answer the button at once.
    pub fn check(&self) -> Result<()> {
        if self.asked() {
            return Err(ScriptError::Cancelled);
        }
        Ok(())
    }

    /// Waits out the duration, or less when stopping is asked for meanwhile.
    pub fn sleep(&self, how_long: Duration) -> Result<()> {
        let deadline = Instant::now() + how_long;
        let mut guard = self.sleeping.lock().map_err(|_| ScriptError::Cancelled)?;
        while !self.asked() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            let (next, _) = self
                .woken
                .wait_timeout(guard, left)
                .map_err(|_| ScriptError::Cancelled)?;
            guard = next;
        }
        Err(ScriptError::Cancelled)
    }
}

/// Installs the hook that stops a script which asks the host nothing.
///
/// The hook raises on every trigger once the end is forced, and it is never
/// taken off again: a `pcall` catches the first refusal, and the next
/// instruction raises the same one, so the script cannot get past it.
pub fn install(lua: &Lua, cancel: &Arc<Cancel>) -> Result<()> {
    let cancel = cancel.clone();
    lua.set_hook(
        HookTriggers::new()
            .every_nth_instruction(HOOK_EVERY)
            .on_calls()
            .on_returns(),
        move |_, _| {
            if cancel.is_hard() {
                return Err(mlua::Error::external(ScriptError::Cancelled));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|source| ScriptError::Runtime {
        name: String::from("the interrupt hook"),
        said: source.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocking_call_is_refused_once_stopping_was_asked_for() {
        let cancel = Cancel::new();
        assert!(cancel.check().is_ok());
        cancel.ask();
        assert!(matches!(cancel.check(), Err(ScriptError::Cancelled)));
    }

    #[test]
    fn a_sleep_ends_early_when_stopping_is_asked_for() {
        let cancel = Cancel::new();
        let asking = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            asking.ask();
        });

        let started = Instant::now();
        let outcome = cancel.sleep(Duration::from_secs(30));
        assert!(matches!(outcome, Err(ScriptError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(5), "it waited out");
    }

    #[test]
    fn a_sleep_nobody_interrupts_waits_it_out() {
        let cancel = Cancel::new();
        let started = Instant::now();
        cancel.sleep(Duration::from_millis(40)).expect("it waits");
        assert!(started.elapsed() >= Duration::from_millis(30));
    }

    #[test]
    fn the_hook_stops_a_loop_that_asks_the_host_nothing() {
        let cancel = Cancel::new();
        let lua = Lua::new();
        install(&lua, &cancel).expect("the hook is installed");

        let stopping = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            stopping.harden();
        });

        let error = lua
            .load("while true do end")
            .exec()
            .expect_err("the loop is stopped");
        assert!(format!("{error}").contains("cancelled"), "{error}");
    }

    #[test]
    fn a_script_cannot_swallow_the_refusal_with_pcall() {
        let cancel = Cancel::new();
        let lua = Lua::new();
        install(&lua, &cancel).expect("the hook is installed");
        cancel.harden();

        let error = lua
            .load("local ok, said = pcall(function() while true do end end) while true do end")
            .exec()
            .expect_err("it cannot get past the hook");
        assert!(format!("{error}").contains("cancelled"), "{error}");
    }
}
