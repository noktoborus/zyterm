//! Commands a shell marked, one file per source.
//!
//! The marks of a shell (OSC 133) say where a command begins and where its
//! output does, and the emulation reads the line that was typed out of the grid
//! between the two. It is kept here, beside the source it was typed into, so
//! the commands of one console never stand in the history of another.
//!
//! Nothing here is ever reported to the user. A history is a convenience and
//! not a thing the session depends on: a file that cannot be read, written or
//! locked leaves a line in the log, and the list is answered as empty. A
//! console that says nothing about its commands is the ordinary case, not a
//! failure to tell anybody about.
//!
//! Two things shape the file. Several copies of the application may be
//! connected to the same source, so every change is made under a lock and the
//! file is read again inside it: nothing is held in memory between two calls,
//! and a copy that wrote while this one was idle is not overwritten. And a
//! history is a list of what was typed and not of how often, so an entry that
//! is already there is moved to the top instead of being added again — with
//! the directory and the moment of this run, because what a command did is
//! what it did where it ran, and the run that counts is the last one.

use crate::sources::SourceKey;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use zyt_config::ConfigStore;

/// Directory holding the history files, below the configuration directory.
pub const HISTORY_DIR: &str = "history";

/// One command of a history: what was typed, where, and when it last ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// What was typed.
    pub command: String,
    /// Directory the application stood in when it ran, empty when there was
    /// none to name.
    #[serde(default)]
    pub directory: String,
    /// When it last ran.
    pub at: jiff::Timestamp,
}

/// The history of one source as it lives on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CommandHistory {
    /// Key of the source: an identity of a console, `usb:…` or `path:…`.
    key: SourceKey,
    /// What was typed, newest first.
    #[serde(default)]
    commands: Vec<Entry>,
}

impl CommandHistory {
    /// A history of this source with nothing in it.
    fn empty(key: &SourceKey) -> Self {
        Self {
            key: key.clone(),
            commands: Vec::new(),
        }
    }
}

/// The commands of one source as the file stands now, newest first.
///
/// The file is read on every call and never kept, because another copy of the
/// application may have written to it since the last one.
pub fn load(store: &ConfigStore, key: &SourceKey) -> Vec<Entry> {
    read(store, key).commands
}

/// True while the source has a history file with something in it.
///
/// The status bar asks this on every frame, so it is a look at the size of the
/// file and not a parse of it. Every file this module writes holds at least one
/// command — `remember` returns before it writes when there would be none — so
/// a file of no length is a file nobody has written to.
pub fn has_any(store: &ConfigStore, key: &SourceKey) -> bool {
    std::fs::metadata(store.path(&entry(&file_name(key)))).is_ok_and(|file| file.len() > 0)
}

/// Puts one command at the top of the history of a source.
///
/// An entry equal to it is taken out first, so the list holds each command
/// once, and what does not fit under `limit` falls off the end. A limit of
/// nothing keeps no history at all.
///
/// What is kept beside the command is where it ran and when: the entry that
/// was there is replaced rather than lifted, so a command run again in another
/// directory says the directory it ran in this time.
pub fn remember(
    store: &ConfigStore,
    key: &SourceKey,
    command: &str,
    directory: &str,
    limit: usize,
) {
    let command = command.trim();
    if command.is_empty() || limit == 0 {
        return;
    }

    let Some(_guard) = Guard::take(store, key) else {
        return;
    };

    let mut history = read(store, key);
    history.key = key.clone();
    history.commands.retain(|kept| kept.command != command);
    history.commands.insert(
        0,
        Entry {
            command: command.to_string(),
            directory: directory.to_string(),
            at: jiff::Timestamp::now(),
        },
    );
    history.commands.truncate(limit);

    if let Err(error) = store.save(&entry(&file_name(key)), &history) {
        log::warn!("cannot write the command history of {key}: {error}");
    }
}

