//! What the application remembers about one source, one file each.
//!
//! A source is a console or a serial port, and [`SourceKey`] is how either of
//! them is addressed: a console by the identity it carries, a device by the
//! identity that survives replugging. One type, because everything that
//! remembers something about a source — its memory, the values it was answered
//! with, the commands its shell marked — remembers it about either kind.
//!
//! What is remembered lives with the source: in the file of the console, or in
//! a file of its own below the `ports` directory of the configuration. Nothing
//! of it is shared, so two devices never talk each other's parameters — and the
//! two kinds do not talk each other's either, which is why a line belongs to
//! [`PortMemory`] and not to what both of them keep.

use crate::consoles::ConsoleId;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use zyt_config::ConfigStore;
use zyt_serial::{LineParams, PortId};

/// Directory holding the port files, below the configuration directory.
pub const PORTS_DIR: &str = "ports";

/// What a source is addressed by.
///
/// A console carries its own identity; a device is addressed by the one that
/// survives replugging. Its text form is what a configuration file carries and
/// what names the file: an identity on its own for a console, `usb:…` or
/// `path:…` for a device, so the two can never be read for one another.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum SourceKey {
    /// A console, by the identity it carries.
    Console(ConsoleId),
    /// A device, by the identity that survives replugging.
    Port(PortId),
}

impl SourceKey {
    /// The identity of the console this addresses, when it addresses one.
    pub fn console(&self) -> Option<ConsoleId> {
        match self {
            Self::Console(id) => Some(*id),
            Self::Port(_) => None,
        }
    }

    /// The identity of the device this addresses, when it addresses one.
    pub fn port(&self) -> Option<&PortId> {
        match self {
            Self::Console(_) => None,
            Self::Port(id) => Some(id),
        }
    }

    /// File name this key asks for, in the directories keyed by source.
    pub fn slug(&self) -> String {
        slug(&self.to_string())
    }
}

impl std::fmt::Display for SourceKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Console(id) => id.fmt(formatter),
            Self::Port(id) => id.fmt(formatter),
        }
    }
}

impl std::str::FromStr for SourceKey {
    type Err = AppError;

    /// Reads back what [`std::fmt::Display`] wrote.
    ///
    /// A device says which kind it is in the first word of its key, and an
    /// identity of a console is the one text that says nothing of the sort, so
    /// the two are told apart by what they are and not by a mark put in front
    /// of them.
    fn from_str(text: &str) -> Result<Self> {
        if let Ok(id) = text.parse::<ConsoleId>() {
            return Ok(Self::Console(id));
        }
        text.parse::<PortId>()
            .map(Self::Port)
            .map_err(|source| AppError::Port { source })
    }
}

impl From<SourceKey> for String {
    fn from(key: SourceKey) -> Self {
        key.to_string()
    }
}

impl TryFrom<String> for SourceKey {
    type Error = AppError;

    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

/// What the application remembers about any source, console or device alike.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceMemory {
    /// When the source was connected the last time.
    #[serde(default)]
    pub last_connected: Option<jiff::Timestamp>,
    /// Directory a transfer wrote into the last time.
    #[serde(default)]
    pub save_directory: Option<PathBuf>,
    /// Whether the plate of a command of several lines reads its carets as
    /// control codes on this source.
    ///
    /// It is of the source because it is a fact about what is at the other end:
    /// a bootloader is left with `^[` and a board is interrupted with `^C`
    /// every time it is worked on, and a shell somebody pastes text into wants
    /// a caret to stay a caret. The switch of the plate is where it is turned,
    /// and it is written down the moment it is.
    #[serde(default)]
    pub block_caret: bool,
    /// Script used with this source.
    #[serde(default)]
    pub script: Option<String>,
    /// Scripts offered for this source, by name.
    ///
    /// Empty is every script there is, which is what a source that was never
    /// asked about it means and what the menu shows until somebody narrows it.
    #[serde(default)]
    pub scripts: Vec<String>,
    /// Values a script asks this source for by name.
    ///
    /// A script asks for `remote_host` and the value stands here, so one
    /// script serves every device and each device answers for itself.
    ///
    /// It is a list and not a map because it is edited as one: a name is typed
    /// a letter at a time, and a map would move the row being written with
    /// every letter of it.
    #[serde(default)]
    pub variables: Vec<SourceVariable>,
}

