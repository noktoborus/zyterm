//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, TermError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum TermError {
    /// The requested grid size is outside the supported range.
    #[error("invalid grid size: {columns}x{rows}")]
    InvalidSize {
        /// Requested number of columns.
        columns: usize,
        /// Requested number of rows.
        rows: usize,
    },

    /// The search pattern could not be built.
    #[error("invalid search pattern: {pattern}")]
    InvalidPattern {
        /// Pattern that was rejected.
        pattern: String,
    },

    /// A viewport position does not exist in the current grid.
    #[error("position outside the grid: column {column}, row {row}")]
    OutsideGrid {
        /// Requested column.
        column: usize,
        /// Requested row.
        row: usize,
    },
}
