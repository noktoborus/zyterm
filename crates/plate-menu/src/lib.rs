#![cfg_attr(doc, doc = include_str!(concat!(env!("OUT_DIR"), "/pictures.md")))]
#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

mod error;
mod item;
mod search;
mod state;
mod view;

pub use error::{MenuError, Result};
pub use item::MenuItem;
pub use state::{MenuState, Row};
pub use view::{Beside, Chosen, PlateMenu};
