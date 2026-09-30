//! Transfer profiles: which command lines run for which direction.

use crate::error::{Result, XferError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Placeholder replaced by the file path.
pub const FILE_PLACEHOLDER: &str = "{>file}";
/// Placeholder replaced by the directory path.
pub const DIRECTORY_PLACEHOLDER: &str = "{>directory}";
/// Placeholder replaced by every file path, separated by spaces.
pub const FILES_PLACEHOLDER: &str = "{>files}";
/// Placeholder replaced by every directory path, separated by spaces.
pub const DIRECTORIES_PLACEHOLDER: &str = "{>directories}";
/// Placeholder replaced by the name of the target without its directory.
pub const FILENAME_PLACEHOLDER: &str = "{:filename}";
/// Placeholder replaced by the name of the target without its extension.
pub const STEM_PLACEHOLDER: &str = "{:stem}";
/// Placeholder replaced by the extension of the target, without the dot.
pub const SUFFIX_PLACEHOLDER: &str = "{:suffix}";
/// True when a name may stand in a placeholder of its own.
///
/// Letters, digits, the hyphen and the underscore, and at least one of them.
/// The spelling is narrow on purpose: `{remote_host}` is the value a source
/// keeps, `{>file}` is what the transfer carries, `{:filename}` is a part of
/// its name and `{}` is the program's own — and none of those is a name, so
/// what a brace holds says by itself which of the four it is.
pub fn is_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|point| point.is_ascii_alphanumeric() || point == '-' || point == '_')
}

/// The names a text asks a source for, in the order it asks and each once.
pub fn variable_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, _, _) in placeholders(text) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Every placeholder of a name in a text: the name, where it starts and where
/// it ends.
fn placeholders(text: &str) -> Vec<(String, usize, usize)> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;

    while let Some(start) = text[at..].find('{') {
        let start = at + start;
        let Some(end) = text[start..].find('}') else {
            break;
        };
        let end = start + end;
        let name = &text[start + 1..end];
        if is_variable_name(name) {
            found.push((name.to_string(), start, end + 1));
        }
        at = end + 1;
        if at >= bytes.len() {
            break;
        }
    }
    found
}

/// Delay between the remote command and the local command, in milliseconds.
pub const DEFAULT_DELAY_MS: u64 = 700;

/// Direction of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Local file goes to the device.
    Send,
    /// File comes from the device.
    Receive,
}

/// What a command line needs before it can run.
///
/// The order is how much a kind asks for, because a direction takes the
/// stronger of its two lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetKind {
    /// The command line takes no path.
    None,
    /// The command line takes one file.
    File,
    /// The command line takes any number of files.
    Files,
    /// The command line takes one directory.
    Directory,
    /// The command line takes any number of directories.
    Directories,
}

impl TargetKind {
    /// True when the user picks more than one path for this kind.
    pub fn is_multiple(self) -> bool {
        matches!(self, Self::Files | Self::Directories)
    }

    /// True when the paths of this kind are directories.
    pub fn is_directory(self) -> bool {
        matches!(self, Self::Directory | Self::Directories)
    }
}

/// Path handed to the command lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// Nothing is passed.
    None,
    /// A file path.
    File(&'a Path),
    /// Any number of file paths.
    Files(&'a [PathBuf]),
    /// A directory path.
    Directory(&'a Path),
    /// Any number of directory paths.
    Directories(&'a [PathBuf]),
}

impl Target<'_> {
    /// Kind of this target.
    pub fn kind(&self) -> TargetKind {
        match self {
            Self::None => TargetKind::None,
            Self::File(_) => TargetKind::File,
            Self::Files(_) => TargetKind::Files,
            Self::Directory(_) => TargetKind::Directory,
            Self::Directories(_) => TargetKind::Directories,
        }
    }

    /// Paths of this target, in the order the user gave them.
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Self::Files(paths) | Self::Directories(paths) => paths,
            _ => &[],
        }
    }

    /// Path of this target, when it names exactly one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::File(path) | Self::Directory(path) => Some(path),
            Self::Files(paths) | Self::Directories(paths) => paths.first().map(PathBuf::as_path),
            Self::None => None,
        }
    }
}

