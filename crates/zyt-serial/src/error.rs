//! Error type of the crate. Every failure is a distinct variant; lower level
//! errors are wrapped, never converted to text.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, PortError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    /// The requested port path does not exist in the system.
    #[error("port not found: {path}")]
    NotFound {
        /// Operating system path of the port.
        path: String,
    },

    /// The process is not allowed to open the port.
    #[error("permission denied: {path}")]
    PermissionDenied {
        /// Operating system path of the port.
        path: String,
    },

    /// The port exists but is already opened by another process.
    #[error("port busy: {path}")]
    Busy {
        /// Operating system path of the port.
        path: String,
    },

    /// A text that should name a device identity does not.
    #[error("not the text form of a port identity: {text}")]
    Key {
        /// The text as it was read.
        text: String,
    },

    /// The device vanished while the port was open.
    #[error("port disconnected")]
    Disconnected,

    /// A read or write did not finish inside the configured timeout.
    #[error("operation timed out")]
    Timeout,

    /// The driver rejected the requested line parameters.
    #[error("unsupported line parameters")]
    UnsupportedParams,

    /// Port enumeration failed.
    #[error("port enumeration failed")]
    Enumerate {
        /// Underlying driver error.
        #[source]
        source: std::io::Error,
    },

    /// Any other driver error that has no dedicated variant.
    #[error("serial driver error")]
    Driver {
        /// Underlying driver error.
        #[source]
        source: std::io::Error,
    },

    /// Plain I/O failure on an open port.
    #[error("io error")]
    Io {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The supervisor worker thread is gone.
    #[error("supervisor worker stopped")]
    WorkerStopped,

    /// The supervisor worker thread could not be started.
    #[error("supervisor worker could not start")]
    WorkerStart {
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl PortError {
    /// Maps a failure of the driver onto a dedicated variant when the kind is
    /// known. What the platform module hands in is an I/O error, so this
    /// mapping is the same on both platforms.
    pub(crate) fn from_driver(path: &str, source: std::io::Error) -> Self {
        match source.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound {
                path: path.to_string(),
            },
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied {
                path: path.to_string(),
            },
            std::io::ErrorKind::ResourceBusy => Self::Busy {
                path: path.to_string(),
            },
            std::io::ErrorKind::InvalidInput => Self::UnsupportedParams,
            std::io::ErrorKind::TimedOut => Self::Timeout,
            _ => Self::Driver { source },
        }
    }

    /// Maps an I/O error of an open port; disconnects get their own variant.
    pub(crate) fn from_io(source: std::io::Error) -> Self {
        match source.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Self::Timeout,
            std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof => Self::Disconnected,
            _ => Self::Io { source },
        }
    }

    /// True when the error means the device is no longer reachable.
    pub fn is_fatal_for_connection(&self) -> bool {
        matches!(
            self,
            Self::Disconnected | Self::NotFound { .. } | Self::Io { .. } | Self::Driver { .. }
        )
    }
}
