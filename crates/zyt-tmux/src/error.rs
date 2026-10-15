//! Error type of the crate. Every failure is a distinct variant; none of them
//! ends control mode, which is the caller's to leave.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// A line that is no notification and no line of a reply; it is skipped
    /// up to the next line that starts with `%`.
    #[error("a line of control mode that is no notification and no reply")]
    MalformedLine,

    /// A layout whose checksum does not match the text after it.
    #[error("layout checksum {written:04x}, the layout sums to {counted:04x}")]
    LayoutChecksum {
        /// The checksum at the head of the layout.
        written: u16,
        /// The checksum of the text after it.
        counted: u16,
    },

    /// A layout that cannot be read, or nested deeper than this crate follows.
    #[error("a layout that cannot be read")]
    MalformedLayout,

    /// A reply of `display -p` that is not the modes it asked for.
    #[error("a reply of display -p that is not the modes asked for")]
    MalformedModes,

    /// A `%begin` of a reply to this client with nothing sent for it.
    #[error("a reply with no command sent for it")]
    UnexpectedReply,
}
