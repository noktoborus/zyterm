//! Consoles, one file each.
//!
//! A console is everything the application knows about one way of starting a
//! local shell: how it starts and which palette it wears. Every console lives
//! in its own file below the `consoles` directory of the configuration, so a
//! console can be added, copied or thrown away by moving one file.
//!
//! What a console *is* is its [`ConsoleId`] and nothing else. The name is what
//! it is called, which is the user's to change at any moment — and what is
//! remembered about it, the values it was answered with and the commands its
//! shell marked are all addressed by the identity, so changing the name changes
//! the name.

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zyt_config::ConfigStore;

/// Directory holding the console files, below the configuration directory.
pub const CONSOLES_DIR: &str = "consoles";

/// The shell of this machine, as this program ships it.
const SHELL_ID: u128 = 0x7a1c_4e8d_5f34_4a21_9c60_18b2_6f03_d5e1;
/// The way onto another machine, as this program ships it.
const SSH_ID: u128 = 0x2d90_7b46_11c8_4f07_8a5e_3f61_c4d2_90ab;

/// What a console is, whatever it is called.
///
/// A console is named by the user and renamed by the user, so a name is no
/// identity: everything that has to keep pointing at one console across a
/// rename — its file, what is remembered about it, the values it was answered
/// with, the commands its shell marked, the source a window starts on — points
/// at this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct ConsoleId(uuid::Uuid);

impl ConsoleId {
    /// An identity nothing else carries.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// The identity of a console this program ships, which is the same on every
    /// machine: that is how one already written is recognized.
    const fn shipped(value: u128) -> Self {
        Self(uuid::Uuid::from_u128(value))
    }
}

impl Default for ConsoleId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConsoleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::str::FromStr for ConsoleId {
    type Err = AppError;

    fn from_str(text: &str) -> Result<Self> {
        uuid::Uuid::try_parse(text)
            .map(Self)
            .map_err(|_| AppError::ConsoleId {
                text: text.to_string(),
            })
    }
}

impl From<ConsoleId> for String {
    fn from(id: ConsoleId) -> Self {
        id.to_string()
    }
}

impl TryFrom<String> for ConsoleId {
    type Error = AppError;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

/// One way of starting a local console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Console {
    /// What this console is, whatever it is called.
    pub id: ConsoleId,
    /// Name shown in the menu of the sources.
    pub name: String,
    /// Program to run; the platform shell when empty.
    #[serde(default)]
    pub program: String,
    /// Arguments of the program.
    #[serde(default)]
    pub args: Vec<String>,
    /// Directory the program starts in; the directory of this window when
    /// empty.
    ///
    /// A console that names one is started there whatever the window was
    /// started in, because naming it is asking for it.
    #[serde(default)]
    pub directory: String,
    /// True while what this console prints may be acted on.
    ///
    /// It is a judgement of the user, not a fact about the machine: the output
    /// of a trusted console is answered by the trusted side of the settings and
    /// the paths it prints are worked with. A console that carries somebody
    /// else's output — an `ssh`, a `telnet`, a log being followed — is not one
    /// of those, wherever it runs.
    #[serde(default = "trusted")]
    pub trusted: bool,
    /// True while this console is started again when its program ends.
    ///
    /// A shell that is exited usually means the window is done with; a console
    /// that watches something — a log, a board that reboots — is meant to come
    /// back instead.
    #[serde(default)]
    pub restart: bool,
    /// What is remembered about this console: when it was used, where a
    /// transfer wrote, with which profile and what it answers by name.
    ///
    /// Nothing of a line stands here: a speed and a parity are what a device is
    /// opened with, and a console is a program that was started.
    #[serde(default)]
    pub memory: crate::sources::SourceMemory,
}

impl Console {
    /// Key this console is addressed by.
    pub fn key(&self) -> crate::sources::SourceKey {
        crate::sources::SourceKey::Console(self.id)
    }

    /// Program this console runs, resolved for display.
    pub fn program_or_shell(&self) -> String {
        if self.program.trim().is_empty() {
            zyt_pty::default_shell()
        } else {
            self.expand(&self.program)
        }
    }

