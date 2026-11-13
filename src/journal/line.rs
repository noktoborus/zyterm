//! The journal of the line: every byte of it, both ways, in the chunks the
//! line carried them, each with the moment it went and who held the line.
//!
//! A chunk is one line of text, `< 0.012345 screen <bytes>` for what came and
//! `> …` for what went, the bytes escaped as the recorded sessions of
//! `zyt-tmux` are (`escape`), so the reader of those plays a journal too. A
//! line that begins with `#` says something of the session and is skipped by
//! that reader.
//!
//! The thread that holds the line formats the chunk and hands it over through
//! a `ByteSwap` (`RULE payload.transport`); a thread of the journal writes it,
//! so a slow disk holds up the journal and not the line.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use zyt_bytes::ByteSwap;

/// How long the writing thread waits for bytes before it looks at whether it
/// is still wanted.
const WAIT: Duration = Duration::from_millis(250);

/// Which way a chunk went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// From the source: what a device or a program said.
    In,
    /// To the source: what was typed, pasted, answered or sent.
    Out,
}

/// An open journal of the line.
pub struct LineJournal {
    path: PathBuf,
    started: Instant,
    chunks: Arc<ByteSwap>,
    written: AtomicU64,
    closed: Arc<AtomicBool>,
    writer: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for LineJournal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LineJournal")
            .field("path", &self.path)
            .finish()
    }
}

impl LineJournal {
    /// Opens `path` for appending, writes what the session is, and starts the
    /// thread that writes the rest.
    pub fn open(path: &Path, about: &[String]) -> std::io::Result<Arc<Self>> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| {
                std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
            })?;
        let mut head = format!(
            "# zyterm {} journal of the line, process {}, opened {}, {} {}\n",
            env!("CARGO_PKG_VERSION"),
            std::process::id(),
            jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S %z"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        for line in about {
            head.push_str("# ");
            head.push_str(line);
            head.push('\n');
        }
        file.write_all(head.as_bytes())?;

        let chunks = ByteSwap::with_capacity(64 * 1024);
        let closed = Arc::new(AtomicBool::new(false));
        let writer = std::thread::Builder::new()
            .name("zyterm-journal".to_string())
            .spawn({
                let chunks = chunks.clone();
                let closed = closed.clone();
                move || write_out(file, &chunks, &closed)
            })?;
        Ok(Arc::new(Self {
            path: path.to_path_buf(),
            started: Instant::now(),
            chunks,
            written: AtomicU64::new(head.len() as u64),
            closed,
            writer: Mutex::new(Some(writer)),
        }))
    }

    /// Where the journal is written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes handed to the journal so far, the head included.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// Writes down one chunk of the line.
    pub fn record(&self, direction: Direction, owner: &str, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mark = match direction {
            Direction::In => '<',
            Direction::Out => '>',
        };
        let seconds = self.started.elapsed().as_secs_f64();
        let mut line = Vec::with_capacity(bytes.len() + 32);
        line.extend_from_slice(format!("{mark} {seconds:.6} {owner} ").as_bytes());
        super::escape(bytes, &mut line);
        line.push(b'\n');
        self.give(&line);
    }

    /// Writes down something of the session that is no byte of the line: a
    /// source opened, a size changed.
    pub fn note(&self, text: &str) {
        let seconds = self.started.elapsed().as_secs_f64();
        self.give(format!("# {seconds:.6} {text}\n").as_bytes());
    }

    fn give(&self, line: &[u8]) {
        self.written.fetch_add(line.len() as u64, Ordering::Relaxed);
        self.chunks.push(line);
    }
}

impl Drop for LineJournal {
    /// Writes what is still waiting and closes the file.
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.chunks.wake();
        let writer = self
            .writer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(writer) = writer {
            let _ = writer.join();
        }
    }
}

/// The thread of the journal: writes what was handed over until the journal
/// is closed and nothing is left.
fn write_out(mut file: File, chunks: &ByteSwap, closed: &AtomicBool) {
    let mut spare = Vec::new();
    loop {
        chunks.take_into_wait(&mut spare, WAIT);
        if !spare.is_empty()
            && let Err(error) = file.write_all(&spare)
        {
            log::warn!("journal of the line: {error}; nothing more is written");
            return;
        }
        if closed.load(Ordering::Acquire) && chunks.is_empty() {
            break;
        }
    }
    let _ = file.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let count = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("zyterm-journal-{}-{count}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a directory for the test");
        directory
    }

    #[test]
    fn every_chunk_is_a_line_of_its_own_in_the_order_it_went() {
        let path = directory().join("line.log");
        let journal = LineJournal::open(
            &path,
            &["source: serial /dev/ttyUSB0 115200 8N1".to_string()],
        )
        .expect("the journal opens");
        journal.record(Direction::In, "screen", b"\x1bP1000p%begin 1 2 0\r\n");
        journal.note("size 80x24");
        journal.record(Direction::Out, "run", b"ls\r");
        drop(journal);

        let text = std::fs::read_to_string(&path).expect("the journal reads");
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("# zyterm "), "{text}");
        assert_eq!(lines[1], "# source: serial /dev/ttyUSB0 115200 8N1");
        let fields: Vec<&str> = lines[2].splitn(4, ' ').collect();
        assert_eq!(fields[0], "<");
        assert!(fields[1].parse::<f64>().is_ok(), "{}", lines[2]);
        assert_eq!(fields[2], "screen");
        assert_eq!(fields[3], "\\eP1000p%begin 1 2 0\\r\\n");
        assert!(lines[3].starts_with("# ") && lines[3].ends_with(" size 80x24"));
        assert!(lines[4].starts_with("> ") && lines[4].ends_with(" run ls\\r"));
    }

    #[test]
    fn a_journal_that_is_there_is_written_on() {
        let path = directory().join("line.log");
        drop(LineJournal::open(&path, &[]).expect("the journal opens"));
        let journal = LineJournal::open(&path, &[]).expect("the journal opens again");
        journal.record(Direction::In, "screen", b"x");
        drop(journal);
        let text = std::fs::read_to_string(&path).expect("the journal reads");
        assert_eq!(text.matches("# zyterm ").count(), 2, "{text}");
        assert!(text.ends_with("screen x\n"), "{text}");
    }

    #[test]
    fn a_directory_that_is_not_there_says_which_path_failed() {
        let path = directory().join("missing").join("line.log");
        let error = LineJournal::open(&path, &[]).expect_err("nowhere to write");
        assert!(error.to_string().contains("line.log"), "{error}");
    }
}
