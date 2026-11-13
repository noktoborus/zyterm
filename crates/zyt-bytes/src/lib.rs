//! The transport of payload bytes between two threads (`RULE
//! payload.transport`): a port or a console and the window, a script and the
//! line.
#![deny(missing_docs)]

mod swap;

pub use swap::ByteSwap;
