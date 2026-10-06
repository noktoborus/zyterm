//! Errors of the application layer and their translation keys.
//!
//! Library crates return typed errors without any user facing text. The
//! application maps every variant onto a translation key, so all localized text
//! lives here and in the `locales` directory.

use zyt_config::ConfigError;
use zyt_keymux::KeymapError;
use zyt_pty::PtyError;
use zyt_script::ScriptError;
use zyt_serial::PortError;

/// An error and the chain of what caused it, as one line.
///
/// It is for a message printed into the terminal beside the output it belongs
/// to: what a script said is the first of them, and what the platform said
/// underneath it is the rest.
pub fn said(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        let said = next.to_string();
        if !text.contains(&said) {
            text.push_str(": ");
            text.push_str(&said);
        }
        cause = next.source();
    }
    text
}

/// Result alias of the application layer.
pub type Result<T> = std::result::Result<T, AppError>;

/// All failures the application reports to the user.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A serial port operation failed.
    #[error("serial port error")]
    Port {
        /// Underlying error.
        #[source]
        source: PortError,
    },

    /// A local console operation failed.
    #[error("local console error")]
    Pty {
        /// Underlying error.
        #[source]
        source: PtyError,
    },

    /// A configuration file operation failed.
    #[error("configuration error")]
    Config {
        /// Underlying error.
        #[source]
        source: ConfigError,
    },

    /// A key map operation failed.
    #[error("key map error")]
    Keymap {
        /// Underlying error.
        #[source]
        source: KeymapError,
    },

    /// A file transfer failed.
    #[error("file transfer error")]
    Script {
        /// Underlying error.
        #[source]
        source: ScriptError,
    },

    /// The terminal could not be created.
    #[error("terminal error")]
    Terminal {
        /// Underlying error.
        #[source]
        source: zyt_term::TermError,
    },

    /// A file operation of the session failed.
    #[error("file operation error")]
    File {
        /// Underlying error.
        #[source]
        source: zyt_files::FileError,
    },

    /// The program line of a console cannot be read.
    #[error("console program line cannot be read: {id}")]
    ConsoleProgram {
        /// Identity of the console.
        id: crate::consoles::ConsoleId,
    },

    /// A console file could not be written or removed.
    #[error("console file error")]
    Console {
        /// Identity of the console.
        id: crate::consoles::ConsoleId,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A text that should name a console does not.
    #[error("not the identity of a console: {text}")]
    ConsoleId {
        /// The text as it was read.
        text: String,
    },

    /// A theme file could not be read.
    #[error("theme file error")]
    ThemeRead {
        /// Name of the theme.
        name: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A theme file could not be understood.
    #[error("theme file format error")]
    ThemeParse {
        /// Name of the theme.
        name: String,
        /// Underlying format error.
        #[source]
        source: Box<toml::de::Error>,
    },

    /// The directory the answers of the forms stand in could not be read.
    #[error("the directory of the answered forms cannot be read")]
    Forms {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A link could not be opened.
    #[error("cannot open a link")]
    Link {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The console profile does not exist.
    #[error("no such console profile")]
    NoConsole,

    /// No script is picked for this source.
    #[error("no script")]
    NoScript,

    /// No source is connected.
    #[error("no connection")]
    NotConnected,

    /// A transfer that holds the line is already running.
    #[error("a transfer already holds the line")]
    TransferRunning,
}

impl AppError {
    /// Translation key of the message shown to the user.
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Port { source } => match source {
                PortError::NotFound { .. } => "error.port.not_found",
                PortError::PermissionDenied { .. } => "error.port.permission_denied",
                PortError::Busy { .. } => "error.port.busy",
                PortError::Disconnected => "error.port.disconnected",
                PortError::Timeout => "error.port.timeout",
                PortError::UnsupportedParams => "error.port.unsupported_params",
                PortError::Enumerate { .. } => "error.port.enumerate",
                PortError::Key { .. } => "error.port.key",
                PortError::Driver { .. } | PortError::Io { .. } => "error.port.driver",
                PortError::WorkerStopped | PortError::WorkerStart { .. } => "error.port.worker",
            },
            Self::Pty { source } => match source {
                PtyError::Open { .. } | PtyError::Pipes { .. } => "error.pty.open",
                PtyError::Spawn { .. } => "error.pty.spawn",
                PtyError::Resize { .. } => "error.pty.resize",
                PtyError::ThreadStart { .. } | PtyError::Ended => "error.pty.ended",
            },
            Self::Config { source } => match source {
                ConfigError::NoHomeDirectory => "error.config.no_home",
                ConfigError::CreateDirectory { .. } | ConfigError::Write { .. } => {
                    "error.config.write"
                }
                ConfigError::Read { .. } => "error.config.read",
                ConfigError::Decode { .. } => "error.config.decode",
                ConfigError::Encode { .. } => "error.config.encode",
            },
            Self::Keymap { source } => match source {
                KeymapError::Conflict { .. } => "error.keymap.conflict",
                KeymapError::Decode { .. } | KeymapError::Encode { .. } => "error.keymap.file",
                _ => "error.keymap.key",
            },
            Self::Script { source } => match source {
                ScriptError::NotFound { .. } | ScriptError::Duplicate { .. } => {
                    "error.script.not_found"
                }
                ScriptError::Read { .. } | ScriptError::Load { .. } => "error.script.load",
                ScriptError::Manifest { .. } | ScriptError::NoDirection { .. } => {
                    "error.script.manifest"
                }
                ScriptError::Spawn { .. } => "error.script.spawn",
                ScriptError::TargetMismatch { .. } => "error.script.target",
                ScriptError::InvalidFinishKey { .. } => "error.script.finish_key",
                ScriptError::UnsetVariable { .. } => "error.script.variable",
                ScriptError::Cancelled => "error.script.cancelled",
                ScriptError::Abandoned { .. } => "error.script.abandoned",
                _ => "error.script.failed",
            },
            Self::Terminal { .. } => "error.terminal",
            Self::File { source } => match source {
                zyt_files::FileError::Trash { .. } => "error.file.trash",
                zyt_files::FileError::TooLarge { .. } => "error.file.too_large",
                _ => "error.file",
            },
            Self::Console { .. } => "error.console",
            Self::ConsoleProgram { .. } => "error.console.program",
            Self::ConsoleId { .. } => "error.console.id",
            Self::Forms { .. } => "error.forms.read",
            Self::ThemeRead { .. } => "error.theme.read",
            Self::ThemeParse { .. } => "error.theme.parse",
            Self::NoScript => "error.script.no_script",
            Self::NoConsole => "error.no_console",
            Self::Link { .. } => "error.link",
            Self::NotConnected => "error.not_connected",
            Self::TransferRunning => "error.transfer.running",
        }
    }
}

impl From<PortError> for AppError {
    fn from(source: PortError) -> Self {
        Self::Port { source }
    }
}

impl From<PtyError> for AppError {
    fn from(source: PtyError) -> Self {
        Self::Pty { source }
    }
}

impl From<ConfigError> for AppError {
    fn from(source: ConfigError) -> Self {
        Self::Config { source }
    }
}

impl From<KeymapError> for AppError {
    fn from(source: KeymapError) -> Self {
        Self::Keymap { source }
    }
}

impl From<ScriptError> for AppError {
    fn from(source: ScriptError) -> Self {
        Self::Script { source }
    }
}

impl From<zyt_term::TermError> for AppError {
    fn from(source: zyt_term::TermError) -> Self {
        Self::Terminal { source }
    }
}
