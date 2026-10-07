//! Commands a shell marked, one file per source, and the ones added by hand.
//!
//! The marks of a shell (OSC 133) say where a command begins and where its
//! output does, and the emulation reads the line that was typed out of the grid
//! between the two. It is kept here, beside the source it was typed into, so
//! the commands of one console never stand in the history of another.
//!
//! Beside those files stands one more, `history/added.yaml`, of the same shape
//! and naming no source: what somebody picked out of the output and added by
//! hand. That one is shared by every console and every port, because a command
//! worth keeping was worth keeping wherever it was read, and the source it was
//! read on is often not the one it is to be typed into.
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
//! the directory, the moment and the caret notation of this run, because what a
//! command did is what it did where it ran, and the run that counts is the last
//! one.
//!
//! A read is answered from `Cache` while the file stands as it stood when it
//! was last read. That is not a copy kept between two calls: the file is asked
//! what it is — when it was written and how long it is — on every look, and a
//! write by any copy of the application answers differently.

use crate::sources::SourceKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::SystemTime;
use zyt_config::ConfigStore;

/// Directory holding the history files, below the configuration directory.
pub const HISTORY_DIR: &str = "history";

/// Name of the file the added commands are kept in, without its suffix.
const ADDED_FILE: &str = "added";

/// Which list of commands a call is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum List {
    /// The commands the shell of one source marked, kept beside that source.
    Source(SourceKey),
    /// The commands added by hand, which every source shares.
    Added,
}

impl std::fmt::Display for List {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(key) => write!(out, "{key}"),
            Self::Added => write!(out, "the added commands"),
        }
    }
}

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
    /// True while the carets of it were read as control codes when it ran.
    ///
    /// A command is kept as the text it was written in, so this is the only
    /// thing that says what that text means: an entry of it holds `^C` where a
    /// byte went out. It is what the plate of the command says, and what
    /// `App::run_from_history` reads the text by when it is typed back — a
    /// command that ran a control code runs it again.
    #[serde(default)]
    pub caret: bool,
}

/// The history of one list as it lives on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CommandHistory {
    /// Key of the source: an identity of a console, `usb:…` or `path:…`. The
    /// added commands name no source, and the file carries no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<SourceKey>,
    /// What was typed, newest first.
    #[serde(default)]
    commands: Vec<Entry>,
}

impl CommandHistory {
    /// A history of this list with nothing in it.
    fn empty(list: &List) -> Self {
        Self {
            key: key_of(list),
            commands: Vec::new(),
        }
    }
}

/// The commands of one list as the file stands now, newest first.
///
/// The file is read on every call and never kept, because another copy of the
/// application may have written to it since the last one. `Cache` is what a
/// caller that asks often reads through.
pub fn load(store: &ConfigStore, list: &List) -> Vec<Entry> {
    read(store, list).commands
}

/// True while the list has a file with something in it.
///
/// The status bar asks this on every frame, so it is a look at the size of the
/// file and not a parse of it. Every file this module writes holds at least one
/// command — `remember` returns before it writes when there would be none, and
/// `forget` takes the file away when it empties it — so a file of no length is
/// a file nobody has written to.
pub fn has_any(store: &ConfigStore, list: &List) -> bool {
    std::fs::metadata(path(store, list)).is_ok_and(|file| file.len() > 0)
}

/// Puts one command at the top of a list.
///
/// An entry equal to it is taken out first, so the list holds each command
/// once, and what does not fit under `limit` falls off the end. A limit of
/// nothing keeps no history at all.
///
/// What is kept beside the command is where it ran, when, and whether its
/// carets were read as control codes: the entry that was there is replaced
/// rather than lifted, so a command run again in another directory, or with
/// that switch turned the other way, says what it was this time.
pub fn remember(
    store: &ConfigStore,
    list: &List,
    command: &str,
    directory: &str,
    caret: bool,
    limit: usize,
) {
    let command = command.trim();
    if command.is_empty() || limit == 0 {
        return;
    }

    let Some(_guard) = Guard::take(store, list) else {
        return;
    };

    let mut history = read(store, list);
    history.key = key_of(list);
    history.commands.retain(|kept| kept.command != command);
    history.commands.insert(
        0,
        Entry {
            command: command.to_string(),
            directory: directory.to_string(),
            at: jiff::Timestamp::now(),
            caret,
        },
    );
    history.commands.truncate(limit);

    write(store, list, &history);
}