/// The history of one source, empty when there is none to read.
///
/// A file that cannot be read is named in the log and answered as empty: a
/// history nobody can parse is not worth refusing a connection over.
fn read(store: &ConfigStore, key: &SourceKey) -> CommandHistory {
    match store.load::<CommandHistory>(&entry(&file_name(key))) {
        Ok(Some(history)) => history,
        Ok(None) => CommandHistory::empty(key),
        Err(error) => {
            log::warn!("cannot read the command history of {key}: {error}");
            CommandHistory::empty(key)
        }
    }
}

/// A held lock on the history of one source.
///
/// It covers the whole of reading the file, changing it and writing it back,
/// which is what keeps two copies of the application from each writing a file
/// that has never seen the other's commands. The lock sits on a file of its
/// own rather than on the history: the history is replaced by a rename, and a
/// lock on a file that is about to be replaced is a lock on nothing.
///
/// That file lives under the temporary directory of the platform and not
/// beside the history, because it says who is writing now and means nothing
/// once the machine has restarted.
struct Guard {
    file: File,
}

impl Guard {
    /// Takes the lock, waiting for whoever holds it.
    ///
    /// A lock that cannot be taken is a line in the log and no history: the
    /// alternative is writing a file another copy is in the middle of writing,
    /// which loses what that one had to say.
    fn take(store: &ConfigStore, key: &SourceKey) -> Option<Self> {
        match open_lock(store, key) {
            Ok(file) => Some(Self { file }),
            Err(error) => {
                log::warn!("cannot lock the command history of {key}: {error}");
                None
            }
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Opens the lock file of one source, making the directory when it is missing.
fn open_lock(store: &ConfigStore, key: &SourceKey) -> std::io::Result<File> {
    std::fs::create_dir_all(store.lock_dir())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path(store, key))?;
    file.lock()?;
    Ok(file)
}

/// File the history of one source is written to, named after its key.
fn file_name(key: &SourceKey) -> String {
    format!("{}.yaml", key.slug())
}

fn entry(file: &str) -> String {
    format!("{HISTORY_DIR}/{file}")
}

fn lock_path(store: &ConfigStore, key: &SourceKey) -> PathBuf {
    store
        .lock_dir()
        .join(format!("{HISTORY_DIR}-{}.lock", key.slug()))
}

#[cfg(test)]
mod tests {

    /// A device, to stand beside the consoles.
    fn device() -> SourceKey {
        SourceKey::Port("path:/dev/ttyUSB0".parse().expect("the identity reads"))
    }

    /// Two consoles told apart, the way a file name tells them apart.
    fn source(last: u8) -> SourceKey {
        SourceKey::Console(
            format!("00000000-0000-0000-0000-0000000000{last:02}")
                .parse()
                .expect("the identity reads"),
        )
    }

    use super::*;

    /// What the history of a source says, as the commands alone.
    fn commands(store: &ConfigStore, key: &SourceKey) -> Vec<String> {
        load(store, key)
            .into_iter()
            .map(|entry| entry.command)
            .collect()
    }

    fn store(case: &str) -> ConfigStore {
        let root =
            std::env::temp_dir().join(format!("zyterm-history-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the directory is made");
        ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("locks"))
    }

    /// A lock names a copy that is writing now, so it belongs in the temporary
    /// directory and not among the files somebody keeps.
    #[test]
    fn the_lock_is_not_left_among_the_settings() {
        let store = store("lock-place");

        remember(&store, &source(1), "ls", "/tmp", 10);

        let locks: Vec<PathBuf> = std::fs::read_dir(store.lock_dir())
            .expect("the lock directory was made")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|kind| kind == "lock"))
            .collect();
        assert_eq!(locks.len(), 1, "one source, one lock: {locks:?}");

        let strays = walk(store.config_dir());
        assert!(
            !strays.iter().any(|path| path.ends_with(".lock")),
            "a lock was left in the configuration directory: {strays:?}"
        );
    }

