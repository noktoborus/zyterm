//! Running the operations on threads of their own.
//!
//! Every task is started, watched and stopped by its number. Nothing blocks the
//! caller: progress and the end of a task arrive as messages the caller picks
//! up when it likes.

use crate::error::Result;
use crate::ops;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Number of a task, handed out in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

/// What a task does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTask {
    /// Move a file or a directory, which is also how it is renamed.
    Move {
        /// File or directory that moves.
        from: PathBuf,
        /// Where it lands.
        to: PathBuf,
    },
    /// Hand a file to the trash of the desktop.
    Trash {
        /// File that goes.
        path: PathBuf,
    },
    /// Remove a file, or a directory with everything under it, for good.
    Delete {
        /// File or directory that goes.
        path: PathBuf,
    },
    /// Read a whole file, refusing one larger than the limit.
    Read {
        /// File that is read.
        path: PathBuf,
        /// Largest size that is read, in bytes.
        limit: u64,
    },
    /// Write bytes into a file, replacing what was there.
    Write {
        /// File that is written.
        path: PathBuf,
        /// What goes into it.
        bytes: Vec<u8>,
    },
}

impl FileTask {
    /// Kind of the task, without its paths.
    pub fn kind(&self) -> TaskKind {
        match self {
            Self::Move { .. } => TaskKind::Move,
            Self::Trash { .. } => TaskKind::Trash,
            Self::Delete { .. } => TaskKind::Delete,
            Self::Read { .. } => TaskKind::Read,
            Self::Write { .. } => TaskKind::Write,
        }
    }

    /// File the task works on.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Move { from, .. } => from,
            Self::Trash { path }
            | Self::Delete { path }
            | Self::Read { path, .. }
            | Self::Write { path, .. } => path,
        }
    }
}

/// What a task does, without its paths, for a caller that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// A file moves.
    Move,
    /// A file goes to the trash.
    Trash,
    /// A file is removed.
    Delete,
    /// A file is read.
    Read,
    /// A file is written.
    Write,
}

/// What a finished task produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// The file landed here.
    Moved(PathBuf),
    /// The file went to the trash.
    Trashed(PathBuf),
    /// The file was removed.
    Deleted(PathBuf),
    /// The file was read whole.
    Read {
        /// File that was read.
        path: PathBuf,
        /// What it held.
        bytes: Vec<u8>,
    },
    /// The bytes were written into the file.
    Written(PathBuf),
}

/// Something a task reported.
#[derive(Debug)]
pub enum TaskEvent {
    /// The task came this far.
    Progress {
        /// Number of the task.
        id: TaskId,
        /// Bytes done.
        done: u64,
        /// Bytes expected, when the size is known.
        total: Option<u64>,
    },
    /// The task ended, well or badly.
    Finished {
        /// Number of the task.
        id: TaskId,
        /// What it produced, or why it stopped.
        outcome: Result<Done>,
    },
}

/// A task that is still running.
#[derive(Debug, Clone)]
pub struct TaskState {
    /// Number of the task.
    pub id: TaskId,
    /// What it does.
    pub kind: TaskKind,
    /// File it works on.
    pub path: PathBuf,
    /// When it started.
    pub started: Instant,
    /// Bytes done so far.
    pub done: u64,
    /// Bytes expected, when the size is known.
    pub total: Option<u64>,
    /// True once the task was asked to stop and before it has.
    pub cancelled: bool,
}

impl TaskState {
    /// How long the task has been running.
    pub fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }

    /// How long it has left, when there is enough to tell from.
    ///
    /// The guess is the time so far carried over the bytes that are left, which
    /// is only worth showing once something has actually moved.
    pub fn remaining(&self) -> Option<std::time::Duration> {
        let total = self.total?;
        if self.done == 0 || self.done >= total {
            return None;
        }
        let elapsed = self.elapsed().as_secs_f64();
        let left = (total - self.done) as f64 * elapsed / self.done as f64;
        Some(std::time::Duration::from_secs_f64(left))
    }
}

