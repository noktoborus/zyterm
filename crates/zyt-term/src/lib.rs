//! Terminal emulation that is driven by bytes, not by a process.
//!
//! The crate wraps the alacritty terminal state machine and exposes a snapshot
//! of the visible grid in its own types, so a renderer never sees the types of
//! the emulation backend. Where the bytes come from is not its concern: a
//! serial port, a local shell or a test buffer all work the same way.

#![deny(missing_docs)]

mod content;
mod emulator;
mod error;
mod event;
mod input;
mod null;
mod osc;
mod search;

pub use content::{
    Cell, CellStyle, Color, CursorInfo, CursorShape, LinkId, PALETTE_COLORS, RenderableContent,
    Rgb, TerminalModes,
};
pub use emulator::{
    ClipboardAccess, GRID_CELL_BYTES, SelectionKind, SelectionSize, SelectionStep, Terminal,
    TerminalConfig,
};
pub use error::{Result, TermError};
pub use event::TerminalEvent;
pub use input::{Key, Modifiers, MouseButton, encode_key, encode_mouse, encode_paste};
pub use null::{NULL_SYMBOL, NullPart, TIMES_SIGN, null_part, null_text};
pub use osc::{MarkKind, NotificationKind, OscReport, ProgressState, SniffedReport};
pub use search::{SearchDirection, SearchKind, SearchOptions};
