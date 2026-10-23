//! Every process one script started, and how they are all stopped.
//!
//! A script that was cancelled must not leave a program on the line. Nothing
//! it can call starts a process on its own — the calls this crate installs are
//! the only way — so every child is in this list, and stopping the run walks
//! it. The group of each child is signalled rather than the child alone,
//! because a line that went through a shell has the shell as its child and the
//! program as the shell's.

use crate::error::{Result, ScriptError};
use crate::jobobject::JobGroup;
use crate::process::{kill_group, spawn_thread, stop_group};
use std::collections::BTreeMap;
use std::process::Child;
use std::sync::{Arc, Mutex};

/// Number of a process of this script, handed out in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId(pub u64);

/// One child and what is needed to stop it.
#[derive(Debug)]
struct Entry {
    child: Arc<Mutex<Child>>,
    pid: u32,
}

/// The processes of one script.
#[derive(Debug)]
pub struct ProcessRegistry {
    next: u64,
    group: JobGroup,
    children: BTreeMap<ProcId, Entry>,
}

impl ProcessRegistry {
    /// An empty list, with the group its children will belong to.
    pub fn new() -> Result<Self> {
        Ok(Self {
            next: 1,
            group: JobGroup::new()?,
            children: BTreeMap::new(),
        })
    }

    /// Takes a child into the list and answers with its number and handle.
    ///
    /// The handle is shared: the caller reads and writes its pipes, and this
    /// list stops it, and neither waits for the other.
    pub fn adopt(&mut self, child: Child) -> Result<(ProcId, Arc<Mutex<Child>>)> {
        self.group.adopt(&child)?;

        let id = ProcId(self.next.max(1));
        self.next = id.0 + 1;
        let pid = child.id();
        let child = Arc::new(Mutex::new(child));
        self.children.insert(
            id,
            Entry {
                child: child.clone(),
                pid,
            },
        );
        Ok((id, child))
    }

    /// Stops one of them, and everything it started.
    pub fn kill(&mut self, id: ProcId) -> Result<()> {
        let Some(entry) = self.children.remove(&id) else {
            return Ok(());
        };
        stop(&entry)
    }

    /// Kills every one of them and what they started, and waits for none.
    ///
    /// This is the way a run is let go of, so no program is given the time to
    /// leave by itself. A thread of its own reaps each of them.
    pub fn kill_all(&mut self) {
        for entry in std::mem::take(&mut self.children).into_values() {
            kill_group(entry.pid);
            reap_later(entry.child);
        }
        self.group.terminate();
    }

    /// Takes the ones that ended by themselves off the list and names them.
    pub fn reap(&mut self) -> Vec<ProcId> {
        let mut ended = Vec::new();
        self.children.retain(|id, entry| {
            let gone = entry
                .child
                .lock()
                .map(|mut child| matches!(child.try_wait(), Ok(Some(_))))
                .unwrap_or(true);
            if gone {
                ended.push(*id);
            }
            !gone
        });
        ended
    }

    /// How many are still on the list.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// True while nothing of this script is running.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        self.kill_all();
    }
}

/// Waits for a killed child in a thread of its own, so it leaves no zombie
/// and the caller waits for nothing.
fn reap_later(child: Arc<Mutex<Child>>) {
    let reaped = child
        .lock()
        .map(|mut child| !matches!(child.try_wait(), Ok(None)))
        .unwrap_or(true);
    if reaped {
        return;
    }
    let started = spawn_thread("zyt-script-reap", move || {
        if let Ok(mut child) = child.lock() {
            let _ = child.wait();
        }
    });
    if let Err(error) = started {
        log::warn!("a killed process of a script is not reaped: {error}");
    }
}

/// Stops one child and everything of its group, then waits for it.
fn stop(entry: &Entry) -> Result<()> {
    stop_group(entry.pid);

    let mut child = entry.child.lock().map_err(|_| ScriptError::Finished)?;
    match child.kill() {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(source) => return Err(ScriptError::Kill { source }),
    }
    let _ = child.wait();
    Ok(())
}
