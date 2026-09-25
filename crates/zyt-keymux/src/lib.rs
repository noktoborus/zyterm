//! Key bindings with contexts, sequences and a command registry.
//!
//! The crate accepts key presses and resolves them into command identifiers.
//! It runs no action itself and knows no toolkit: the caller maps its own key
//! events onto [`KeyStroke`], sets the active contexts and executes the
//! commands it receives.

#![deny(missing_docs)]

mod defaults;
mod dispatch;
mod error;
mod key;
mod keymap;
mod registry;

pub use defaults::{
    CONTEXT_GLOBAL, CONTEXT_PALETTE, CONTEXT_SEARCH, CONTEXT_SETTINGS, CONTEXT_STATUS_BAR,
    CONTEXT_TERMINAL, CONTEXT_TRANSFER, DEFAULT_BINDINGS, DefaultBinding, default_keymap,
};
pub use dispatch::{ActionSink, Dispatch, KeyDispatcher};
pub use error::{KeymapError, Result};
pub use key::{Chord, KeyCode, KeyStroke, Modifiers};
pub use keymap::{Binding, CommandId, Context, Keymap};
pub use registry::{Command, CommandRegistry, Hit};
