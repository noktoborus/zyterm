//! Files transferred over SHell, as a library.
//!
//! The protocol needs nothing on the far end but a shell and the usual
//! utilities: every command is a script the device runs, and every answer is
//! what it prints. See `PROTOCOL.md` beside this crate for the wire format.
//!
//! Nothing here opens a line or a process: the caller hands in a reader and a
//! writer, which over a console are the two halves of the same line.

#![deny(missing_docs)]

mod catalog;
mod checksum;
mod error;
mod notes;
mod reader;
mod report;
mod script;
mod session;
mod size;
mod walk;
mod wire;

pub use catalog::{Body, Mode, ModeCatalog};
pub use checksum::Digest;
pub use error::{Result, ShXferError};
pub use notes::Notes;
pub use reader::Reader;
pub use report::Report;
pub use script::Command;
pub use session::{DEFAULT_CHUNK, DEFAULT_PIPE_CHUNK, Entry, EntryKind, Session};
pub use size::short_size;
pub use walk::{LocalItem, RemoteItem, local_items, remote_items};
pub use wire::quote;
