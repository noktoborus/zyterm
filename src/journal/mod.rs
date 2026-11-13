//! The journals of debugging, files in the directory `debug.journal_dir`
//! names (`docs/journal.md`).
//!
//! The journal of the line holds every byte of the line, both ways, so that a
//! session can be played again.

mod escape;
mod line;

pub use escape::escape;
pub use line::{Direction, LineJournal};

use std::path::{Path, PathBuf};

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