/// Takes one command out of a list.
///
/// The file is read again under the lock, so a command another copy of the
/// application added meanwhile is still there afterwards. A list the command
/// leaves empty has its file taken away rather than written empty: a file of no
/// length is what `has_any` answers no for, and a list of nothing is a list
/// there is no button for.
pub fn forget(store: &ConfigStore, list: &List, command: &str) {
    let Some(_guard) = Guard::take(store, list) else {
        return;
    };

    let mut history = read(store, list);
    let before = history.commands.len();
    history.commands.retain(|kept| kept.command != command);
    if history.commands.len() == before {
        return;
    }

    if history.commands.is_empty() {
        if let Err(error) = std::fs::remove_file(path(store, list)) {
            log::warn!("cannot take away the emptied command history of {list}: {error}");
        }
        return;
    }

    history.key = key_of(list);
    write(store, list, &history);
}

/// The lists already read, each answered from here while its file stands as it
/// did when it was read.
///
/// A history is read whenever the menu of it opens, and a menu that opens on
/// every key of a query would read and parse the same file again for nothing.
/// What says the file is the file that was read is what the file system says
/// about it: the moment it was last written and how long it is. Both are asked
/// on every look, so a write by this copy of the application or by any other is
/// a read of the file and not of this.
#[derive(Debug, Default)]
pub struct Cache {
    /// What was read from each file, by the name of that file.
    files: BTreeMap<String, Kept>,
}

/// One list as it was last read, with what the file said about itself then.
#[derive(Debug)]
struct Kept {
    /// The moment the file was last written and how long it was.
    stamp: (SystemTime, u64),
    /// What it held.
    commands: Vec<Entry>,
}

impl Cache {
    /// The commands of one list, read from the file when it has changed since
    /// the last look and answered from here when it has not.
    ///
    /// A file the system says nothing about — one that is not there, one whose
    /// moment of writing cannot be had — is read every time and never kept:
    /// what is not known cannot be compared.
    pub fn commands(&mut self, store: &ConfigStore, list: &List) -> Vec<Entry> {
        let file = file_name(list);
        let stamp = stamp(store, list);

        if let Some(stamp) = stamp
            && let Some(kept) = self.files.get(&file)
            && kept.stamp == stamp
        {
            return kept.commands.clone();
        }

        let commands = load(store, list);
        match stamp {
            Some(stamp) => {
                self.files.insert(
                    file,
                    Kept {
                        stamp,
                        commands: commands.clone(),
                    },
                );
            }
            None => {
                self.files.remove(&file);
            }
        }
        commands
    }

    /// Says this copy of the application has written to a list, so the next look
    /// reads the file again.
    ///
    /// What the file system says about a file answers for a write by anybody,
    /// but only to the moment and the length of it: a list at its limit whose
    /// oldest command was dropped for one of the same length, within whatever
    /// the file system counts a moment in, would say it is the file that was
    /// read. A write this copy made is known here without asking, so it is said
    /// here.
    pub fn changed(&mut self, list: &List) {
        self.files.remove(&file_name(list));
    }
}

/// What the file system says about a history file: when it was last written and
/// how long it is, or nothing when it cannot say.: when it was last written and
/// how long it is, or nothing when it cannot say.
fn stamp(store: &ConfigStore, list: &List) -> Option<(SystemTime, u64)> {
    let file = std::fs::metadata(path(store, list)).ok()?;
    Some((file.modified().ok()?, file.len()))
}

/// The history of one list, empty when there is none to read.
///
/// A file that cannot be read is named in the log and answered as empty: a
/// history nobody can parse is not worth refusing a connection over.
fn read(store: &ConfigStore, list: &List) -> CommandHistory {
    match store.load::<CommandHistory>(&entry(&file_name(list))) {
        Ok(Some(history)) => history,
        Ok(None) => CommandHistory::empty(list),
        Err(error) => {
            log::warn!("cannot read the command history of {list}: {error}");
            CommandHistory::empty(list)
        }
    }
}

