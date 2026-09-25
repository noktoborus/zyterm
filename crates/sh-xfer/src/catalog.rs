//! The set of scripts one mode of transfer sends.
//!
//! A mode is a whole catalogue, not a flag read halfway through a command: the
//! scripts differ, the probe asks for what those scripts need, and the body of
//! a chunk is made the way those scripts will read it. Everything else about a
//! transfer is the same either way.

use crate::script::Command;
use base64::Engine;

/// How many bytes become one line of base64.
///
/// Fifty-seven bytes are seventy-six characters, which is what `base64` itself
/// writes and what every terminal will take. A terminal in its usual mode
/// holds one line and no more: what does not fit is dropped where it stands,
/// with no error and no gap — the reader simply never sees it. POSIX promises
/// 255 characters, Linux gives 4096, and a line longer than the promise is a
/// file that arrives wrong on a system that keeps only the promise.
const LINE_BYTES: usize = 57;
/// How many characters those bytes become, without the newline that ends them.
const LINE_CHARS: usize = 76;
/// The word that closes the here-document a chunk of base64 travels in.
const HEREDOC: &str = "SHXFER_EOF";

/// Which set of scripts a transfer sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// base64, which any line carries whatever its settings.
    Base64,
    /// The bytes themselves, with the line switched to binary for as long as
    /// they travel.
    Raw,
}

impl Mode {
    /// Name of the mode as the command line spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Base64 => "base64",
            Self::Raw => "raw",
        }
    }

    /// The scripts this mode sends.
    pub fn catalog(self) -> ModeCatalog {
        match self {
            Self::Base64 => ModeCatalog::base64(),
            Self::Raw => ModeCatalog::raw(),
        }
    }

    /// How many bytes of a file one chunk carries, for a chunk of `on_wire`
    /// bytes of the line.
    ///
    /// base64 is cut to whole lines, so no line is ever longer than a terminal
    /// will keep and no line of a body is ever half of one.
    pub fn slice(self, on_wire: usize) -> u64 {
        match self {
            Self::Raw => on_wire as u64,
            Self::Base64 => ((on_wire / (LINE_CHARS + 1)).max(1) * LINE_BYTES) as u64,
        }
    }

    /// The bytes of a chunk as the scripts of this mode will read them.
    ///
    /// This is the one place the two modes differ outside their scripts: raw
    /// sends the bytes, base64 sends them encoded and cut into lines.
    pub fn body(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::Raw => bytes.to_vec(),
            Self::Base64 => {
                let lines = bytes.len().div_ceil(LINE_BYTES);
                let mut text = String::with_capacity(lines * (LINE_CHARS + 1));
                for piece in bytes.chunks(LINE_BYTES) {
                    text.push_str(&base64::engine::general_purpose::STANDARD.encode(piece));
                    text.push('\n');
                }
                text.into_bytes()
            }
        }
    }
}

/// How the body of a chunk reaches the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    /// The command counts the body out, so it says when the line is ready and
    /// the client then writes exactly the bytes that were counted.
    Counted,
    /// The body travels in a here-document, which the shell reads itself. The
    /// client closes it with `delimiter` and asks for the reply with `end`.
    ///
    /// It is the one form that crosses a pipe: a shell reading its commands
    /// from one reads ahead, and whatever it took would never reach a reader
    /// waiting behind it.
    Document {
        /// The word that closes the here-document.
        delimiter: &'static str,
        /// The command that asks for the reply once it is closed.
        end: Command,
    },
}

