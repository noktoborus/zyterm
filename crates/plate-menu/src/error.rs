//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, MenuError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum MenuError {
    /// The menu was opened with nothing that can be chosen.
    #[error("menu without an entry that can be chosen")]
    Empty,
    /// Entries were given to a menu that is not open.
    #[error("entries given to a closed menu")]
    Closed,
}
