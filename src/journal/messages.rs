//! The messages of the `log` crate: to stderr as `RUST_LOG` says, as before,
//! and to the journal of messages as the block *Debugging* says.
//!
//! A message is of zyterm when its target is `zyterm` or a crate `zyt_*`, and
//! of the others otherwise; each side has its level (`debug.messages`,
//! `debug.messages_others`), so the messages of the libraries can be written
//! without those of zyterm, and the other way round.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use super::writer::Writer;
use crate::config::MessageLevel;

/// An open journal of messages.
pub struct MessageJournal {
    writer: Writer,
}

impl MessageJournal {
    /// Opens `path` for appending and starts the thread that writes it.
    pub fn open(path: &Path) -> std::io::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            writer: Writer::open(path, &super::head("journal of messages"))?,
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

    /// Writes down one message, as one line: the second, the level, the
    /// target and the text, its line breaks escaped.
    fn record(&self, record: &log::Record<'_>) {
        let text = record
            .args()
            .to_string()
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        let line = format!(
            "{:.6} {} {} {text}\n",
            super::seconds(),
            record.level(),
            record.target()
        );
        self.writer.give(line.as_bytes());
    }
}

/// The logger of the process.
struct Logger {
    stderr: env_logger::Logger,
    zyterm: AtomicUsize,
    others: AtomicUsize,
    journal: RwLock<Option<Arc<MessageJournal>>>,
}

/// The logger, once it is installed.
static LOGGER: OnceLock<&'static Logger> = OnceLock::new();

impl Logger {
    /// The level a message of `target` is written to the journal from.
    fn journal_level(&self, target: &str) -> log::LevelFilter {
        let ours =
            target == "zyterm" || target.starts_with("zyterm::") || target.starts_with("zyt_");
        let level = match ours {
            true => &self.zyterm,
            false => &self.others,
        };
        filter_of(level.load(Ordering::Relaxed))
    }

    /// True when a message of this level and target goes to the journal.
    fn to_journal(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.journal_level(metadata.target()) && self.open().is_some()
    }

    fn open(&self) -> Option<Arc<MessageJournal>> {
        self.journal
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// The most detailed level anything still wants, which the `log` crate
    /// checks before it builds a message at all.
    fn widest(&self) -> log::LevelFilter {
        let journal = match self.open() {
            Some(_) => filter_of(self.zyterm.load(Ordering::Relaxed))
                .max(filter_of(self.others.load(Ordering::Relaxed))),
            None => log::LevelFilter::Off,
        };
        self.stderr.filter().max(journal)
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.stderr.enabled(metadata) || self.to_journal(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.stderr.matches(record) {
            self.stderr.log(record);
        }
        if record.level() <= self.journal_level(record.target())
            && let Some(journal) = self.open()
        {
            journal.record(record);
        }
    }

    fn flush(&self) {
        self.stderr.flush();
    }
}

/// The filter a level stored as a number stands for.
fn filter_of(level: usize) -> log::LevelFilter {
    MessageLevel::ALL
        .get(level)
        .copied()
        .unwrap_or_default()
        .filter()
}

/// Installs the logger of the process: stderr as `RUST_LOG` says, `info` when
/// it says nothing, and no journal yet.
pub fn install() {
    let stderr =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    let logger: &'static Logger = Box::leak(Box::new(Logger {
        stderr,
        zyterm: AtomicUsize::new(0),
        others: AtomicUsize::new(0),
        journal: RwLock::new(None),
    }));
    if log::set_logger(logger).is_ok() {
        let _ = LOGGER.set(logger);
        log::set_max_level(logger.widest());
    }
}

/// The levels from which the messages of zyterm and of the others go to the
/// journal of messages.
pub fn set_levels(zyterm: MessageLevel, others: MessageLevel) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let index = |level| {
        MessageLevel::ALL
            .iter()
            .position(|of| *of == level)
            .unwrap_or(0)
    };
    logger.zyterm.store(index(zyterm), Ordering::Relaxed);
    logger.others.store(index(others), Ordering::Relaxed);
    log::set_max_level(logger.widest());
}

/// Writes the messages into `journal` from now on, or into none.
///
/// The journal that was open is closed after the lock is let go of: closing
/// waits for its thread, and that thread may have a message to say.
pub fn set_journal(journal: Option<Arc<MessageJournal>>) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let previous = std::mem::replace(
        &mut *logger
            .journal
            .write()
            .unwrap_or_else(|error| error.into_inner()),
        journal,
    );
    log::set_max_level(logger.widest());
    drop(previous);
}

/// The journal of messages, if one is open.
pub fn journal() -> Option<Arc<MessageJournal>> {
    LOGGER.get().and_then(|logger| logger.open())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger() -> Logger {
        Logger {
            stderr: env_logger::Builder::new().build(),
            zyterm: AtomicUsize::new(0),
            others: AtomicUsize::new(0),
            journal: RwLock::new(None),
        }
    }

    #[test]
    fn the_messages_of_zyterm_and_of_the_others_have_levels_of_their_own() {
        let logger = logger();
        logger.zyterm.store(4, Ordering::Relaxed);
        logger.others.store(0, Ordering::Relaxed);
        assert_eq!(
            logger.journal_level("zyterm::tmux"),
            log::LevelFilter::Debug
        );
        assert_eq!(
            logger.journal_level("zyt_serial::supervisor"),
            log::LevelFilter::Debug
        );
        assert_eq!(
            logger.journal_level("winit::platform"),
            log::LevelFilter::Off
        );
        assert_eq!(
            logger.journal_level("egui_plate_menu"),
            log::LevelFilter::Off
        );

        logger.zyterm.store(0, Ordering::Relaxed);
        logger.others.store(3, Ordering::Relaxed);
        assert_eq!(logger.journal_level("zyterm"), log::LevelFilter::Off);
        assert_eq!(logger.journal_level("wgpu_core"), log::LevelFilter::Info);
    }

    #[test]
    fn a_message_is_one_line_of_its_level_and_target() {
        let path = std::env::temp_dir().join(format!("zyterm-messages-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = MessageJournal::open(&path).expect("the journal opens");
        journal.record(
            &log::Record::builder()
                .level(log::Level::Debug)
                .target("zyterm::tmux")
                .args(format_args!("two\nlines"))
                .build(),
        );
        drop(journal);
        let text = std::fs::read_to_string(&path).expect("the journal reads");
        let last = text.lines().last().expect("a line");
        assert!(last.ends_with(" DEBUG zyterm::tmux two\\nlines"), "{text}");
        let _ = std::fs::remove_file(&path);
    }
}