    /// Every file under a directory, named relative to nothing in particular.
    fn walk(root: &std::path::Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path.display().to_string());
            }
        }
        found
    }

    #[test]
    fn the_newest_command_stands_first() {
        let store = store("order");

        remember(&store, &source(1), "ls", "/tmp", 10);
        remember(&store, &source(1), "cargo test", "/tmp", 10);

        assert_eq!(commands(&store, &source(1)), ["cargo test", "ls"]);
    }

    #[test]
    fn a_command_that_is_already_there_moves_up_instead_of_repeating() {
        let store = store("dedup");

        for command in ["ls", "cargo test", "ls"] {
            remember(&store, &source(1), command, "/tmp", 10);
        }

        assert_eq!(commands(&store, &source(1)), ["ls", "cargo test"]);
    }

    #[test]
    fn what_does_not_fit_falls_off_the_end() {
        let store = store("limit");

        for command in ["one", "two", "three"] {
            remember(&store, &source(1), command, "/tmp", 2);
        }

        assert_eq!(commands(&store, &source(1)), ["three", "two"]);
    }

    #[test]
    fn an_entry_says_where_it_ran_and_when() {
        let store = store("where");
        let before = jiff::Timestamp::now();

        remember(&store, &source(1), "make", "/srv/build", 10);

        let entry = load(&store, &source(1))
            .into_iter()
            .next()
            .expect("the command was written down");
        assert_eq!(entry.command, "make");
        assert_eq!(entry.directory, "/srv/build");
        assert!(entry.at >= before, "the moment of the run");
    }

    /// A command run again somewhere else says where it ran this time: the
    /// entry is replaced and not lifted.
    #[test]
    fn a_command_run_again_says_the_directory_of_the_last_run() {
        let store = store("moved");

        remember(&store, &source(1), "make", "/srv/one", 10);
        remember(&store, &source(1), "make", "/srv/two", 10);

        let entries = load(&store, &source(1));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].directory, "/srv/two");
    }

    #[test]
    fn a_limit_of_nothing_keeps_nothing() {
        let store = store("nolimit");

        remember(&store, &source(1), "ls", "/tmp", 0);

        assert!(load(&store, &source(1)).is_empty());
    }

    #[test]
    fn a_source_nobody_typed_into_has_no_history_to_offer() {
        let store = store("none");

        assert!(!has_any(&store, &source(1)));

        remember(&store, &source(1), "ls", "/tmp", 10);

        assert!(has_any(&store, &source(1)));
        assert!(!has_any(&store, &source(2)));
    }

    #[test]
    fn one_source_never_reads_the_history_of_another() {
        let store = store("apart");

        remember(&store, &source(1), "ls", "/tmp", 10);
        remember(&store, &device(), "reboot", "/tmp", 10);

        assert_eq!(commands(&store, &source(1)), ["ls"]);
        assert_eq!(commands(&store, &device()), ["reboot"]);
    }

    /// What a second copy of the application wrote while this one was idle is
    /// still there afterwards, because the file is read again inside the lock
    /// and never carried over from the last call.
    #[test]
    fn a_write_by_somebody_else_is_not_overwritten() {
        let store = store("shared");
        let other = ConfigStore::with_paths(
            store.config_dir().to_path_buf(),
            store.data_dir().to_path_buf(),
            store.lock_dir().to_path_buf(),
        );

        remember(&store, &source(1), "ls", "/tmp", 10);
        remember(&other, &source(1), "reboot", "/tmp", 10);
        remember(&store, &source(1), "cargo test", "/tmp", 10);

        assert_eq!(commands(&store, &source(1)), ["cargo test", "reboot", "ls"]);
    }

    #[test]
    fn a_file_that_cannot_be_read_answers_as_empty() {
        let store = store("broken");
        std::fs::create_dir_all(store.path(HISTORY_DIR)).expect("the directory is made");
        std::fs::write(
            store.path(&entry(&file_name(&source(1)))),
            "not: [a, history",
        )
        .expect("the file is written");

        assert!(load(&store, &source(1)).is_empty());
    }
}