/// Tasks of the application, each on a thread of its own.
#[derive(Debug)]
pub struct TaskRunner {
    next: u64,
    running: BTreeMap<TaskId, Running>,
    sender: Sender<TaskEvent>,
    events: Receiver<TaskEvent>,
}

#[derive(Debug)]
struct Running {
    state: TaskState,
    cancel: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRunner {
    /// A runner with nothing running.
    pub fn new() -> Self {
        let (sender, events) = channel();
        Self {
            next: 1,
            running: BTreeMap::new(),
            sender,
            events,
        }
    }

    /// Starts a task and returns its number.
    ///
    /// `notify` is called whenever something happened, so a caller that sleeps
    /// between frames knows to wake up.
    pub fn start(&mut self, task: FileTask, notify: Option<Arc<dyn Fn() + Send + Sync>>) -> TaskId {
        let id = TaskId(self.next);
        self.next += 1;

        let cancel = Arc::new(AtomicBool::new(false));
        let state = TaskState {
            id,
            kind: task.kind(),
            path: task.path().to_path_buf(),
            started: Instant::now(),
            done: 0,
            total: ops::size_of(task.path()),
            cancelled: false,
        };

        let sender = self.sender.clone();
        let flag = cancel.clone();
        let handle = std::thread::spawn(move || {
            let outcome = run(&task, &flag, &sender, id, notify.as_ref());
            let _ = sender.send(TaskEvent::Finished { id, outcome });
            if let Some(notify) = notify {
                notify();
            }
        });

        self.running.insert(
            id,
            Running {
                state,
                cancel,
                handle: Some(handle),
            },
        );
        id
    }

    /// Asks one task to stop where it is.
    ///
    /// An operation that is a loop — a copy, a read, a deletion — answers
    /// within a chunk or an entry. One that is a single call to the system, the
    /// trash above all, only answers before it starts.
    pub fn cancel(&self, id: TaskId) {
        if let Some(running) = self.running.get(&id) {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Asks every task to stop where it is.
    pub fn cancel_all(&self) {
        for running in self.running.values() {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Everything that happened since the last time, with the running tasks
    /// brought up to date and the finished ones taken off the list.
    pub fn poll(&mut self) -> Vec<TaskEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match &event {
                TaskEvent::Progress { id, done, total } => {
                    if let Some(running) = self.running.get_mut(id) {
                        running.state.done = *done;
                        running.state.total = *total;
                    }
                }
                TaskEvent::Finished { id, .. } => {
                    if let Some(mut running) = self.running.remove(id)
                        && let Some(handle) = running.handle.take()
                    {
                        let _ = handle.join();
                    }
                }
            }
            events.push(event);
        }
        events
    }

    /// The tasks that are still running, oldest first.
    pub fn running(&self) -> Vec<TaskState> {
        self.running
            .values()
            .map(|running| TaskState {
                cancelled: running.cancel.load(Ordering::Relaxed),
                ..running.state.clone()
            })
            .collect()
    }

    /// How many tasks are running.
    pub fn len(&self) -> usize {
        self.running.len()
    }

    /// True while nothing is running.
    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }
}

fn run(
    task: &FileTask,
    cancel: &Arc<AtomicBool>,
    sender: &Sender<TaskEvent>,
    id: TaskId,
    notify: Option<&Arc<dyn Fn() + Send + Sync>>,
) -> Result<Done> {
    let mut report = |done: u64, total: Option<u64>| {
        let _ = sender.send(TaskEvent::Progress { id, done, total });
        if let Some(notify) = notify {
            notify();
        }
    };

    match task {
        FileTask::Move { from, to } => {
            ops::move_path(from, to, cancel, &mut report).map(Done::Moved)
        }
        FileTask::Trash { path } => {
            ops::trash_file(path, cancel).map(|()| Done::Trashed(path.clone()))
        }
        FileTask::Delete { path } => {
            ops::delete_path(path, cancel, &mut report).map(|()| Done::Deleted(path.clone()))
        }
        FileTask::Read { path, limit } => {
            ops::read_file(path, *limit, cancel, &mut report).map(|bytes| Done::Read {
                path: path.clone(),
                bytes,
            })
        }
        FileTask::Write { path, bytes } => {
            ops::write_file(path, bytes, cancel, &mut report).map(Done::Written)
        }
    }
}

/// Shared handle of a runner, for a caller that hands it around.
pub type SharedRunner = Arc<Mutex<TaskRunner>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FileError;

    fn directory(case: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("zyt-tasks-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the directory is created");
        path
    }

    fn wait_for(runner: &mut TaskRunner, id: TaskId) -> Result<Done> {
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while Instant::now() < deadline {
            for event in runner.poll() {
                if let TaskEvent::Finished { id: done, outcome } = event
                    && done == id
                {
                    return outcome;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the task never finished");
    }

    #[test]
    fn a_task_runs_on_its_own_and_reports_its_end() {
        let directory = directory("run");
        let from = directory.join("one.txt");
        let to = directory.join("two.txt");
        std::fs::write(&from, b"payload").expect("the file is written");

        let mut runner = TaskRunner::new();
        let id = runner.start(
            FileTask::Move {
                from: from.clone(),
                to: to.clone(),
            },
            None,
        );
        assert_eq!(runner.len(), 1);

        assert!(matches!(
            wait_for(&mut runner, id),
            Ok(Done::Moved(landed)) if landed == to
        ));
        assert!(runner.is_empty(), "a finished task leaves the list");
        assert!(to.exists());

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_cancelled_task_says_so() {
        let directory = directory("cancel");
        let path = directory.join("big.bin");
        std::fs::write(&path, vec![0_u8; 8 * 1024 * 1024]).expect("the file is written");

        let mut runner = TaskRunner::new();
        let id = runner.start(
            FileTask::Read {
                path,
                limit: 32 * 1024 * 1024,
            },
            None,
        );
        runner.cancel(id);

        assert!(matches!(
            wait_for(&mut runner, id),
            Err(FileError::Cancelled) | Ok(Done::Read { .. })
        ));

        let _ = std::fs::remove_dir_all(directory);
    }

    /// Bytes go into the file the task names, and the task says where they
    /// landed.
    #[test]
    fn a_write_puts_the_bytes_in_the_file_it_names() {
        let directory = directory("write");
        let path = directory.join("selection.txt");

        let mut runner = TaskRunner::new();
        let id = runner.start(
            FileTask::Write {
                path: path.clone(),
                bytes: b"two lines\nof a selection\n".to_vec(),
            },
            None,
        );

        assert_eq!(
            wait_for(&mut runner, id).expect("the write finishes"),
            Done::Written(path.clone())
        );
        assert_eq!(
            std::fs::read(&path).expect("the file is there"),
            b"two lines\nof a selection\n"
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    /// A write that is stopped leaves no file behind: half of what was asked for
    /// says nothing about which half it is.
    #[test]
    fn a_write_that_is_stopped_leaves_no_file() {
        let directory = directory("write-cancelled");
        let path = directory.join("large.bin");

        let mut runner = TaskRunner::new();
        let id = runner.start(
            FileTask::Write {
                path: path.clone(),
                bytes: vec![7_u8; 8 * 1024 * 1024],
            },
            None,
        );
        runner.cancel(id);

        match wait_for(&mut runner, id) {
            Err(FileError::Cancelled) => assert!(!path.exists(), "and nothing is left of it"),
            Ok(Done::Written(_)) => assert!(path.exists(), "it finished before it was stopped"),
            other => panic!("neither written nor stopped: {other:?}"),
        }

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn what_is_left_is_guessed_from_what_is_done() {
        let state = TaskState {
            id: TaskId(1),
            kind: TaskKind::Move,
            path: PathBuf::from("/tmp/one"),
            started: Instant::now() - std::time::Duration::from_secs(2),
            done: 100,
            total: Some(300),
            cancelled: false,
        };

        let left = state.remaining().expect("two thirds are left");
        assert!(
            left.as_secs_f64() > 3.0 && left.as_secs_f64() < 5.0,
            "{left:?}"
        );

        let untouched = TaskState {
            done: 0,
            ..state.clone()
        };
        assert_eq!(untouched.remaining(), None, "nothing to guess from yet");
    }
}
