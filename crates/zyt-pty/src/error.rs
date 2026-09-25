//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, PtyError>;

/// Error of the pty layer, wrapped so it can be carried in a variant.
pub type SourceError = Box<dyn std::error::Error + Send + Sync>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    /// The pseudo terminal could not be created.
    #[error("cannot open a pseudo terminal")]
    Open {
        /// Underlying pty error.
        #[source]
        source: SourceError,
    },

    /// The shell could not be started.
    #[error("cannot start shell: {program}")]
    Spawn {
        /// Program that was requested.
        program: String,
        /// Underlying pty error.
        #[source]
        source: SourceError,
    },

    /// The reader or writer end of the pty was not available.
    #[error("cannot use the pty pipes")]
    Pipes {
        /// Underlying pty error.
        #[source]
        source: SourceError,
    },

    /// The window size could not be applied.
    #[error("cannot resize the pty to {columns}x{rows}")]
    Resize {
        /// Requested number of columns.
        columns: u16,
        /// Requested number of rows.
        rows: u16,
        /// Underlying pty error.
        #[source]
        source: SourceError,
    },

    /// A helper thread could not be started.
    #[error("cannot start pty thread")]
    ThreadStart {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The session already ended.
    #[error("session already ended")]
    Ended,
}
