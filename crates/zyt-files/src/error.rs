//! Error type of the crate.

use std::path::PathBuf;

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, FileError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// A file could not be read, written or removed.
    #[error("file error at {path}")]
    Io {
        /// Path the operation worked on.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The desktop refused to take the file into its trash.
    #[error("cannot move {path} to the trash")]
    Trash {
        /// Path the operation worked on.
        path: PathBuf,
        /// Underlying error of the desktop.
        #[source]
        source: trash::Error,
    },

    /// The file is larger than the caller allows.
    #[error("{path} is {size} bytes, more than the {limit} allowed")]
    TooLarge {
        /// Path the operation worked on.
        path: PathBuf,
        /// Size of the file.
        size: u64,
        /// Largest size the caller allows.
        limit: u64,
    },

    /// The task was cancelled before it finished.
    #[error("cancelled")]
    Cancelled,
}