/// One command line.
///
/// A local line is run by the shell of this machine, a remote line is typed
/// into the console of the device. Both may use shell syntax, for example
/// `cd {>directory} && rb -vv` or `cat > {:filename}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandLine(pub String);

impl CommandLine {
    /// Command line from a string slice.
    pub fn new(line: &str) -> Self {
        Self(line.to_string())
    }

    /// Text of the command line.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the line runs nothing.
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// What the line needs before it can run.
    pub fn target_kind(&self) -> TargetKind {
        if self.0.contains(DIRECTORIES_PLACEHOLDER) {
            TargetKind::Directories
        } else if self.0.contains(DIRECTORY_PLACEHOLDER) {
            TargetKind::Directory
        } else if self.0.contains(FILES_PLACEHOLDER) {
            TargetKind::Files
        } else if self.0.contains(FILE_PLACEHOLDER) || self.has_name_placeholder() {
            TargetKind::File
        } else {
            TargetKind::None
        }
    }

    /// Names of the values of the source this line asks for, in the order it
    /// asks for them and each of them once.
    pub fn variables(&self) -> Vec<String> {
        variable_names(&self.0)
    }

    /// Command line with every placeholder replaced by a quoted value.
    ///
    /// A placeholder that stands for several paths becomes them all, each
    /// quoted on its own and separated by a space, so the line stays one
    /// command with several arguments.
    ///
    /// `variables` are the values the source keeps. A name the line asks for
    /// that is not there, or is there and empty, stops the transfer:
    /// `user@:{}` is a destination that would be acted on rather than refused,
    /// and the one moment to say so is before anything runs. Each value is
    /// quoted for the shell like everything else, so a value beside another
    /// word — `{user}@{host}` — still comes out as the one word the
    /// shell puts back together.
    pub fn resolve(
        &self,
        profile: &str,
        target: Target<'_>,
        variables: &std::collections::BTreeMap<String, String>,
    ) -> Result<String> {
        if self.is_empty() {
            return Err(XferError::EmptyCommand {
                profile: profile.to_string(),
            });
        }

        let expected = self.target_kind();
        if expected != TargetKind::None && expected != target.kind() {
            return Err(XferError::TargetMismatch {
                profile: profile.to_string(),
                expected,
                given: target.kind(),
            });
        }

        let line = self.with_variables(profile, variables)?;
        let paths = match target {
            Target::None => return Ok(line),
            Target::File(path) | Target::Directory(path) => vec![path],
            Target::Files(paths) | Target::Directories(paths) => {
                paths.iter().map(PathBuf::as_path).collect()
            }
        };

        let placeholder = match target.kind() {
            TargetKind::Directories => DIRECTORIES_PLACEHOLDER,
            TargetKind::Directory => DIRECTORY_PLACEHOLDER,
            TargetKind::Files => FILES_PLACEHOLDER,
            _ => FILE_PLACEHOLDER,
        };

        let line = line
            .replace(
                placeholder,
                &quoted_list(&paths, |path| path.to_string_lossy().to_string()),
            )
            .replace(
                FILENAME_PLACEHOLDER,
                &quoted_list(&paths, |path| name_part(path.file_name())),
            )
            .replace(
                STEM_PLACEHOLDER,
                &quoted_list(&paths, |path| name_part(path.file_stem())),
            )
            .replace(
                SUFFIX_PLACEHOLDER,
                &quoted_list(&paths, |path| name_part(path.extension())),
            );
        Ok(line)
    }

    /// The line with every value of the source put in, or the name of the
    /// first one that is not there.
    fn with_variables(
        &self,
        profile: &str,
        variables: &std::collections::BTreeMap<String, String>,
    ) -> Result<String> {
        let mut line = String::with_capacity(self.0.len());
        let mut at = 0;

        for (name, start, end) in placeholders(&self.0) {
            let value = variables
                .get(&name)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| XferError::UnsetVariable {
                    profile: profile.to_string(),
                    name: name.clone(),
                })?;

            line.push_str(&self.0[at..start]);
            line.push_str(&quote_for_shell(value));
            at = end;
        }
        line.push_str(&self.0[at..]);
        Ok(line)
    }

    fn has_name_placeholder(&self) -> bool {
        self.0.contains(FILENAME_PLACEHOLDER)
            || self.0.contains(STEM_PLACEHOLDER)
            || self.0.contains(SUFFIX_PLACEHOLDER)
    }
}

