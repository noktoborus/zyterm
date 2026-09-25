//! Error type of the crate.

use std::path::PathBuf;

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, ShXferError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum ShXferError {
    /// The device said nothing for as long as the caller was willing to wait.
    #[error("the line went quiet for {seconds} seconds")]
    Quiet {
        /// How long was waited.
        seconds: u64,
    },

    /// The device answered a command with a refusal.
    #[error("the device refused {command}: reply code {code}")]
    Refused {
        /// Command that was refused.
        command: String,
        /// Code the device answered with.
        code: u32,
    },

    /// The device answered with a code the command does not expect.
    #[error("{command} expected reply code {expected}, the device said {code}")]
    Unexpected {
        /// Command that was sent.
        command: String,
        /// Code that was expected.
        expected: u32,
        /// Code that arrived.
        code: u32,
    },

    /// The device stopped in the middle of a file.
    #[error("{path}: {got} bytes of {expected} arrived before the line stopped")]
    Truncated {
        /// File that was being read.
        path: String,
        /// Size the device announced.
        expected: u64,
        /// How much arrived.
        got: u64,
    },

    /// The file the device holds is not as long as what was written into it.
    #[error("{path}: {got} bytes stand on the device, {expected} were written")]
    WrongSize {
        /// File that was being written.
        path: String,
        /// How much was written.
        expected: u64,
        /// How much the device says stands there.
        got: u64,
    },

    /// The two sides do not say the same thing about a file that travelled.
    #[error("{path}: the device says {theirs}, this machine says {ours}")]
    Mismatch {
        /// File that does not match.
        path: String,
        /// What this machine counted.
        ours: String,
        /// What the device said.
        theirs: String,
    },

    /// The program that counts a sum said nothing about a file.
    #[error("{program} said nothing about {path}")]
    NoDigest {
        /// Program that was run.
        program: String,
        /// File it was run on.
        path: String,
    },

    /// The device did not say which directory it stands in.
    #[error("the device did not say where it stands")]
    NoWorkingDirectory,

    /// The program that was to run where the device stands did not start.
    #[error("cannot start {program}")]
    NoProgram {
        /// Program that was named.
        program: String,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },

    /// Nothing was named to fetch and nothing said to take the lot.
    #[error("name what to fetch, or pass --all to take the whole directory")]
    NothingToTake,

    /// The device has not got a command the transfer cannot do without.
    ///
    /// The probe names it: the device answers `E<program>` for a command that
    /// does not work, and that program is what this carries.
    #[error("the device has no {command}, which is needed to carry a file this way")]
    MissingCommand {
        /// Command the probe found missing, as the device named it.
        command: String,
    },

    /// The device announced something that is not a size.
    #[error("{path}: the device gave no size")]
    NoSize {
        /// File that was being read.
        path: String,
    },

    /// The base64 the device sent cannot be read.
    #[error("the device sent base64 that cannot be read")]
    Decode {
        /// What the decoder said.
        #[source]
        source: base64::DecodeError,
    },

    /// A path leads back into itself through a symbolic link.
    #[error("{path} leads back into itself")]
    Loop {
        /// Path the walk stopped at.
        path: PathBuf,
    },

    /// A local path could not be walked.
    #[error("{path} cannot be walked")]
    Walk {
        /// Path the walk stopped at.
        path: PathBuf,
        /// What the walker said.
        #[source]
        source: walkdir::Error,
    },

    /// A local file could not be read or written.
    #[error("{path} cannot be read or written")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// What the system said.
        #[source]
        source: std::io::Error,
    },

    /// The line itself could not be read or written.
    #[error("the line cannot be read or written")]
    Line {
        /// What the system said.
        #[source]
        source: std::io::Error,
    },
}
