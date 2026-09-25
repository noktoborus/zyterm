//! What can go wrong while the list is made.

/// One failure of the tool.
#[derive(Debug, thiserror::Error)]
pub enum GlyphsError {
    /// The font file could not be read.
    #[error("the font file {} cannot be read", path.display())]
    Font {
        /// Path that was asked for.
        path: std::path::PathBuf,
        /// What the file system said.
        #[source]
        source: std::io::Error,
    },

    /// The list could not be written.
    #[error("the list cannot be written to {}", path.display())]
    Write {
        /// Path that was written to.
        path: std::path::PathBuf,
        /// What the file system said.
        #[source]
        source: std::io::Error,
    },

    /// The fonts carry no glyph at all, so there is nothing to list.
    #[error("no font carries a single glyph")]
    NoGlyphs,
}

/// Result of this tool.
pub type Result<T> = std::result::Result<T, GlyphsError>;
