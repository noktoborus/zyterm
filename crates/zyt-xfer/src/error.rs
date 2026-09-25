//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, XferError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum XferError {
    /// The command line of the profile is empty.
    #[error("profile {profile} has no command")]
    EmptyCommand {
        /// Profile that was used.
        profile: String,
    },

    /// The command line could not be started.
    #[error("cannot start command: {command}")]
    Spawn {
        /// Command line that was requested.
        command: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The child process did not provide the pipes the gateway needs.
    #[error("child process has no usable pipes")]
    MissingPipes,

    /// A helper thread could not be started.
    #[error("cannot start gateway thread")]
    ThreadStart {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The profile expects another kind of path than the one that was given.
    #[error("profile {profile} expects {expected:?}, got {given:?}")]
    TargetMismatch {
        /// Profile that was used.
        profile: String,
        /// What the argument list asks for.
        expected: crate::profile::TargetKind,
        /// What the caller passed.
        given: crate::profile::TargetKind,
    },

    /// The key sent at the end of a transfer cannot be read.
    #[error("profile {profile} has an invalid finish key: {input}")]
    InvalidFinishKey {
        /// Profile that was used.
        profile: String,
        /// Text that was given.
        input: String,
    },

    /// The profile asks the source for a value it has not got.
    #[error("profile {profile} asks for the value {name}, which the source has not got")]
    UnsetVariable {
        /// Profile that was used.
        profile: String,
        /// Name the line asked for.
        name: String,
    },

    /// The file the output of a job goes into could not be made.
    #[error("cannot open the log file: {path}")]
    Log {
        /// File that was asked for.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The job was already finished when the call was made.
    #[error("transfer already finished")]
    Finished,

    /// The child process could not be stopped.
    #[error("cannot stop the child process")]
    Kill {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
