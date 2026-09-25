//! Error type of the crate.

use std::path::PathBuf;

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The platform reported no home directory, so no location can be derived.
    #[error("no home directory for the current user")]
    NoHomeDirectory,

    /// A directory could not be created.
    #[error("cannot create directory: {path}")]
    CreateDirectory {
        /// Directory that was requested.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A file could not be read.
    #[error("cannot read file: {path}")]
    Read {
        /// File that was requested.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A file could not be written.
    #[error("cannot write file: {path}")]
    Write {
        /// File that was requested.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The file content is not valid for the requested type.
    #[error("cannot decode file: {path}")]
    Decode {
        /// File that was read.
        path: PathBuf,
        /// Underlying format error.
        #[source]
        source: serde_yaml_ng::Error,
    },

    /// The value cannot be represented in the file format.
    #[error("cannot encode value for file: {path}")]
    Encode {
        /// File that was written.
        path: PathBuf,
        /// Underlying format error.
        #[source]
        source: serde_yaml_ng::Error,
    },
}
