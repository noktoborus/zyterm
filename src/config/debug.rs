//! What is of debugging, under `debug:` in the file.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The debug window, the journal of the line and the journal of messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugSettings {
    /// The debug window is shown: what the program costs while it runs.
    #[serde(default)]
    pub window: bool,
    /// The bytes of the line, both ways, go to `<date>-<time>-<pid>-line.log`
    /// in [`DebugSettings::journal_dir`], so that a session can be played
    /// again.
    #[serde(default)]
    pub journal: bool,
    /// Where the journals go; empty is the working directory of the process.
    #[serde(default)]
    pub journal_dir: PathBuf,
    /// Messages of this program — `zyterm` and the crates `zyt_*` — that go to
    /// `<date>-<time>-<pid>-window.log`, from this level up.
    #[serde(default)]
    pub messages: MessageLevel,
    /// Messages of every other crate that go to the same file, from this level
    /// up, whatever [`DebugSettings::messages`] says.
    #[serde(default)]
    pub messages_others: MessageLevel,
}

impl DebugSettings {
    /// The directory the journals go to: the one named, the working directory
    /// of the process when none is, and a relative one under that.
    pub fn directory(&self) -> std::io::Result<PathBuf> {
        if self.journal_dir.is_absolute() {
            return Ok(self.journal_dir.clone());
        }
        Ok(std::env::current_dir()?.join(&self.journal_dir))
    }
}

/// The least important message that is still written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageLevel {
    /// None at all.
    #[default]
    Off,
    /// Errors.
    Error,
    /// Warnings and errors.
    Warn,
    /// What happens, as a person would want to know it.
    Info,
    /// What happens, as whoever looks for a fault wants to know it.
    Debug,
    /// Everything.
    Trace,
}

impl MessageLevel {
    /// Every level, in the order a menu offers them.
    pub const ALL: [MessageLevel; 6] = [
        MessageLevel::Off,
        MessageLevel::Error,
        MessageLevel::Warn,
        MessageLevel::Info,
        MessageLevel::Debug,
        MessageLevel::Trace,
    ];

    /// The name of the level in the file, and the slug of its menu entry.
    pub fn slug(self) -> &'static str {
        match self {
            MessageLevel::Off => "off",
            MessageLevel::Error => "error",
            MessageLevel::Warn => "warn",
            MessageLevel::Info => "info",
            MessageLevel::Debug => "debug",
            MessageLevel::Trace => "trace",
        }
    }

    /// The level a slug names.
    pub fn of_slug(slug: &str) -> Option<MessageLevel> {
        MessageLevel::ALL
            .into_iter()
            .find(|level| level.slug() == slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_without_the_block_reads_as_nothing_written() {
        let read: DebugSettings = serde_yaml_ng::from_str("{}").expect("the block reads");
        assert_eq!(read, DebugSettings::default());
        assert!(!read.journal && !read.window);
    }

    #[test]
    fn the_two_levels_stand_apart() {
        let read: DebugSettings =
            serde_yaml_ng::from_str("messages_others: info\n").expect("the block reads");
        assert_eq!(read.messages, MessageLevel::Off);
        assert_eq!(read.messages_others, MessageLevel::Info);
    }

    #[test]
    fn an_empty_or_relative_directory_stands_in_the_working_one() {
        let here = std::env::current_dir().expect("a working directory");
        let mut debug = DebugSettings::default();
        assert_eq!(debug.directory().expect("resolved"), here);
        debug.journal_dir = PathBuf::from("logs");
        assert_eq!(debug.directory().expect("resolved"), here.join("logs"));
        let absolute = std::env::temp_dir();
        debug.journal_dir = absolute.clone();
        assert_eq!(debug.directory().expect("resolved"), absolute);
    }

    #[test]
    fn a_slug_names_its_level() {
        for level in MessageLevel::ALL {
            assert_eq!(MessageLevel::of_slug(level.slug()), Some(level));
        }
    }
}