    /// Name of this console as it is shown, with the values it keeps put in.
    ///
    /// The name is written once and worn on whatever the console reaches, so it
    /// asks for the values of this console by name the way its command line
    /// does — one console named `{remote_host}` is every board it is pointed
    /// at. Nothing is addressed by either spelling of it: that is what the
    /// identity is for.
    pub fn display_name(&self) -> String {
        self.expand(&self.name)
    }

    /// The names this console asks for, across its name, its line, its
    /// arguments and its directory.
    pub fn variables(&self) -> Vec<String> {
        let mut names = crate::sources::variable_names(&self.name);
        for text in std::iter::once(&self.program)
            .chain(self.args.iter())
            .chain(std::iter::once(&self.directory))
        {
            for name in crate::sources::variable_names(text) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// One text of this console with the values it keeps put in.
    ///
    /// The line and the directory are written once and used on whatever the
    /// console reaches — a host, a port, a path that differs per machine — so
    /// they ask for the values of this console by name, the way a command line
    /// of a transfer profile does.
    fn expand(&self, text: &str) -> String {
        crate::sources::expand(text, &self.memory.variable_map())
    }

    /// The directory this console starts in, when it names one that is there.
    ///
    /// A directory that is named and is not there is a line in the log and
    /// nothing else: a console must still start.
    pub fn working_directory(&self) -> Option<PathBuf> {
        let named = self.expand(&self.directory);
        let named = named.trim();
        if named.is_empty() {
            return None;
        }

        let path = PathBuf::from(named);
        if path.is_dir() {
            return Some(path);
        }
        log::warn!("{}: {named} is not a directory", self.id);
        None
    }

    /// The program to start and the arguments to start it with.
    ///
    /// The line is read the way a shell reads one, so `ssh -p 2222 host` is a
    /// program and two arguments and not a program with a very odd name.
    /// Nothing is run through a shell: the words are handed to the program
    /// directly, and the arguments of the console follow them.
    pub fn command(&self) -> Result<(String, Vec<String>)> {
        let expanded = self.expand(&self.program);
        let line = expanded.trim();
        if line.is_empty() {
            return Ok((
                zyt_pty::default_shell(),
                self.args.iter().map(|arg| self.expand(arg)).collect(),
            ));
        }

        let Some(mut words) = shlex::split(line) else {
            return Err(AppError::ConsoleProgram { id: self.id });
        };
        if words.is_empty() {
            return Err(AppError::ConsoleProgram { id: self.id });
        }

        let program = words.remove(0);
        words.extend(self.args.iter().map(|arg| self.expand(arg)));
        Ok((program, words))
    }
}

/// Default of a console that was not judged: a console the user configured and
/// starts is trusted until they say otherwise.
fn trusted() -> bool {
    true
}

/// A console of this machine's shell, with an identity of its own.
///
/// It is what every new console starts from: the shell, under a name, answering
/// nothing and wearing nothing.
pub fn default_console() -> Console {
    Console {
        id: ConsoleId::new(),
        name: "Local shell".to_string(),
        program: String::new(),
        args: Vec::new(),
        directory: String::new(),
        trusted: true,
        restart: false,
        memory: crate::sources::SourceMemory::default(),
    }
}

/// The consoles this program ships: a shell of this machine, and a way onto
/// another one.
///
/// They are a starting point and not a list to keep up to date — every one of
/// them is a file the user owns the moment it is written, to be edited, renamed
/// or thrown away.
///
/// The `ssh` one names what it connects to rather than carrying it: the user
/// and the host are values asked for on the way in, which is what the window of
/// `ui::ask` is for, so one console reaches every machine instead of one per
/// machine. Its output is not trusted, because what prints on it is the other
/// machine talking: a sequence that would set the title of this window or reach
/// its clipboard comes from somewhere nobody here can vouch for.
///
/// Two options stand on the line. `-e none` because the terminal is the
/// terminal — `ssh` must not keep an escape character of its own out of the
/// stream. And `NumberOfPasswordPrompts=1` because a password asked for again is
/// a password that was wrong, and a console that asks three times is a console
/// standing on a question nobody is going to answer differently; one refusal
/// ends it and the window says so.
pub fn shipped() -> Vec<Console> {
    vec![
        Console {
            id: ConsoleId::shipped(SHELL_ID),
            ..default_console()
        },
        Console {
            id: ConsoleId::shipped(SSH_ID),
            name: "SSH".to_string(),
            program: "ssh -e none -o NumberOfPasswordPrompts=1 {remote_user}@{remote_host}"
                .to_string(),
            trusted: false,
            ..default_console()
        },
    ]
}

/// Reads every console file, ordered by name.
///
/// A file that cannot be read is left out with a line in the log: one broken
/// file never hides the consoles beside it. A file that names no identity is
/// one of those — a console is its identity, and a file that does not say which
/// console it holds holds none.
///
/// The order is the name, because the name is what the list is read by; nothing
/// is addressed by the position.
pub fn load_all(store: &ConfigStore) -> Vec<Console> {
    let mut consoles: Vec<Console> = files(store)
        .into_iter()
        .filter_map(|file| match store.load::<Console>(&entry(&file)) {
            Ok(Some(console)) => Some(console),
            Ok(None) => None,
            Err(error) => {
                log::warn!("console {file}: {error}");
                None
            }
        })
        .collect();
    consoles.sort_by(|left, right| left.name.cmp(&right.name));
    consoles
}

/// Writes one console, to the file its identity names.
///
/// A rename is a write like any other: the file is the identity and the name is
/// in it, so nothing moves and nothing is written twice.
pub fn save(store: &ConfigStore, console: &Console) -> Result<()> {
    store
        .save(&entry(&file_name(console.id)), console)
        .map_err(AppError::from)
}

/// Throws one console away, file and all.
pub fn remove(store: &ConfigStore, console: &Console) -> Result<()> {
    std::fs::remove_file(path(store, &file_name(console.id))).map_err(|source| AppError::Console {
        id: console.id,
        source,
    })
}

/// Writes every shipped console the configuration directory has never heard
/// of, and answers with the ones it wrote.
///
/// A machine that has never run this program gets all of them, which is what a
/// list with nothing in it would otherwise be. One that has run it gets the
/// ones added since, which is how a console shipped later reaches somebody who
/// already has the directory; the price is the one transfer profiles pay for
/// the same rule — one thrown away comes back.
///
/// Every shipped console carries the same identity on every machine, so what is
/// already there is recognized whatever it has been renamed to since: a console
/// the user made their own is theirs, and it is never written a second time.
pub fn add_shipped(store: &ConfigStore) -> Vec<Console> {
    let known: Vec<ConsoleId> = load_all(store)
        .into_iter()
        .map(|console| console.id)
        .collect();

    let mut written = Vec::new();
    for console in shipped() {
        if known.contains(&console.id) {
            continue;
        }
        match save(store, &console) {
            Ok(()) => written.push(console),
            Err(error) => log::error!("cannot write the console {}: {error}", console.id),
        }
    }
    written
}

/// File one console is written to: its identity and nothing else, so the name
/// of the file says nothing that could stop being true.
fn file_name(id: ConsoleId) -> String {
    format!("{id}.yaml")
}

/// Entry of one console inside the console directory.
fn entry(file: &str) -> String {
    format!("{CONSOLES_DIR}/{file}")
}

fn path(store: &ConfigStore, file: &str) -> PathBuf {
    store.path(&entry(file))
}

fn files(store: &ConfigStore) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(store.path(CONSOLES_DIR)) else {
        return Vec::new();
    };
    let mut files: Vec<String> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("yaml"))
        })
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file of the console directory, for the tests that count them.
    fn written(store: &ConfigStore) -> Vec<String> {
        files(store)
    }

    #[test]
    fn a_console_that_names_no_directory_starts_where_the_window_did() {
        assert_eq!(default_console().working_directory(), None);
    }

    #[test]
    fn a_console_that_names_one_starts_there() {
        let directory = std::env::temp_dir();
        let console = Console {
            directory: directory.display().to_string(),
            ..default_console()
        };

        assert_eq!(console.working_directory(), Some(directory));
    }

    #[test]
    fn a_directory_that_is_not_there_is_no_directory() {
        let console = Console {
            directory: "/no/such/place/anywhere".to_string(),
            ..default_console()
        };

        assert_eq!(console.working_directory(), None);
    }

    fn store(case: &str) -> (ConfigStore, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("zyterm-consoles-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store =
            ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("locks"));
        (store, root)
    }

    #[test]
    fn a_console_is_written_read_and_renamed() {
        let (store, root) = store("round-trip");
        let mut console = default_console();

        save(&store, &console).expect("the console is written");
        assert_eq!(written(&store), vec![format!("{}.yaml", console.id)]);
        assert_eq!(load_all(&store), vec![console.clone()]);

        let key = console.key();
        console.name = "Build box".to_string();
        save(&store, &console).expect("the console is written again");
        assert_eq!(
            written(&store),
            vec![format!("{}.yaml", console.id)],
            "a rename is not a move and leaves no second file"
        );
        assert_eq!(load_all(&store), vec![console.clone()]);
        assert_eq!(console.key(), key, "and nothing is addressed differently");

        remove(&store, &console).expect("the console is removed");
        assert!(load_all(&store).is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    /// A speed and a parity are what a device is opened with, so nothing of a
    /// line stands in the file of a console: the two kinds keep what is theirs
    /// and the file says only what the console is.
    #[test]
    fn a_console_keeps_nothing_of_a_line() {
        let (store, root) = store("line");
        let console = default_console();
        save(&store, &console).expect("the console is written");

        let written = std::fs::read_to_string(
            store
                .config_dir()
                .join(CONSOLES_DIR)
                .join(file_name(console.id)),
        )
        .expect("the file is there");

        assert!(written.contains(&format!("id: {}", console.id)));
        assert!(!written.contains("line:"), "{written}");
        assert!(!written.contains("baud_rate"), "{written}");

        let _ = std::fs::remove_dir_all(root);
    }

    /// A console is its identity, so a file that does not say which console it
    /// holds holds none — and says so without taking the consoles beside it
    /// with it.
    #[test]
    fn a_file_that_names_no_console_is_no_console() {
        let (store, root) = store("nameless");
        let console = default_console();
        save(&store, &console).expect("the console is written");
        std::fs::write(
            store.config_dir().join(CONSOLES_DIR).join("old.yaml"),
            "name: Old\nprogram: sh\n",
        )
        .expect("the file is written");

        assert_eq!(load_all(&store), vec![console]);

        let _ = std::fs::remove_dir_all(root);
    }

    /// A file that leaves a switch out is read at the default of that switch:
    /// a console nobody said anything about does not come back by itself, and
    /// one nobody judged is trusted.
    #[test]
    fn a_console_that_leaves_the_switches_out_takes_their_defaults() {
        let (store, root) = store("defaults");
        let directory = store.config_dir().join(CONSOLES_DIR);
        std::fs::create_dir_all(&directory).expect("the directory is made");
        let id = ConsoleId::new();
        std::fs::write(
            directory.join(format!("{id}.yaml")),
            format!("id: {id}\nname: Old\nprogram: sh\nargs: []\n"),
        )
        .expect("the file is written");

        let loaded = load_all(&store);
        let console = loaded.first().expect("the console is read");
        assert!(
            !console.restart,
            "a console nobody asked to come back stays"
        );
        assert!(console.trusted, "and one nobody judged is trusted");

        let _ = std::fs::remove_dir_all(root);
    }

    /// A machine that has never run this program gets every shipped console,
    /// and one that has gets only what was shipped since: a console written
    /// again on every start would be a console that comes back the moment it is
    /// thrown away.
    #[test]
    fn a_shipped_console_is_written_once() {
        let (store, root) = store("shipped");

        let first = add_shipped(&store);
        assert_eq!(first.len(), shipped().len());
        assert!(first.iter().any(|console| console.name == "SSH"));

        assert!(
            add_shipped(&store).is_empty(),
            "nothing is written a second time"
        );
        assert_eq!(load_all(&store).len(), shipped().len());

        let _ = std::fs::remove_dir_all(root);
    }

    /// A shipped console the user renamed is the user's own: it carries the
    /// identity it was shipped with, so it is recognized whatever it is called
    /// now and is never written a second time.
    #[test]
    fn a_shipped_console_that_was_renamed_is_not_written_again() {
        let (store, root) = store("renamed");
        add_shipped(&store);

        let mut console = load_all(&store)
            .into_iter()
            .find(|console| console.name == "SSH")
            .expect("it is shipped");
        console.name = "The board".to_string();
        save(&store, &console).expect("the console is written");

        assert!(add_shipped(&store).is_empty());
        assert_eq!(load_all(&store).len(), shipped().len());

        let _ = std::fs::remove_dir_all(root);
    }

    /// What the shipped `ssh` console is: it names the machine it reaches
    /// rather than carrying one, so the window that asks for the values is what
    /// points it at a host, and what prints on it is another machine talking.
    #[test]
    fn the_shipped_ssh_console_asks_for_the_machine_it_reaches() {
        let ssh = shipped()
            .into_iter()
            .find(|console| console.name == "SSH")
            .expect("it is shipped");

        assert_eq!(
            ssh.program,
            "ssh -e none -o NumberOfPasswordPrompts=1 {remote_user}@{remote_host}"
        );
        assert!(ssh.directory.is_empty(), "it starts where the window did");
        assert!(!ssh.trusted, "what prints on it is not this machine");
        assert_eq!(ssh.variables(), ["remote_user", "remote_host"]);
    }

    /// A file is named after the identity, so two consoles that happen to carry
    /// one name are two files and neither is written over the other.
    #[test]
    fn two_consoles_of_one_name_keep_two_files() {
        let (store, root) = store("collision");

        for _ in 0..2 {
            let console = Console {
                name: "shell".to_string(),
                ..default_console()
            };
            save(&store, &console).expect("the console is written");
        }

        let consoles = load_all(&store);
        assert_eq!(
            consoles.len(),
            2,
            "a name says nothing about which console it is"
        );
        assert_ne!(consoles[0].id, consoles[1].id);
        assert_eq!(written(&store).len(), 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_program_line_is_read_the_way_a_shell_reads_one() {
        let console = |program: &str| Console {
            program: program.to_string(),
            ..default_console()
        };

        let (program, args) = console("ssh -p 2222 host")
            .command()
            .expect("the line reads");
        assert_eq!(program, "ssh");
        assert_eq!(args, vec!["-p", "2222", "host"]);

        let (program, args) = console("'/usr/local/bin/my shell' --login")
            .command()
            .expect("the line reads");
        assert_eq!(program, "/usr/local/bin/my shell");
        assert_eq!(args, vec!["--login"]);

        let (program, args) = console("  ").command().expect("the line reads");
        assert_eq!(
            program,
            zyt_pty::default_shell(),
            "an empty line is the shell"
        );
        assert!(args.is_empty());

        let mut with_args = console("sh");
        with_args.args = vec!["-c".to_string(), "echo hi".to_string()];
        let (_, args) = with_args.command().expect("the line reads");
        assert_eq!(
            args,
            vec!["-c", "echo hi"],
            "the arguments of the console follow the words of the line"
        );

        assert!(
            console("ssh 'host").command().is_err(),
            "a quote left open is an error, not a program with a strange name"
        );
    }

    /// The identity is a text and comes back from it, which is what a
    /// configuration file and a command line hand around.
    #[test]
    fn an_identity_comes_back_from_its_text() {
        let id = ConsoleId::new();
        let text = id.to_string();

        assert_eq!(text.parse::<ConsoleId>().expect("the text reads"), id);
        assert_ne!(ConsoleId::new(), id, "two consoles never share one");
        for text in ["", "Local shell", "usb:0403:6001:A1"] {
            assert!(text.parse::<ConsoleId>().is_err(), "{text}");
        }
    }

    /// Every shipped console carries the same identity on every machine: that
    /// is what makes one already written recognizable.
    #[test]
    fn a_shipped_console_carries_the_identity_it_was_shipped_with() {
        let first: Vec<ConsoleId> = shipped().into_iter().map(|console| console.id).collect();
        let again: Vec<ConsoleId> = shipped().into_iter().map(|console| console.id).collect();

        assert_eq!(first, again);
        assert_ne!(first[0], first[1]);
        assert_ne!(
            default_console().id,
            first[0],
            "a console made here is nobody's but this machine's"
        );
    }
}
