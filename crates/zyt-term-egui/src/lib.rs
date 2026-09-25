//! egui widget for a [`zyt_term::Terminal`].
//!
//! The widget draws a snapshot of the terminal, handles mouse selection,
//! scrolling and mouse reporting, and returns the bytes that must be written to
//! the device. Keyboard input is left to the caller, which usually runs its key
//! bindings first; [`map_key`] and [`map_modifiers`] convert egui key events
//! into the input types of the emulation crate.

#![deny(missing_docs)]

mod cache;
mod font;
mod input_map;
mod scrollbar;
mod theme;
mod view;

pub use cache::TerminalCache;
pub use font::TerminalFont;
pub use input_map::{map_button, map_key, map_modifiers};
pub use theme::TerminalTheme;
pub use view::{LinkTarget, TerminalOutput, TerminalView};