/// What a device remembers besides that: what its line is opened with.
///
/// A speed and a parity are what a device is opened with. A console is a
/// program that was started, so none of this stands in its file — and nothing
/// of this can reach one, because the only way to it is a device identity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PortMemory {
    /// What this device remembers as any source does.
    #[serde(flatten)]
    pub source: SourceMemory,
    /// Line parameters used with this device.
    #[serde(default)]
    pub line: LineParams,
    /// What this side does with the two lines it drives on this device.
    ///
    /// It is remembered because it is asked of the device and not of the handle
    /// that happened to be open: a board held in reset is held in reset across a
    /// replug, across a disconnect, and across a run of this program.
    #[serde(default)]
    pub holds: zyt_serial::LineHolds,
    /// Which way a press on the button of each of those two lines holds it.
    ///
    /// It is of the device for the same reason the holds are: which level puts
    /// this board in reset is a fact about this board, and a direction carried
    /// over to whatever is plugged in next would hold a line the wrong way round.
    #[serde(default)]
    pub forces: crate::config::LineForces,
    /// Which of the lines this device shows in the status bar and in the plate.
    ///
    /// It is of the device because it is a fact about the device: which of its
    /// letters ever move is a thing about the adapter and the board at the end of
    /// it, and the answer for one is no answer for the next.
    #[serde(default)]
    pub shown_lines: crate::config::ShownLines,
    /// Speeds the menu of the line offers for this device.
    ///
    /// Empty is the shared list of the settings: a device that wants a speed
    /// nobody else does says so here, and every other device is left with the
    /// list it always had.
    #[serde(default)]
    pub baud_rates: Vec<u32>,
}

/// One value a source keeps under a name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceVariable {
    /// Name a command line asks for with `{<name>}`.
    pub name: String,
    /// What it stands for.
    #[serde(default)]
    pub value: String,
}

impl SourceMemory {
    /// The values by name, as a command line is resolved with them.
    ///
    /// A name written twice is answered by the first of them: a list is edited
    /// by hand and a name typed over another one is a moment, not an answer.
    pub fn variable_map(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        for variable in &self.variables {
            let name = variable.name.trim();
            if !name.is_empty() {
                values
                    .entry(name.to_string())
                    .or_insert_with(|| variable.value.clone());
            }
        }
        values
    }
}

/// True when a name may stand in a placeholder of its own.
///
/// Letters, digits, the hyphen and the underscore, and at least one of them.
/// The spelling is narrow on purpose: `{remote_host}` is a value a source
/// keeps, and anything else in braces is not a name and is left standing.
pub fn is_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|point| point.is_ascii_alphanumeric() || point == '-' || point == '_')
}

/// The names a text asks a source for, in the order it asks and each once.
pub fn variable_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = text;

    while let Some(open) = rest.find('{') {
        rest = &rest[open..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let name = &rest[1..close];
        if is_variable_name(name) && !names.iter().any(|kept| kept == name) {
            names.push(name.to_string());
        }
        rest = &rest[close + 1..];
    }
    names
}

/// One text with every `{<name>}` replaced by what the source keeps under that
/// name.
///
/// Nothing is quoted. A command line of a transfer profile goes to a shell and
/// is quoted on the way, but a console is not run through one — its line is
/// read into words and the words go to the program — so a value quoted here
/// would arrive with the quotes in it.
///
/// A name the source has not got is left standing as it was written: a console
/// has to start, and a line that shows the name it wanted says more than one
/// with a hole where the name was.
pub fn expand(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for name in variable_names(text) {
        if let Some(value) = values.get(&name) {
            out = out.replace(&format!("{{{name}}}"), value);
        }
    }
    out
}

/// One device as it lives on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortFile {
    /// Identity of the device, which is also the name of the file.
    pub key: PortId,
    /// What is remembered about it.
    pub memory: PortMemory,
}

/// Reads every port file, keyed the way the application addresses a device.
pub fn load_ports(store: &ConfigStore) -> BTreeMap<PortId, PortMemory> {
    let Ok(entries) = std::fs::read_dir(store.path(PORTS_DIR)) else {
        return BTreeMap::new();
    };

    let mut ports = BTreeMap::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        let Some(file) = yaml_name(&path) else {
            continue;
        };
        match store.load::<PortFile>(&entry(&file)) {
            Ok(Some(port)) => {
                ports.insert(port.key, port.memory);
            }
            Ok(None) => {}
            Err(error) => log::warn!("port {file}: {error}"),
        }
    }
    ports
}

/// Writes the memory of one device.
pub fn save_port(store: &ConfigStore, key: &PortId, memory: &PortMemory) -> Result<()> {
    let port = PortFile {
        key: key.clone(),
        memory: memory.clone(),
    };
    store
        .save(&entry(&file_name(key)), &port)
        .map_err(AppError::from)
}

/// File one device is written to, named after its identity.
fn file_name(key: &PortId) -> String {
    format!("{}.yaml", slug(&key.to_string()))
}

fn entry(file: &str) -> String {
    format!("{PORTS_DIR}/{file}")
}

