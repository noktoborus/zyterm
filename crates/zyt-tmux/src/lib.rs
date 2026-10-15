//! The control mode protocol of tmux, `tmux -CC`.
//!
//! [`Splitter`] finds where control mode starts and ends in the bytes a line
//! brings, so the bytes before and after go to the terminal and those inside
//! go to the protocol. [`Client`] reads the protocol: the output of panes, the
//! replies matched to the commands sent, the notifications; and writes the
//! commands.
//!
//! The crate does no I/O and starts no thread; the bytes it is given are the
//! caller's, and the bytes of a pane leave it as slices of them.

#![deny(missing_docs)]

mod client;
mod command;
mod error;
mod escape;
mod id;
mod layout;
mod line;
mod modes;
mod splitter;

pub use client::{Client, Reply, Sink};
pub use command::{BELLS, Command, Flow, Keys, Part};
pub use error::{Error, Result};
pub use id::{PaneId, SessionId, WindowId};
pub use layout::{Content, Layout, MAX_DEPTH};
pub use line::Notice;
pub use modes::{MODES, Modes};
pub use splitter::{Piece, Splitter};
