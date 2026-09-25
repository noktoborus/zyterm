//! Error type of the crate.

/// Result alias used by every public function of this crate.
pub type Result<T> = std::result::Result<T, KeymapError>;

/// All failures produced by this crate.
#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    /// A key description could not be parsed.
    #[error("invalid key description: {input}")]
    InvalidKey {
        /// Text that was parsed.
        input: String,
    },

    /// A modifier name is not known.
    #[error("unknown modifier: {name}")]
    UnknownModifier {
        /// Text that was parsed.
        name: String,
    },

    /// A binding has no keys.
    #[error("binding without keys")]
    EmptyBinding,

    /// Two bindings of the same context use the same key sequence.
    #[error("conflicting binding in context {context}: {keys}")]
    Conflict {
        /// Context holding the conflict.
        context: String,
        /// Key sequence bound twice.
        keys: String,
        /// Command bound first.
        existing: String,
        /// Command that could not be bound.
        incoming: String,
    },

    /// The stored key map is not valid YAML for this format.
    #[error("cannot decode key map")]
    Decode {
        /// Underlying format error.
        #[source]
        source: serde_yaml_ng::Error,
    },

    /// The key map cannot be written as YAML.
    #[error("cannot encode key map")]
    Encode {
        /// Underlying format error.
        #[source]
        source: serde_yaml_ng::Error,
    },
}