/// Every script one mode sends, one field per command of the protocol.
#[derive(Debug, Clone, Copy)]
pub struct ModeCatalog {
    /// What the device is asked at the start: whether it has what this mode
    /// cannot do without.
    pub hello: Command,
    /// Where the device stands.
    pub pwd: Command,
    /// The path a directory really is, every symbolic link resolved.
    pub canonical: Command,
    /// What a path is.
    pub kind: Command,
    /// How many bytes a file holds.
    pub size: Command,
    /// What a directory holds.
    pub list: Command,
    /// Makes a directory, and every directory above it.
    pub make_directory: Command,
    /// Makes a file of no bytes, ready to be written chunk by chunk.
    pub create: Command,
    /// What a program of the device says the sum of a file is.
    pub digest: Command,
    /// Whether the device can say an md5 sum.
    pub probe_md5sum: Command,
    /// Whether the device can say a sha1 sum.
    pub probe_sha1sum: Command,
    /// Whether the device can say a sha256 sum.
    pub probe_sha256sum: Command,
    /// One chunk of a file being read.
    pub retrieve: Command,
    /// One chunk of a file being written.
    pub store: Command,
    /// How the body of that chunk reaches the device.
    pub body: Body,
}

impl ModeCatalog {
    /// The scripts that carry a body as base64.
    pub fn base64() -> Self {
        Self {
            hello: Command::new(include_str!("../scripts/base64/hello.sh")),
            retrieve: Command::new(include_str!("../scripts/base64/retrieve.sh")),
            store: Command::new(include_str!("../scripts/base64/store.sh")),
            body: Body::Document {
                delimiter: HEREDOC,
                end: Command::new(include_str!("../scripts/base64/store-end.sh")),
            },
            ..Self::shared()
        }
    }

    /// The scripts that carry a body as the bytes themselves.
    pub fn raw() -> Self {
        Self {
            hello: Command::new(include_str!("../scripts/raw/hello.sh")),
            retrieve: Command::new(include_str!("../scripts/raw/retrieve.sh")),
            store: Command::new(include_str!("../scripts/raw/store.sh")),
            body: Body::Counted,
            ..Self::shared()
        }
    }

    /// What the device says about one kind of sum.
    pub fn probe(&self, program: &str) -> Option<Command> {
        match program {
            "md5sum" => Some(self.probe_md5sum),
            "sha1sum" => Some(self.probe_sha1sum),
            "sha256sum" => Some(self.probe_sha256sum),
            _ => None,
        }
    }

    /// The commands that are the same whichever way a body travels.
    ///
    /// Only the probe, the two chunk commands and the framing of a body know
    /// what the mode is; the rest of the protocol asks the same questions.
    fn shared() -> Self {
        Self {
            hello: Command::new(""),
            pwd: Command::new(include_str!("../scripts/pwd.sh")),
            canonical: Command::new(include_str!("../scripts/canonical.sh")),
            kind: Command::new(include_str!("../scripts/kind.sh")),
            size: Command::new(include_str!("../scripts/size.sh")),
            list: Command::new(include_str!("../scripts/list.sh")),
            make_directory: Command::new(include_str!("../scripts/make-directory.sh")),
            create: Command::new(include_str!("../scripts/create.sh")),
            digest: Command::new(include_str!("../scripts/digest.sh")),
            probe_md5sum: Command::new(include_str!("../scripts/probe-md5sum.sh")),
            probe_sha1sum: Command::new(include_str!("../scripts/probe-sha1sum.sh")),
            probe_sha256sum: Command::new(include_str!("../scripts/probe-sha256sum.sh")),
            retrieve: Command::new(""),
            store: Command::new(""),
            body: Body::Counted,
        }
    }

