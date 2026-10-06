//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, ScriptError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// A program was asked for with nothing to run.
    #[error("no command to run")]
    EmptyCommand,

    /// A program could not be started.
    #[error("cannot start command: {command}")]
    Spawn {
        /// Command line or program that was asked for.
        command: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The child process did not provide the pipes the caller asked for.
    #[error("child process has no usable pipes")]
    MissingPipes,

    /// A helper thread could not be started.
    #[error("cannot start a thread for a script")]
    ThreadStart {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A child process could not be stopped.
    #[error("cannot stop the child process")]
    Kill {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The group every child of a script is kept in could not be made.
    #[error("cannot make the process group of a script")]
    Group {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
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
    #[error("the job already finished")]
    Finished,

    /// The key sent at the end of a run cannot be read.
    #[error("invalid finish key: {input}")]
    InvalidFinishKey {
        /// Text that was given.
        input: String,
    },

    /// The script expects another kind of path than the one that was given.
    #[error("the script expects {expected:?}, got {given:?}")]
    TargetMismatch {
        /// What the script asks for.
        expected: crate::target::TargetKind,
        /// What the caller passed.
        given: crate::target::TargetKind,
    },

    /// The script asks the source for a value it has not got.
    #[error("the script asks for the value {name}, which the source has not got")]
    UnsetVariable {
        /// Name the script asked for.
        name: String,
    },

    /// The line is gone: the session was disconnected or the run was dropped.
    #[error("the line is closed")]
    LineClosed,

    /// The run was asked to stop.
    #[error("the run was cancelled")]
    Cancelled,

    /// No script of that name was found in any of the directories searched.
    #[error("no script named {name}")]
    NotFound {
        /// Name that was asked for.
        name: String,
    },

    /// Two scripts of one directory call themselves the same thing.
    #[error("two scripts call themselves {name}: {first} and {second}")]
    Duplicate {
        /// Name both of them carry.
        name: String,
        /// The first file found.
        first: std::path::PathBuf,
        /// The second file found.
        second: std::path::PathBuf,
    },

    /// A file of a script could not be read.
    #[error("cannot read {path}")]
    Read {
        /// File that was asked for.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A script could not be loaded: it is not Lua, or it fails as it loads.
    #[error("cannot load the script {path}")]
    Load {
        /// File that was loaded.
        path: std::path::PathBuf,
        /// What the interpreter said.
        #[source]
        source: mlua::Error,
    },

    /// A script says nothing of itself, or says it wrongly.
    #[error("the script {name} has no usable manifest: {what}")]
    Manifest {
        /// Script that was loaded.
        name: String,
        /// What is wrong with it.
        what: String,
    },

    /// The script does not offer the direction it was asked for.
    #[error("the script {name} does not {direction}")]
    NoDirection {
        /// Script that was asked.
        name: String,
        /// Direction that was asked for.
        direction: String,
    },

    /// A script failed while it ran.
    #[error("the script {name} failed: {said}")]
    Runtime {
        /// Script that was running.
        name: String,
        /// What the script itself said, without the walk of the interpreter.
        ///
        /// The whole of what the interpreter says carries a traceback, which
        /// is what the cause is kept for; the one line a script raised is what
        /// whoever started it came to read.
        said: String,
        /// What the interpreter said.
        #[source]
        source: mlua::Error,
    },

    /// A script was given up on: it answered neither the flag nor the hook.
    #[error("the script {name} was abandoned")]
    Abandoned {
        /// Script that was running.
        name: String,
    },

    /// The script asked a question with nobody there to answer it.
    #[error("there is nobody to ask")]
    NoPrompt,

    /// Something took longer than it was given.
    #[error("{what} took longer than {seconds} s")]
    Timeout {
        /// What was waited for.
        what: String,
        /// How long it was given.
        seconds: u64,
    },

    /// A directory of this machine could not be walked.
    #[error("cannot walk {path}")]
    Walk {
        /// Directory that was walked.
        path: std::path::PathBuf,
        /// Underlying error.
        #[source]
        source: walkdir::Error,
    },

    /// A symbolic link leads back into the tree it stands in.
    #[error("{path} leads back into its own tree")]
    Loop {
        /// Link that was followed.
        path: std::path::PathBuf,
    },

    /// Text that was supposed to be base64 is not.
    #[error("cannot read base64")]
    Decode {
        /// Underlying error.
        #[source]
        source: base64::DecodeError,
    },

    /// A file could not be read or written.
    #[error("file error: {path}")]
    Io {
        /// File that was asked for.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
