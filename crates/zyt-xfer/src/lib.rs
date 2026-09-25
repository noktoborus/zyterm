//! File transfer through external programs.
//!
//! The crate never implements a transfer protocol. A profile holds two command
//! lines per direction: a local one, started here with pipes on all three
//! standard channels, and a remote one, which the caller types into the console
//! of the device so the counterpart starts itself. Bytes move between the pipes
//! and the caller, which keeps the line open the whole time; diagnostic output
//! of the local program becomes [`TransferEvent::Log`].
//!
//! Every program runs on pipes and reads no keys. What it has to say it says on
//! its diagnostic channel, line by line, and the caller shows those lines where
//! they belong; nothing here draws.
//!
//! That is the profile that says `pty`. A profile that does not is not on the
//! line at all: it reaches the device some other way, so it is given no input,
//! everything it says goes into a text file of its own, and any number of them
//! run beside each other. [`JobRunner`] keeps those by number.

#![deny(missing_docs)]

mod chunks;
mod detached;
mod error;
mod gateway;
mod process;
mod profile;
mod runner;

pub use detached::{DetachedJob, Outcome};
pub use error::{Result, XferError};
pub use gateway::{TransferEvent, TransferJob};
pub use profile::{
    CommandLine, CommandStep, DEFAULT_DELAY_MS, DIRECTORIES_PLACEHOLDER, DIRECTORY_PLACEHOLDER,
    Direction, FILE_PLACEHOLDER, FILENAME_PLACEHOLDER, FILES_PLACEHOLDER, FINISH_NONE,
    FINISH_PRESETS, SCP_TO_REMOTE_PWD, STEM_PLACEHOLDER, SUFFIX_PLACEHOLDER, Target, TargetKind,
    TransferCommands, TransferProfile, default_profiles, finish_bytes, is_variable_name,
    quote_for_shell, variable_names,
};
pub use runner::{JobId, JobRunner, JobState};