    /// Every command of this catalogue, named, for the tests that hold them
    /// all to the same rules.
    #[cfg(test)]
    pub fn commands(&self) -> Vec<(&'static str, Command)> {
        let mut all = vec![
            ("hello", self.hello),
            ("pwd", self.pwd),
            ("canonical", self.canonical),
            ("kind", self.kind),
            ("size", self.size),
            ("list", self.list),
            ("make-directory", self.make_directory),
            ("create", self.create),
            ("digest", self.digest),
            ("probe-md5sum", self.probe_md5sum),
            ("probe-sha1sum", self.probe_sha1sum),
            ("probe-sha256sum", self.probe_sha256sum),
            ("retrieve", self.retrieve),
            ("store", self.store),
        ];
        if let Body::Document { end, .. } = self.body {
            all.push(("store-end", end));
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values good enough to render every script for a syntax check.
    fn sample() -> Vec<(&'static str, &'static str)> {
        vec![
            ("path", "'/tmp/one.bin'"),
            ("program", "\\sha1sum"),
            ("offset", "2965"),
            ("end", "4446"),
            ("count", "1482"),
            ("heredoc", HEREDOC),
        ]
    }

    fn every_command() -> Vec<(&'static str, Command)> {
        let mut all = ModeCatalog::base64().commands();
        all.extend(ModeCatalog::raw().commands());
        all
    }

    #[test]
    fn a_chunk_of_base64_is_cut_to_whole_lines() {
        for chunk in [1, 20, 77, 153, 2048] {
            assert_eq!(
                Mode::Base64.slice(chunk) % LINE_BYTES as u64,
                0,
                "a chunk of {chunk} bytes cut a line of base64 in half"
            );
        }
        assert_eq!(Mode::Base64.slice(1), LINE_BYTES as u64);
        assert_eq!(Mode::Base64.slice(2048), 26 * LINE_BYTES as u64);
        assert_eq!(Mode::Raw.slice(2048), 2048);
    }

    #[test]
    fn the_body_of_a_chunk_is_the_one_thing_the_two_modes_make_differently() {
        assert_eq!(Mode::Raw.body(b"abc"), b"abc");
        assert_eq!(Mode::Base64.body(b"abc"), b"YWJj\n");
        assert!(Mode::Base64.body(b"").is_empty());
    }

    #[test]
    fn no_line_of_a_body_is_longer_than_a_terminal_will_keep() {
        let bytes: Vec<u8> = (0..=255_u8).cycle().take(40_000).collect();
        let body = Mode::Base64.body(&bytes);
        for line in String::from_utf8(body).expect("base64 is text").lines() {
            assert!(
                line.len() <= LINE_CHARS,
                "a line of {} characters",
                line.len()
            );
        }
    }

    #[test]
    fn no_shipped_script_carries_the_marker_of_a_reply() {
        for (name, command) in every_command() {
            let rendered = command.render(&sample());
            assert!(
                !rendered.contains("###"),
                "{name} carries the marker its own echo would forge:\n{rendered}"
            );
            assert!(
                !rendered.contains('\n'),
                "{name} did not fold into one line:\n{rendered}"
            );
        }
    }

    #[test]
    fn every_shipped_script_is_written_the_way_it_folds() {
        for (name, command) in every_command() {
            let body = command.body();
            assert!(
                !body.contains('\t'),
                "{name} is indented with tabs, which a fold turns into nothing"
            );
            let mut lines = body
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .peekable();
            while let Some(line) = lines.next() {
                if lines.peek().is_none() {
                    break;
                }
                let ends_open = [";", "do", "then", "else", "in", "(", ")"]
                    .iter()
                    .any(|ending| line.ends_with(ending));
                assert!(
                    ends_open,
                    "{name} has a line the fold would run into the next one:\n{line}"
                );
            }
        }
    }

    #[test]
    fn every_shipped_script_is_shell_a_shell_accepts() {
        for (name, command) in every_command() {
            let rendered = command.render(&sample());
            assert!(
                !rendered.contains('{'),
                "{name} has a hole nothing answered to:\n{rendered}"
            );

            let checked = std::process::Command::new("sh")
                .arg("-n")
                .arg("-c")
                .arg(&rendered)
                .output()
                .expect("a shell runs");
            assert!(
                checked.status.success(),
                "{name} is not shell a shell accepts:\n{rendered}\n{}",
                String::from_utf8_lossy(&checked.stderr)
            );
        }
    }

    #[test]
    fn nothing_a_script_sets_outlives_the_command_that_set_it() {
        for (name, command) in every_command() {
            let rendered = command.render(&sample());
            assert!(
                !rendered.contains("SHXFER_STTY"),
                "{name} keeps the settings of the line in a variable:\n{rendered}"
            );
            if rendered.contains("stty raw") || rendered.contains("stty -echo") {
                assert!(
                    rendered.contains("stty -raw echo"),
                    "{name} changes the line and does not put it back:\n{rendered}"
                );
            }
        }
    }
}
