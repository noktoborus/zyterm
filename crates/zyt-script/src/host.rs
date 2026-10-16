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

/// What a stage of a run is, in the crate's own words: the application says
/// them in its language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    /// Getting ready: asking, looking, before anything goes on the line.
    Preparing,
    /// Telling the device what to run.
    Telling,
    /// Reading what the device holds.
    Listing,
    /// Waiting for the other end to agree on a transfer.
    Negotiating,
    /// A file going to the device.
    Sending,
    /// A file coming from the device.
    Receiving,
    /// Asking again after the line said nothing, or something damaged.
    Retrying,
    /// Closing the transfer with the other end.
    Finishing,
    /// Something the kinds above do not name, said by `text`.
    Other,
}

impl StageKind {
    /// The kind a script names, or nothing for a name that is none of them.
    pub fn of_name(name: &str) -> Option<Self> {
        Some(match name {
            "preparing" => Self::Preparing,
            "telling" => Self::Telling,
            "listing" => Self::Listing,
            "negotiating" => Self::Negotiating,
            "sending" => Self::Sending,
            "receiving" => Self::Receiving,
            "retrying" => Self::Retrying,
            "finishing" => Self::Finishing,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

/// Where a script says its run stands.
///
/// It is data and not a sentence: what each kind is called, and how the file
/// and the count are said, is the application's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// What is going on.
    pub kind: StageKind,
    /// The file it is about.
    pub name: Option<String>,
    /// Which file of the run it is, from one.
    pub file: Option<u32>,
    /// How many files the run has, when that is known.
    pub files: Option<u32>,
    /// How many times something was tried again.
    pub count: Option<u32>,
    /// What the script says, for [`StageKind::Other`] or beside any kind.
    pub text: Option<String>,
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

    /// Says where the run stands. [`Progress::Removed`] puts it away too.
    fn stage(&self, _stage: Stage) {}

    /// Says how many bytes went, and of how many when that is known.
    /// [`Progress::Removed`] puts them away too.
    fn bytes(&self, _done: u64, _whole: Option<u64>) {}
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
    /// Where it stands.
    Stage(Stage),
    /// How many bytes went, of how many.
    Bytes {
        /// What went so far.
        done: u64,
        /// The whole, when it is known.
        whole: Option<u64>,
    },
    /// It ended, and this is how.
    Finished {
        /// How it ended.
        outcome: crate::detached::Outcome,
    },
}
