//! A menu of wide plates for egui.
//!
//! The menu is a window of full width entries in the middle of the window: the
//! arrow keys and the wheel walk it, `Right` steps into the entries of an entry, `Enter`
//! chooses, `Esc` closes, and typing searches every entry and sub-entry at
//! once. A click beside it closes it as well; the pointer wandering off does
//! not.
//!
//! The crate knows the toolkit and nothing else: entries are plain data the
//! caller builds, and the caller is told the identifier of the entry that was
//! chosen together with the keys that were held while it was.

#![deny(missing_docs)]

mod error;
mod item;
mod search;
mod state;
mod view;

pub use error::{MenuError, Result};
pub use item::MenuItem;
pub use state::{MenuState, Row};
pub use view::{Beside, Chosen, PlateMenu, Shift};
