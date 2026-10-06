//! What a script is given: the line, and the one it is talking to.
//!
//! Both traits are implemented by whoever runs the script, and every call of
//! them is made from the thread the script runs on. An implementation that
//! draws therefore hands the work to the thread that draws and waits for the
//! answer: [`Prompt::ask`] blocking is the whole point of it, because a script
//! asking a question has nothing to do until it is answered.

use crate::form::{Form, Value};
use std::collections::BTreeMap;
use std::time::Duration;

/// The colour a notice is printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// Something happened and it is going to plan.
    Info,
    /// Something failed.
    Error,
}

/// How far along a script says it is.
///
/// It is the crate's own type: the application decides what the bar and the
/// taskbar of the platform make of it, and nothing here knows either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// This share of the work is done, from zero to a hundred.
    Share(u8),
    /// Work is going on and how much of it is not known.
    Indeterminate,
    /// It failed at this share.
    Error(u8),
    /// It is waiting at this share.
    Paused(u8),
    /// There is nothing to show any more.
    Removed,
}

impl Progress {
    /// This share of the work, brought back inside a hundred.
    pub fn share(share: i64) -> Self {
        Self::Share(share.clamp(0, 100) as u8)
    }
}

/// The line of the session, as a script sees it.
///
/// The script never opens a port: the application keeps it and moves bytes
/// between the device and this channel once a frame, which is why writing
/// answers at once and reading waits with a deadline.
pub trait Line: Send + Sync {
    /// Queues bytes for the device.
    fn write(&self, bytes: &[u8]);

    /// Takes whatever the device said into `into`, which is cleared first.
    ///
    /// It waits up to `timeout` for the first byte and answers with nothing
    /// when none came.
    fn read(&self, into: &mut Vec<u8>, timeout: Duration);

    /// How many bytes have been queued for the device and not yet left it.
    fn pending_output(&self) -> usize;

    /// How many bytes the device has said that nobody has taken yet.
    ///
    /// It is a look and not a read: a script waiting for the device to answer
    /// at all — the echo of a command line, the first frame of a handshake —
    /// asks this rather than reading, because what it read would be gone from
    /// the program it is about to hand the line to.
    fn waiting(&self) -> usize {
        0
    }

    /// True while the line is still there.
    fn is_open(&self) -> bool;

    /// Shuts the line, so what the script does from here reaches nothing.
    ///
    /// It is what a run that could not be stopped is given up on with. A line
    /// that is not a channel of this crate has nothing to shut and says so by
    /// doing nothing.
    fn close(&self) {}
}

/// The one the script is talking to.
pub trait Prompt: Send + Sync {
    /// Asks the user what the form asks and waits for the answer.
    ///
    /// Nothing is a question waved away; the script is expected to stop.
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>>;

    /// Says one thing in the frame and colour of the application.
    fn notice(&self, kind: NoticeKind, text: String);

    /// Prints bytes straight into the terminal, as the device would be shown.
    ///
    /// While a script holds the line the device is not drawn, so this is the
    /// only way a script shows anything of its own. The bytes are written as
    /// they stand: escape sequences in them do what they say.
    fn echo(&self, bytes: &[u8]);

    /// Says how far along the script is.
    fn progress(&self, progress: Progress);
}

/// One thing a running script reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptEvent {
    /// A notice, in the frame of the application.
    Note {
        /// Which colour it is printed in.
        kind: NoticeKind,
        /// What it says.
        text: String,
    },
    /// Bytes for the terminal, as they stand.
    Echo(Vec<u8>),
    /// How far along it is.
    Progress(Progress),
    /// It ended, and this is how.
    Finished {
        /// How it ended.
        outcome: crate::detached::Outcome,
    },
}