/// Writes a history back, a failure named in the log and nowhere else.
fn write(store: &ConfigStore, list: &List, history: &CommandHistory) {
    if let Err(error) = store.save(&entry(&file_name(list)), history) {
        log::warn!("cannot write the command history of {list}: {error}");
    }
}

/// A held lock on one list.
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
    fn take(store: &ConfigStore, list: &List) -> Option<Self> {
        match open_lock(store, list) {
            Ok(file) => Some(Self { file }),
            Err(error) => {
                log::warn!("cannot lock the command history of {list}: {error}");
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

/// Opens the lock file of one list, making the directory when it is missing.
fn open_lock(store: &ConfigStore, list: &List) -> std::io::Result<File> {
    std::fs::create_dir_all(store.lock_dir())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path(store, list))?;
    file.lock()?;
    Ok(file)
}

/// Source a list names, nothing for the added commands.
fn key_of(list: &List) -> Option<SourceKey> {
    match list {
        List::Source(key) => Some(key.clone()),
        List::Added => None,
    }
}

/// Name a list is written under, the slug of its source or the shared name.
fn slug(list: &List) -> String {
    match list {
        List::Source(key) => key.slug(),
        List::Added => ADDED_FILE.to_string(),
    }
}

/// File a list is written to.
fn file_name(list: &List) -> String {
    format!("{}.yaml", slug(list))
}

fn entry(file: &str) -> String {
    format!("{HISTORY_DIR}/{file}")
}

fn path(store: &ConfigStore, list: &List) -> PathBuf {
    store.path(&entry(&file_name(list)))
}

fn lock_path(store: &ConfigStore, list: &List) -> PathBuf {
    store
        .lock_dir()
        .join(format!("{HISTORY_DIR}-{}.lock", slug(list)))
}

#[cfg(test)]
mod tests {

    /// A device, to stand beside the consoles.
    fn device() -> List {
        List::Source(SourceKey::Port(
            "path:/dev/ttyUSB0".parse().expect("the identity reads"),
        ))
    }

    /// Two consoles told apart, the way a file name tells them apart.
    fn source(last: u8) -> List {
        List::Source(SourceKey::Console(
            format!("00000000-0000-0000-0000-0000000000{last:02}")
                .parse()
                .expect("the identity reads"),
        ))
    }

    use super::*;

    /// What one list says, as the commands alone.
    fn commands(store: &ConfigStore, list: &List) -> Vec<String> {
        load(store, list)
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

        remember(&store, &source(1), "ls", "/tmp", false, 10);

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

        remember(&store, &source(1), "ls", "/tmp", false, 10);
        remember(&store, &source(1), "cargo test", "/tmp", false, 10);

        assert_eq!(commands(&store, &source(1)), ["cargo test", "ls"]);
    }

    #[test]
    fn a_command_that_is_already_there_moves_up_instead_of_repeating() {
        let store = store("dedup");

        for command in ["ls", "cargo test", "ls"] {
            remember(&store, &source(1), command, "/tmp", false, 10);
        }

        assert_eq!(commands(&store, &source(1)), ["ls", "cargo test"]);
    }

    #[test]
    fn what_does_not_fit_falls_off_the_end() {
        let store = store("limit");

        for command in ["one", "two", "three"] {
            remember(&store, &source(1), command, "/tmp", false, 2);
        }

        assert_eq!(commands(&store, &source(1)), ["three", "two"]);
    }

    #[test]
    fn an_entry_says_where_it_ran_and_when() {
        let store = store("where");
        let before = jiff::Timestamp::now();

        remember(&store, &source(1), "make", "/srv/build", false, 10);

        let entry = load(&store, &source(1))
            .into_iter()
            .next()
            .expect("the command was written down");
        assert_eq!(entry.command, "make");
        assert_eq!(entry.directory, "/srv/build");
        assert!(entry.at >= before, "the moment of the run");
        assert!(!entry.caret, "it was sent as it was written");
    }

    /// The switch of the plate is written down with the command, because a
    /// command holding `^C` ran as a byte and reads as two characters.
    #[test]
    fn an_entry_says_whether_its_carets_were_read() {
        let store = store("carets");

        remember(&store, &source(1), "cat\r^D", "/tmp", true, 10);

        let entry = load(&store, &source(1))
            .into_iter()
            .next()
            .expect("the command was written down");
        assert!(entry.caret);
    }

    /// The flag is of the run and not of the command, so the entry that was
    /// there is replaced by what this run was.
    #[test]
    fn a_command_run_again_says_the_carets_of_the_last_run() {
        let store = store("carets-again");

        remember(&store, &source(1), "reset", "/tmp", true, 10);
        remember(&store, &source(1), "reset", "/tmp", false, 10);

        let kept = load(&store, &source(1));
        assert_eq!(kept.len(), 1);
        assert!(!kept[0].caret);
    }

    /// A command run again somewhere else says where it ran this time: the
    /// entry is replaced and not lifted.
    #[test]
    fn a_command_run_again_says_the_directory_of_the_last_run() {
        let store = store("moved");

        remember(&store, &source(1), "make", "/srv/one", false, 10);
        remember(&store, &source(1), "make", "/srv/two", false, 10);

        let entries = load(&store, &source(1));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].directory, "/srv/two");
    }

    #[test]
    fn a_limit_of_nothing_keeps_nothing() {
        let store = store("nolimit");

        remember(&store, &source(1), "ls", "/tmp", false, 0);

        assert!(load(&store, &source(1)).is_empty());
    }

    #[test]
    fn a_source_nobody_typed_into_has_no_history_to_offer() {
        let store = store("none");

        assert!(!has_any(&store, &source(1)));

        remember(&store, &source(1), "ls", "/tmp", false, 10);

        assert!(has_any(&store, &source(1)));
        assert!(!has_any(&store, &source(2)));
    }

    #[test]
    fn one_source_never_reads_the_history_of_another() {
        let store = store("apart");

        remember(&store, &source(1), "ls", "/tmp", false, 10);
        remember(&store, &device(), "reboot", "/tmp", false, 10);

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

        remember(&store, &source(1), "ls", "/tmp", false, 10);
        remember(&other, &source(1), "reboot", "/tmp", false, 10);
        remember(&store, &source(1), "cargo test", "/tmp", false, 10);

        assert_eq!(commands(&store, &source(1)), ["cargo test", "reboot", "ls"]);
    }

    /// The added commands are one list for every source: a command put there
    /// from one console is offered on the next one, and it stands in neither
    /// source's own file.
    #[test]
    fn the_added_commands_are_shared_and_kept_apart_from_the_sources() {
        let store = store("added");

        remember(&store, &source(1), "ls", "/tmp", false, 10);
        remember(&store, &List::Added, "journalctl -b", "/tmp", false, 10);

        assert_eq!(commands(&store, &source(1)), ["ls"]);
        assert_eq!(commands(&store, &device()), Vec::<String>::new());
        assert_eq!(commands(&store, &List::Added), ["journalctl -b"]);
        assert!(has_any(&store, &List::Added));
    }

    /// The added commands name no source, so the file carries no key at all.
    #[test]
    fn the_file_of_the_added_commands_names_no_source() {
        let store = store("added-key");

        remember(&store, &List::Added, "ls", "/tmp", false, 10);

        let text = std::fs::read_to_string(path(&store, &List::Added)).expect("the file is there");
        assert!(!text.contains("key"), "{text}");
    }

    #[test]
    fn a_command_taken_out_is_gone_and_the_rest_stay() {
        let store = store("forget");
        for command in ["one", "two", "three"] {
            remember(&store, &source(1), command, "/tmp", false, 10);
        }

        forget(&store, &source(1), "two");

        assert_eq!(commands(&store, &source(1)), ["three", "one"]);
    }

    /// A command is taken out of the list it was named in and out of no other,
    /// even when both lists hold it.
    #[test]
    fn a_command_is_taken_out_of_the_list_it_was_named_in() {
        let store = store("forget-which");
        remember(&store, &source(1), "reboot", "/tmp", false, 10);
        remember(&store, &List::Added, "reboot", "/tmp", false, 10);

        forget(&store, &List::Added, "reboot");

        assert_eq!(commands(&store, &source(1)), ["reboot"]);
        assert!(commands(&store, &List::Added).is_empty());
    }

    /// A list the last command left empty has no file: `has_any` answers by the
    /// size of it, and the button that opens the list follows that answer.
    #[test]
    fn a_list_emptied_of_its_last_command_has_no_file_left() {
        let store = store("forget-last");
        remember(&store, &List::Added, "ls", "/tmp", false, 10);

        forget(&store, &List::Added, "ls");

        assert!(!path(&store, &List::Added).exists());
        assert!(!has_any(&store, &List::Added));
    }

    #[test]
    fn taking_out_a_command_that_is_not_there_changes_nothing() {
        let store = store("forget-none");
        remember(&store, &source(1), "ls", "/tmp", false, 10);

        forget(&store, &source(1), "reboot");

        assert_eq!(commands(&store, &source(1)), ["ls"]);
    }

    /// A file that stands as it stood is not parsed again: the content is
    /// replaced behind the cache with the length and the moment of writing kept,
    /// and the cache answers what it read.
    #[test]
    fn a_file_that_has_not_been_written_since_is_not_read_again() {
        let store = store("cache");
        let mut cache = Cache::default();
        remember(&store, &List::Added, "aaa", "/tmp", false, 10);

        assert_eq!(
            cache
                .commands(&store, &List::Added)
                .into_iter()
                .map(|entry| entry.command)
                .collect::<Vec<String>>(),
            ["aaa"]
        );

        let file = path(&store, &List::Added);
        let written = std::fs::metadata(&file)
            .expect("the file is there")
            .modified()
            .expect("the moment is there");
        let text = std::fs::read_to_string(&file).expect("the file reads");
        std::fs::write(&file, text.replace("aaa", "bbb")).expect("the file is written");
        keep_written(&file, written);

        assert_eq!(
            cache
                .commands(&store, &List::Added)
                .into_iter()
                .map(|entry| entry.command)
                .collect::<Vec<String>>(),
            ["aaa"],
            "the file says it is the file that was read"
        );

        keep_written(&file, written + std::time::Duration::from_secs(1));

        assert_eq!(
            cache
                .commands(&store, &List::Added)
                .into_iter()
                .map(|entry| entry.command)
                .collect::<Vec<String>>(),
            ["bbb"],
            "a file written since is read again"
        );
    }

    /// Says a file was written at this moment, which is what a cache of files
    /// is tested by.
    fn keep_written(file: &std::path::Path, at: SystemTime) {
        OpenOptions::new()
            .write(true)
            .open(file)
            .expect("the file opens")
            .set_modified(at)
            .expect("the moment is set");
    }

    /// A write this copy of the application made is said to the cache, which is
    /// what covers the one write the file system cannot tell apart: the same
    /// length, within the same moment.
    #[test]
    fn a_write_of_this_copy_is_read_again_whatever_the_file_says() {
        let store = store("cache-written");
        let mut cache = Cache::default();
        remember(&store, &List::Added, "aaa", "/tmp", false, 10);
        assert_eq!(first(&mut cache, &store), "aaa");

        let file = path(&store, &List::Added);
        let written = std::fs::metadata(&file)
            .expect("the file is there")
            .modified()
            .expect("the moment is there");
        let text = std::fs::read_to_string(&file).expect("the file reads");
        std::fs::write(&file, text.replace("aaa", "bbb")).expect("the file is written");
        keep_written(&file, written);
        assert_eq!(first(&mut cache, &store), "aaa");

        cache.changed(&List::Added);

        assert_eq!(first(&mut cache, &store), "bbb");
    }

    /// The first command a cache answers with for the added list.
    fn first(cache: &mut Cache, store: &ConfigStore) -> String {
        cache
            .commands(store, &List::Added)
            .into_iter()
            .next()
            .expect("there is a command")
            .command
    }

    /// A list nothing has written to is read every time: there is no file to
    /// compare, so nothing is kept to answer from.
    #[test]
    fn a_list_with_no_file_is_read_every_time() {
        let store = store("cache-none");
        let mut cache = Cache::default();

        assert!(cache.commands(&store, &List::Added).is_empty());

        remember(&store, &List::Added, "ls", "/tmp", false, 10);

        assert_eq!(cache.commands(&store, &List::Added).len(), 1);
    }

    #[test]
    fn a_file_that_cannot_be_read_answers_as_empty() {
        let store = store("broken");
        std::fs::create_dir_all(store.path(HISTORY_DIR)).expect("the directory is made");
        std::fs::write(path(&store, &source(1)), "not: [a, history").expect("the file is written");

        assert!(load(&store, &source(1)).is_empty());
    }
}
