//! The journal of the line: every byte of it, both ways, in the chunks the
//! line carried them, each with the moment it went and who held the line.
//!
//! A chunk is one line of text, `< 0.012345 screen <bytes>` for what came and
//! `> …` for what went, the bytes escaped as the recorded sessions of
//! `zyt-tmux` are (`escape`), so the reader of those plays a journal too. A
//! line that begins with `#` says something of the session and is skipped by
//! that reader.
//!
//! The thread that holds the line formats the chunk; the writer of the journal
//! (`writer`) takes it from there.

use std::path::Path;
use std::sync::Arc;

use super::writer::Writer;

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
    writer: Writer,
}

impl std::fmt::Debug for LineJournal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LineJournal")
            .field("path", &self.writer.path())
            .finish()
    }
}

impl LineJournal {
    /// Opens `path` for appending, writes what the session is, and starts the
    /// thread that writes the rest.
    pub fn open(path: &Path, about: &[String]) -> std::io::Result<Arc<Self>> {
        let mut head = super::head("journal of the line");
        for line in about {
            head.push_str("# ");
            head.push_str(line);
            head.push('\n');
        }
        Ok(Arc::new(Self {
            writer: Writer::open(path, &head)?,
        }))
    }

    /// Where the journal is written.
    pub fn path(&self) -> &Path {
        self.writer.path()
    }

    /// Bytes handed to the journal so far, the head included.
    pub fn written(&self) -> u64 {
        self.writer.written()
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
        let mut line = Vec::with_capacity(bytes.len() + 32);
        line.extend_from_slice(format!("{mark} {:.6} {owner} ", super::seconds()).as_bytes());
        super::escape(bytes, &mut line);
        line.push(b'\n');
        self.writer.give(&line);
    }

    /// Writes down something of the session that is no byte of the line: a
    /// source opened or let go of.
    pub fn note(&self, text: &str) {
        self.writer
            .give(format!("# {:.6} {text}\n", super::seconds()).as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
