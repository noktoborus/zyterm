//! The journals of debugging, files in the directory `debug.journal_dir`
//! names (`docs/journal.md`).
//!
//! The journal of the line holds every byte of the line, both ways, so that a
//! session can be played again; the journal of messages holds what the `log`
//! crate is told.

mod escape;
mod line;
mod messages;
mod writer;

pub use escape::escape;
pub use line::{Direction, LineJournal};
pub use messages::{
    MessageJournal, install as install_logger, journal as messages, set_journal as set_messages,
    set_levels,
};

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

/// The moment the process started, which every journal counts its seconds
/// from, so the lines of two journals of one run can be laid side by side.
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Marks the start of the process; the first call wins.
pub fn start() {
    STARTED.get_or_init(Instant::now);
}

/// Seconds since the start of the process.
fn seconds() -> f64 {
    STARTED.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The first line of a journal of `what`: the program, the process, the moment
/// it opened and the system.
fn head(what: &str) -> String {
    format!(
        "# zyterm {} {what}, process {}, opened {}, at {:.6} seconds of the process, {} {}\n",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S %z"),
        seconds(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// The name of a journal opened now: `<date>-<time>-<pid>-<kind>.log`, in
/// the local time.
pub fn file_name(kind: &str) -> String {
    let now = jiff::Zoned::now();
    format!(
        "{}-{}-{kind}.log",
        now.strftime("%Y%m%d-%H%M%S"),
        std::process::id()
    )
}

/// The path of a journal of `kind` opened now in `directory`.
pub fn path_in(directory: &Path, kind: &str) -> PathBuf {
    directory.join(file_name(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_says_when_which_process_and_what() {
        let name = file_name("line");
        let pid = std::process::id().to_string();
        let parts: Vec<&str> = name.trim_end_matches(".log").split('-').collect();
        assert_eq!(parts.len(), 4, "{name}");
        assert_eq!(parts[0].len(), 8, "the date: {name}");
        assert_eq!(parts[1].len(), 6, "the time: {name}");
        assert_eq!(parts[2], pid);
        assert_eq!(parts[3], "line");
    }
}
