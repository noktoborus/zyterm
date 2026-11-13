//! The file of one journal and the thread that writes it.
//!
//! The thread that has something to say formats a whole line and hands it over
//! through a `ByteSwap` (`RULE payload.transport`); the thread of the journal
//! writes it, so a slow disk holds up the journal and not whoever spoke.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use zyt_bytes::ByteSwap;

/// How long the writing thread waits for lines before it looks at whether it
/// is still wanted.
const WAIT: Duration = Duration::from_millis(250);

/// One open journal file.
pub struct Writer {
    path: PathBuf,
    lines: Arc<ByteSwap>,
    written: AtomicU64,
    /// Set when the journal is closed, and by the thread when a write failed:
    /// nothing more is taken then, so a journal that cannot be written cannot
    /// grow in memory either.
    closed: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Writer {
    /// Opens `path` for appending, writes `head`, and starts the thread that
    /// writes the rest.
    pub fn open(path: &Path, head: &str) -> std::io::Result<Self> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
            })?;
        file.write_all(head.as_bytes())?;

        let lines = ByteSwap::with_capacity(64 * 1024);
        let closed = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("zyterm-journal".to_string())
            .spawn({
                let lines = lines.clone();
                let closed = closed.clone();
                move || write_out(file, &lines, &closed)
            })?;
        Ok(Self {
            path: path.to_path_buf(),
            lines,
            written: AtomicU64::new(head.len() as u64),
            closed,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// Where the journal is written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes handed to the journal so far, the head included.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// Hands one whole line over, or drops it when the journal is closed.
    pub fn give(&self, line: &[u8]) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        self.written.fetch_add(line.len() as u64, Ordering::Relaxed);
        self.lines.push(line);
    }
}

impl Drop for Writer {
    /// Writes what is still waiting and closes the file.
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.lines.wake();
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

/// The thread of a journal: writes what was handed over until the journal is
/// closed and nothing is left.
fn write_out(mut file: File, lines: &ByteSwap, closed: &AtomicBool) {
    let mut spare = Vec::new();
    loop {
        lines.take_into_wait(&mut spare, WAIT);
        if !spare.is_empty()
            && let Err(error) = file.write_all(&spare)
        {
            closed.store(true, Ordering::Release);
            lines.clear();
            log::warn!("journal: {error}; nothing more is written");
            return;
        }
        if closed.load(Ordering::Acquire) && lines.is_empty() {
            break;
        }
    }
    let _ = file.flush();
}