impl std::fmt::Display for CommandLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One part of a path as text, empty when the path has none.
fn name_part(part: Option<&std::ffi::OsStr>) -> String {
    part.map(|part| part.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The values of every path, each quoted on its own, separated by a space.
fn quoted_list(paths: &[&Path], value: impl Fn(&Path) -> String) -> String {
    paths
        .iter()
        .map(|path| quote_for_shell(&value(path)))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Quotes a value for the shell of the platform, so a path with a space or a
/// quote in it stays one word.
pub fn quote_for_shell(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

/// One command line with the delay before it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandStep {
    /// Milliseconds to wait after the transfer started before this line runs.
    #[serde(default)]
    pub delay_ms: u64,
    /// The line itself.
    #[serde(default)]
    pub line: CommandLine,
}

impl CommandStep {
    /// Step that runs `line` after `delay_ms` milliseconds.
    pub fn new(delay_ms: u64, line: &str) -> Self {
        Self {
            delay_ms,
            line: CommandLine::new(line),
        }
    }

    /// True when there is nothing to run.
    pub fn is_empty(&self) -> bool {
        self.line.is_empty()
    }

    /// Delay before this line runs.
    pub fn delay(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.delay_ms)
    }
}

/// Name of the profile that copies over the network into the directory the
/// device stands in.
pub const SCP_TO_REMOTE_PWD: &str = "SCP to remote PWD";

/// Text of the key sent when nothing is sent.
pub const FINISH_NONE: &str = "";
/// Every key the interface offers as a preset.
pub const FINISH_PRESETS: &[&str] = &["esc", "ctrl+c", "ctrl+d", "enter"];

/// The two steps of one direction.
///
/// Both steps start from the moment the transfer starts, each after its own
/// delay, so a profile decides itself which side goes first and how long the
/// other one waits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferCommands {
    /// Line run here by the shell of this machine.
    pub local: CommandStep,
    /// Line typed into the console of the device.
    pub remote: CommandStep,
    /// Key sent to the device once the transfer is over, empty for none.
    #[serde(default)]
    pub finish: String,
}

impl TransferCommands {
    /// Pair of steps with their delays and no key at the end.
    pub fn new(local: CommandStep, remote: CommandStep) -> Self {
        Self {
            local,
            remote,
            finish: String::new(),
        }
    }

    /// The same pair with a key sent once the transfer is over.
    pub fn finished_by(mut self, finish: &str) -> Self {
        self.finish = finish.to_string();
        self
    }

    /// Bytes of the key sent when the transfer is over.
    pub fn finish_bytes(&self, profile: &str) -> Result<Vec<u8>> {
        finish_bytes(profile, &self.finish)
    }

    /// Text of that key for a message, when there is one.
    pub fn finish_label(&self) -> Option<String> {
        let text = self.finish.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }

    /// True when this direction can run at all.
    pub fn is_available(&self) -> bool {
        !self.local.is_empty()
    }

    /// Names of the values of the source both lines together ask for.
    pub fn variables(&self) -> Vec<String> {
        let mut names = self.local.line.variables();
        for name in self.remote.line.variables() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// What both lines together need before they can run.
    pub fn target_kind(&self) -> TargetKind {
        self.local
            .line
            .target_kind()
            .max(self.remote.line.target_kind())
    }
}

/// Turns the text form of a key into the bytes sent to the device.
///
/// Accepts the names `esc`, `escape`, `enter`, `cr`, `lf`, `tab`, the form
/// `ctrl+<char>`, the escapes `\r`, `\n`, `\t`, `\e`, `\\` and `\xNN`,
/// and takes anything else as literal text.
pub fn finish_bytes(profile: &str, text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let lower = trimmed.to_ascii_lowercase();
    let named = match lower.as_str() {
        "esc" | "escape" => Some(0x1b),
        "enter" | "return" | "cr" => Some(0x0d),
        "lf" | "newline" => Some(0x0a),
        "tab" => Some(0x09),
        "backspace" => Some(0x7f),
        _ => None,
    };
    if let Some(byte) = named {
        return Ok(vec![byte]);
    }

    if let Some(rest) = lower.strip_prefix("ctrl+") {
        let mut characters = rest.chars();
        let (Some(character), None) = (characters.next(), characters.next()) else {
            return Err(XferError::InvalidFinishKey {
                profile: profile.to_string(),
                input: trimmed.to_string(),
            });
        };
        return control_byte(character)
            .map(|byte| vec![byte])
            .ok_or_else(|| XferError::InvalidFinishKey {
                profile: profile.to_string(),
                input: trimmed.to_string(),
            });
    }

    unescape(profile, trimmed)
}

/// Control code of one character, as a terminal sends it with Control held.
fn control_byte(character: char) -> Option<u8> {
    match character {
        'a'..='z' => Some(character as u8 - b'a' + 1),
        '@' | ' ' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

/// Resolves the backslash escapes of a literal key.
fn unescape(profile: &str, text: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut characters = text.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            continue;
        }

        let invalid = || XferError::InvalidFinishKey {
            profile: profile.to_string(),
            input: text.to_string(),
        };
        match characters.next().ok_or_else(invalid)? {
            'r' => bytes.push(0x0d),
            'n' => bytes.push(0x0a),
            't' => bytes.push(0x09),
            'e' => bytes.push(0x1b),
            '0' => bytes.push(0x00),
            '\\' => bytes.push(b'\\'),
            'x' => {
                let high = characters.next().ok_or_else(invalid)?;
                let low = characters.next().ok_or_else(invalid)?;
                let digits: String = [high, low].into_iter().collect();
                let byte = u8::from_str_radix(&digits, 16).map_err(|_| invalid())?;
                bytes.push(byte);
            }
            _ => return Err(invalid()),
        }
    }

    Ok(bytes)
}

/// True, which is what a profile that does not say means.
///
/// Every shipped profile is on the line, so a file that leaves the field out
/// is read as the kind it most likely holds.
fn yes() -> bool {
    true
}

/// A named pair of directions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferProfile {
    /// Name shown in the interface.
    pub name: String,
    /// True when the program holds the line of the terminal.
    ///
    /// A transfer program of the classic kind — the modems, `cat`, the shell
    /// transfer — is the far end of a conversation the device is holding on
    /// its own console: it reads what the device sends and answers on the same
    /// channel, block by block. So the console is taken away from the terminal
    /// for as long as it runs and handed to the program, which is why one of
    /// them runs at a time and why the device needs a command of its own
    /// before it starts.
    ///
    /// A program that says no is not on the line at all — it reaches the device
    /// over a network, or it does something with the file here. It is given no
    /// input, its output goes to a file, and any number of them run beside each
    /// other while the terminal goes on being a terminal.
    #[serde(default = "yes")]
    pub hold_line: bool,
    /// Commands that send a file to the device.
    pub send: TransferCommands,
    /// Commands that receive a file from the device.
    pub receive: TransferCommands,
}

impl TransferProfile {
    /// Commands of the given direction.
    pub fn commands(&self, direction: Direction) -> &TransferCommands {
        match direction {
            Direction::Send => &self.send,
            Direction::Receive => &self.receive,
        }
    }

    /// The line typed into the console of the device before the transfer, when
    /// this profile has one.
    ///
    /// Only a profile on the line has one: the console of a profile beside the
    /// line belongs to the terminal the whole time, and a command typed into it
    /// would land in whatever the user is doing there.
    pub fn remote(&self, direction: Direction) -> Option<&CommandStep> {
        let step = &self.commands(direction).remote;
        (self.hold_line && !step.is_empty()).then_some(step)
    }

    /// Names of the values of the source every line of this profile asks for.
    ///
    /// The line typed into the device counts only while the profile holds the
    /// line, because that is the only time it is typed at all.
    pub fn variables(&self) -> Vec<String> {
        let mut names = Vec::new();
        for direction in [Direction::Send, Direction::Receive] {
            let commands = self.commands(direction);
            let asked = if self.hold_line {
                commands.variables()
            } else {
                commands.local.line.variables()
            };
            for name in asked {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The key sent to the device once the transfer is over, when this profile
    /// sends one, for the same reason.
    pub fn finish(&self, direction: Direction) -> &str {
        if self.hold_line {
            &self.commands(direction).finish
        } else {
            FINISH_NONE
        }
    }
}

/// Profiles shipped with the application.
///
/// `SCP to remote PWD` is the one that never carries a byte over the line: it
/// holds the line only long enough to ask the device where it stands — that is
/// `sh-xfer pwd-exec`, and `{}` is the answer — and then `scp` copies over the
/// network into that directory. It is on the line all the same, because the
/// question is asked on the console of the device, and it asks the source for
/// `remote_user` and `remote_host`, which are set per connection: one profile
/// serves every board and each board answers for itself.
///
/// `Shell Transfer` stands first because it is the one that needs nothing on
/// the device but a shell: the others want a program installed at the far end,
/// and the first of a list is what somebody tries first.
///
/// Sending asks the device first and pushes after a moment; receiving starts
/// the program here first. `Shell Transfer` is the exception to that too: it
/// types no remote line at all and carries what the directory of the device
/// holds. It says `--all`, `--mode base64`, `--chunk-size`
/// and `--digest auto` out loud rather than leaning on a default, so the line
/// in the settings says what it does and a slow line can be given a smaller
/// chunk — or the raw transport, on a line that carries it — by editing it
/// there.
///
/// Every profile that runs `sh-xfer` ends in `enter`: that program leaves the
/// shell of the device with a line of its own to read, and the prompt comes
/// back only once that line is sent. `Cat file` ends in `ctrl+c`, because
/// `cat` reading a line has to be told the line is over.
pub fn default_profiles() -> Vec<TransferProfile> {
    vec![
        TransferProfile {
            name: "Shell Transfer".to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(
                    0,
                    "sh-xfer put --mode base64 --chunk-size 2048 --digest auto --size-check on {>files}",
                ),
                CommandStep::default(),
            )
            .finished_by("enter"),
            receive: TransferCommands::new(
                CommandStep::new(
                    0,
                    "sh-xfer get --all --mode base64 --chunk-size 2048 --digest auto --size-check on -C {>directory}",
                ),
                CommandStep::default(),
            )
            .finished_by("enter"),
        },
        TransferProfile {
            name: "ymodem".to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(DEFAULT_DELAY_MS, "sb -vv {>file}"),
                CommandStep::new(0, "rb"),
            ),
            receive: TransferCommands::new(
                CommandStep::new(0, "cd {>directory} && rb -vv"),
                CommandStep::default(),
            ),
        },
        TransferProfile {
            name: "xmodem".to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(DEFAULT_DELAY_MS, "sx -vv {>file}"),
                CommandStep::new(0, "rx {:filename}"),
            ),
            receive: TransferCommands::new(
                CommandStep::new(0, "rx -vv {>file}"),
                CommandStep::default(),
            ),
        },
        TransferProfile {
            name: "zmodem".to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(DEFAULT_DELAY_MS, "sz -vv -b {>file}"),
                CommandStep::new(0, "rz -y"),
            ),
            receive: TransferCommands::new(
                CommandStep::new(0, "cd {>directory} && rz -vv -b -E"),
                CommandStep::new(DEFAULT_DELAY_MS, "sz"),
            ),
        },
        TransferProfile {
            name: SCP_TO_REMOTE_PWD.to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(
                    0,
                    "sh-xfer pwd-exec scp -- -v -O {>files} {remote_user}@{remote_host}:{}",
                ),
                CommandStep::default(),
            )
            .finished_by("enter"),
            receive: TransferCommands::default(),
        },
        TransferProfile {
            name: "Cat file".to_string(),
            hold_line: true,
            send: TransferCommands::new(
                CommandStep::new(DEFAULT_DELAY_MS, "cat {>file}"),
                CommandStep::new(0, "cat > {:filename}"),
            )
            .finished_by("ctrl+c"),
            receive: TransferCommands::default(),
        },
    ]
}