fn yaml_name(path: &std::path::Path) -> Option<String> {
    if !path.is_file()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("yaml"))
    {
        return None;
    }
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// File name a text asks for: itself, with everything a path cannot carry
/// replaced, and never empty.
fn slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
            slug.push(character);
        } else {
            slug.push('_');
        }
    }
    let slug = slug.trim().trim_matches('.').to_string();
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(case: &str) -> (ConfigStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("zyterm-ports-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store =
            ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("locks"));
        (store, root)
    }

    /// A device identity, written the way a file carries one.
    fn device(key: &str) -> PortId {
        key.parse().expect("the identity reads")
    }

    /// A port file written before a setting existed reads as the default of it, and
    /// a hold nobody wrote is the driver's.
    ///
    /// A device whose file was written by an older run must not come back with a
    /// line held down that nobody asked for — a board held in reset by a file this
    /// program wrote before it knew how to hold one would be a board that does not
    /// start.
    #[test]
    fn a_port_file_that_names_no_hold_leaves_both_lines_to_the_driver() {
        let file: PortFile = serde_yaml_ng::from_str(
            "key: path:/dev/ttyS0\nmemory:\n  line:\n    baud_rate: 9600\n    data_bits: Eight\n    parity: None\n    stop_bits: One\n    flow_control: None\n",
        )
        .expect("a file written before the holds existed still parses");

        assert_eq!(file.memory.holds, zyt_serial::LineHolds::default());
        assert_eq!(
            file.memory.shown_lines,
            crate::config::ShownLines::default(),
            "and a file naming no letters shows the ones a row begins with"
        );
        assert_eq!(file.memory.holds.rts, zyt_serial::LineHold::Auto);
        assert_eq!(file.memory.holds.dtr, zyt_serial::LineHold::Auto);
        assert_eq!(
            file.memory.forces,
            crate::config::LineForces::default(),
            "and a file naming no direction holds both lines down when it is asked to"
        );
        assert_eq!(file.memory.line.baud_rate, 9600);
        assert!(
            !file.memory.source.block_caret,
            "and a file naming no switch reads a caret as the character it is"
        );
    }

    /// The switch of the carets is of the source, so it goes to the file of
    /// that device and comes back from it.
    #[test]
    fn a_device_keeps_the_switch_of_the_carets() {
        let (store, root) = store("carets");
        let memory = PortMemory {
            source: SourceMemory {
                block_caret: true,
                ..SourceMemory::default()
            },
            ..PortMemory::default()
        };

        save_port(&store, &device("path:/dev/ttyS0"), &memory).expect("the device is written");

        let ports = load_ports(&store);
        assert!(
            ports
                .get(&device("path:/dev/ttyS0"))
                .expect("the file is read")
                .source
                .block_caret
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_device_keeps_its_own_file_and_its_own_line() {
        let (store, root) = store("round-trip");
        let first = PortMemory {
            line: LineParams {
                baud_rate: 9600,
                ..LineParams::default()
            },
            ..PortMemory::default()
        };
        let second = PortMemory {
            line: LineParams {
                baud_rate: 921_600,
                ..LineParams::default()
            },
            ..PortMemory::default()
        };

        save_port(&store, &device("usb:0403:6001:A1"), &first).expect("the device is written");
        save_port(&store, &device("path:/dev/ttyS0"), &second).expect("the device is written");

        let ports = load_ports(&store);
        assert_eq!(ports.get(&device("usb:0403:6001:A1")), Some(&first));
        assert_eq!(ports.get(&device("path:/dev/ttyS0")), Some(&second));

        let _ = std::fs::remove_dir_all(root);
    }

    /// A console is a program that was started and a device is a line that was
    /// opened, so neither file carries what only the other has: nothing of a
    /// line can be written down for a console, because the only way to write
    /// one down asks for a device identity.
    #[test]
    fn what_a_device_keeps_of_its_line_is_not_what_every_source_keeps() {
        let (store, root) = store("apart");
        let memory = PortMemory {
            line: LineParams {
                baud_rate: 9600,
                ..LineParams::default()
            },
            baud_rates: vec![9600],
            holds: zyt_serial::LineHolds {
                rts: zyt_serial::LineHold::Down,
                dtr: zyt_serial::LineHold::Auto,
            },
            forces: crate::config::LineForces {
                rts: crate::config::LineForce::Down,
                dtr: crate::config::LineForce::Up,
            },
            shown_lines: crate::config::ShownLines {
                ring: true,
                ..crate::config::ShownLines::default()
            },
            source: SourceMemory {
                script: Some("zmodem".to_string()),
                ..SourceMemory::default()
            },
        };

        save_port(&store, &device("path:/dev/ttyS0"), &memory).expect("the device is written");
        let written = std::fs::read_to_string(store.path(&format!(
            "{PORTS_DIR}/{}",
            file_name(&device("path:/dev/ttyS0"))
        )))
        .expect("the file is there");

        assert!(written.contains("baud_rate: 9600"));
        assert!(written.contains("script: zmodem"));
        assert!(
            written.contains("rts: Down"),
            "the hold is written down too"
        );
        assert!(
            written.contains("dtr: Up"),
            "and which way a press holds the other line"
        );
        assert_eq!(
            load_ports(&store).get(&device("path:/dev/ttyS0")),
            Some(&memory)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_values_of_a_source_answer_by_name_and_the_first_of_a_name_wins() {
        let memory = SourceMemory {
            variables: vec![
                SourceVariable {
                    name: " remote_host ".to_string(),
                    value: "192.168.1.1".to_string(),
                },
                SourceVariable {
                    name: "remote_host".to_string(),
                    value: "10.0.0.1".to_string(),
                },
                SourceVariable {
                    name: "  ".to_string(),
                    value: "nameless".to_string(),
                },
            ],
            ..SourceMemory::default()
        };

        let values = memory.variable_map();
        assert_eq!(
            values.get("remote_host").map(String::as_str),
            Some("192.168.1.1")
        );
        assert_eq!(values.len(), 1, "a row with no name answers nothing");
    }

    #[test]
    fn what_a_source_keeps_survives_a_round_trip() {
        let (store, root) = store("variables");
        let memory = PortMemory {
            baud_rates: vec![115_200, 921_600],
            source: SourceMemory {
                scripts: vec!["shell-driven-scp".to_string()],
                variables: vec![SourceVariable {
                    name: "remote_user".to_string(),
                    value: "root".to_string(),
                }],
                ..SourceMemory::default()
            },
            ..PortMemory::default()
        };

        save_port(&store, &device("usb:0403:6001:A1"), &memory).expect("the device is written");
        assert_eq!(
            load_ports(&store).get(&device("usb:0403:6001:A1")),
            Some(&memory)
        );

        let _ = std::fs::remove_dir_all(root);
    }

    /// A key a file leaves out is read at the default of that setting: a
    /// device that was never asked about a speed list has none, and one that
    /// was never asked about a script offers them all.
    #[test]
    fn a_file_that_leaves_a_key_out_reads_at_its_default() {
        let (store, root) = store("defaults");
        std::fs::create_dir_all(store.path(PORTS_DIR)).expect("the directory is made");
        std::fs::write(
            store.path(&format!("{PORTS_DIR}/old.yaml")),
            concat!(
                "key: 'path:/dev/ttyS0'\n",
                "memory:\n",
                "  line:\n",
                "    baud_rate: 9600\n",
                "    data_bits: Eight\n",
                "    parity: None\n",
                "    stop_bits: One\n",
                "    flow_control: None\n",
                "  script: zmodem\n",
            ),
        )
        .expect("the file is written");

        let ports = load_ports(&store);
        let memory = ports.get(&device("path:/dev/ttyS0")).expect("it is read");
        assert_eq!(memory.line.baud_rate, 9600);
        assert_eq!(memory.source.script.as_deref(), Some("zmodem"));
        assert!(memory.source.variables.is_empty());
        assert!(memory.source.scripts.is_empty(), "no list is every script");
        assert!(memory.baud_rates.is_empty(), "no list is the shared one");

        let _ = std::fs::remove_dir_all(root);
    }

    /// The key of a source is a text and comes back from it, and the two kinds
    /// are never read for one another.
    #[test]
    fn a_key_comes_back_from_its_text() {
        let console = SourceKey::Console(crate::consoles::ConsoleId::new());
        let keys = [
            console.clone(),
            SourceKey::Port(device("usb:0403:6001:A1")),
            SourceKey::Port(device("usb:0403:6001:A1:B2")),
            SourceKey::Port(device("path:/dev/ttyS0")),
        ];

        for key in keys {
            let text = key.to_string();
            assert_eq!(text.parse::<SourceKey>().expect("it reads"), key, "{text}");
        }

        assert!(console.console().is_some() && console.port().is_none());
        assert!("neither one nor the other".parse::<SourceKey>().is_err());
    }

    #[test]
    fn a_text_says_which_names_it_asks_for_and_wears_their_values() {
        let line = "ssh {user}@{host} -p {port} {host} {not a name}";
        assert_eq!(variable_names(line), ["user", "host", "port"]);

        let mut values = BTreeMap::new();
        values.insert("user".to_string(), "root".to_string());
        values.insert("host".to_string(), "board.lan".to_string());

        assert_eq!(
            expand(line, &values),
            "ssh root@board.lan -p {port} board.lan {not a name}",
            "a name nothing answers stands as it was written, and nothing is quoted"
        );
        assert_eq!(expand("plain", &values), "plain");
    }

    #[test]
    fn a_key_that_is_no_file_name_still_gets_a_file() {
        assert_eq!(slug("path:/dev/ttyUSB0"), "path__dev_ttyUSB0");
        assert_eq!(slug("usb:0403:6001:A1"), "usb_0403_6001_A1");
    }
}
