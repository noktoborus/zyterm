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
mod gap;
mod images;
mod input;
mod null;
mod osc;
mod search;
mod shrink;
mod sixel;

pub use content::{
    Cell, CellStyle, Color, CursorInfo, CursorShape, LinkId, PALETTE_COLORS, RenderableContent,
    Rgb, SnapshotPicture, TerminalModes,
};
pub use emulator::{
    ClipboardAccess, DEFAULT_CELL, DefaultColors, GRID_CELL_BYTES, SelectionKind, SelectionSize,
    SelectionStep, Terminal, TerminalConfig,
};
pub use error::{Result, TermError};
pub use event::TerminalEvent;
pub use gap::{GapPart, PAUSE_SIGN, gap_part};
pub use images::{DEFAULT_PICTURE_BUDGET, ImageId, ImagePart, Picture};
pub use input::{Key, Modifiers, MouseButton, ShiftEnter, encode_key, encode_mouse, encode_paste};
pub use null::{NULL_SYMBOL, NullPart, TIMES_SIGN, null_part, null_text};
pub use osc::{MarkKind, NotificationKind, OscReport, ProgressState, Sniffed, SniffedReport};
pub use search::{SearchDirection, SearchKind, SearchOptions};
pub use sixel::{
    DEFAULT_PICTURE_LIMIT, DEFAULT_PICTURE_WEIGHT, GraphicsQuery, MAX_CANVAS, PALETTE_REGISTERS,
    PictureAccess, PictureLimit, Pixels, SixelPlaceholder, SixelRefusal, SixelReport, decode,
};
