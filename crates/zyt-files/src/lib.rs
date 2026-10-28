//! File operations of a terminal, run as cancellable tasks.
//!
//! The crate does three things: it says where the address of a link leads and
//! what kind of content a file holds, it moves, trashes, removes and reads
//! files in chunks that can be stopped, and it runs those operations on threads
//! of their own, reporting progress and the end of each one by number.
//!
//! It holds no text of the interface and knows nothing of the caller.

#![deny(missing_docs)]

mod error;
mod ops;
mod task;
mod uri;

pub use error::{FileError, Result};
pub use ops::size_of;
pub use task::{Done, FileTask, SharedRunner, TaskEvent, TaskId, TaskKind, TaskRunner, TaskState};
pub use uri::{Content, Place, content_of, place_of};
