//! Transfer scripts: what a script is given and what it may start.
//!
//! A script replaces the external transfer program. It is handed the line of
//! the session, a way to ask the user something, and a way to start programs
//! of its own; what it does with them is its business. The crate carries the
//! host side of that: the buffers the line moves through, the processes a
//! script started and how they are stopped, the description of a dialog, and
//! the types both ends agree on.
//!
//! Nothing here draws and nothing here reads a configuration file. A dialog is
//! [`Form`], plain data; whoever drew the window answers with [`Value`]s. The
//! application implements [`Line`] and [`Prompt`], so the same script runs
//! against a window, against a pseudo terminal or against a written down
//! conversation.
//!
//! A program a script starts is put in a group of its own and kept in a
//! [`ProcessRegistry`], because a script that was stopped must not leave a
//! program holding the line.

#![deny(missing_docs)]

pub mod cli;

mod api;
mod channel;
mod chunks;
mod detached;
mod engine;
mod error;
mod finish;
mod form;
mod host;
mod interrupt;
mod jobobject;
mod manifest;
mod process;
mod quote;
mod registry;
mod run;
mod runner;
mod search;
mod target;

pub use api::script::{fill, fold};
pub use channel::LineChannel;
pub use chunks::ByteSwap;
pub use detached::{DetachedJob, Outcome};
pub use engine::{Engine, Start, Talking, check};
pub use error::{Result, ScriptError};
pub use finish::{FINISH_NONE, FINISH_PRESETS, finish_bytes, finish_label};
pub use form::{Choice, Field, FieldKind, Form, Value};
pub use host::{Line, NoticeKind, Progress, Prompt, ScriptEvent};
pub use interrupt::{ABANDON_AFTER, CLEANUP_BUDGET, Cancel};
pub use manifest::{DEFAULT_ENTRY, DirectionSpec, MANIFEST, Manifest};
pub use process::{own_group, shell, shell_command, spawn_thread, stop_group};
pub use quote::{quote_for_shell, quote_posix};
pub use registry::{ProcId, ProcessRegistry};
pub use run::ScriptRun;
pub use runner::{JobId, JobRunner, JobState};
pub use search::{
    Entry, Library, Problem, SCRIPTS, default_roots, executable_dir, shipped_dirs, system_dirs,
    user_dirs,
};
pub use target::{Direction, Target, TargetKind, Targets};
