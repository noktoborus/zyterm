//! What a script is given: the line, and the one it is talking to.
//!
//! Both traits are implemented by whoever runs the script, and every call of
//! them is made from the thread the script runs on. An implementation that
//! draws therefore hands the work to the thread that draws and waits for the
//! answer: [`Prompt::ask`] blocking is the whole point of it, because a script
//! asking a question has nothing to do until it is answered.

use crate::form::{Form, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
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

/// What a script asks the dialog of the file system for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    /// One file that is there.
    File,
    /// Any number of files that are there.
    Files,
    /// One directory.
    Directory,
    /// Any number of directories.
    Directories,
    /// A path to write a file to, which may not be there yet.
    Save,
}

impl PickKind {
    /// The kind a script names, or nothing for a name that is none of them.
    pub fn of_name(name: &str) -> Option<Self> {
        Some(match name {
            "file" => Self::File,
            "files" => Self::Files,
            "directory" => Self::Directory,
            "directories" => Self::Directories,
            "save" => Self::Save,
            _ => return None,
        })
    }

    /// The word a script writes for this kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Files => "files",
            Self::Directory => "directory",
            Self::Directories => "directories",
            Self::Save => "save",
        }
    }

    /// True when more than one path may be picked.
    pub fn is_multiple(self) -> bool {
        matches!(self, Self::Files | Self::Directories)
    }

    /// True when what is picked is a directory.
    pub fn is_directory(self) -> bool {
        matches!(self, Self::Directory | Self::Directories)
    }
}

/// A question for the dialog of the file system, as a script describes it.
///
/// It is data, like [`Form`]: which dialog it is and what stands in its title
/// is the script's, how it is drawn and where it opens is the application's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    /// What is picked.
    pub kind: PickKind,
    /// What the dialog says it is for.
    pub title: String,
    /// The name a [`PickKind::Save`] offers to begin with.
    pub name: Option<String>,
}

/// How fast a run goes now, each way, in bytes a second, as the script
/// measured it: a way it says nothing of is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Speed {
    /// What comes to this side.
    pub received: Option<u64>,
    /// What goes to the far side.
    pub sent: Option<u64>,
}

/// The parameters of a serial line, as a script sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortParams {
    /// Bits a second.
    pub baud: u32,
    /// Data bits of a character, 5 to 8.
    pub data_bits: u8,
    /// `none`, `odd` or `even`.
    pub parity: String,
    /// Stop bits, 1 or 2.
    pub stop_bits: u8,
    /// `none`, `software`, `hardware` or `both`.
    pub flow: String,
}

/// What a script drives on a serial line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSwitch {
    /// Request To Send held up, held down, or left to the driver.
    Rts(Option<bool>),
    /// Data Terminal Ready held up, held down, or left to the driver.
    Dtr(Option<bool>),
    /// The break condition on or off.
    Break(bool),
    /// The reading of the port held, or let go.
    Hold(bool),
    /// What waits to be sent thrown away.
    Purge,
}

/// The lines of the modem as they stand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PortLines {
    /// Request To Send.
    pub rts: bool,
    /// Data Terminal Ready.
    pub dtr: bool,
    /// Clear To Send.
    pub cts: bool,
    /// Data Set Ready.
    pub dsr: bool,
    /// Data Carrier Detect.
    pub cd: bool,
    /// Ring Indicator.
    pub ri: bool,
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

    /// The parameters of the serial port behind the line; nothing when the
    /// line is no serial port.
    fn port(&self) -> Option<PortParams> {
        None
    }

    /// Sets the parameters of the serial port; false when there is none or
    /// it does not take them.
    fn set_port(&self, _params: &PortParams) -> bool {
        false
    }

    /// Drives the serial port; false when there is none.
    fn switch_port(&self, _switch: PortSwitch) -> bool {
        false
    }

    /// The lines of the modem of the serial port; nothing when there is none.
    fn port_lines(&self) -> Option<PortLines> {
        None
    }

    /// Gives the line the size of a far terminal, in cells. True when the
    /// line took it; a line that carries no size answers false.
    fn resize(&self, _columns: u16, _rows: u16) -> bool {
        false
    }

    /// Shuts the line, so what the script does from here reaches nothing.
    ///
    /// It is what a run that could not be stopped is given up on with. A line
    /// that is not a channel of this crate has nothing to shut and says so by
    /// doing nothing.
    fn close(&self) {}

    /// What to call when the device says something, while a wait of the
    /// script stands (`zyt.wait`); nothing takes it away. A line that cannot
    /// call it leaves the wait to find the bytes at its next slice.
    fn set_wake(&self, _wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>) {}
}

/// The one the script is talking to.
pub trait Prompt: Send + Sync {
    /// Asks the user what the form asks and waits for the answer.
    ///
    /// Nothing is a question waved away; the script is expected to stop.
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>>;

    /// Asks the user for paths with the dialog of the file system and waits.
    ///
    /// Nothing is a dialog closed with nothing picked; the script is expected
    /// to stop, as it does when a form is waved away.
    fn pick(&self, pick: Pick) -> Option<Vec<PathBuf>>;

    /// Says one thing in the frame and colour of the application.
    fn notice(&self, kind: NoticeKind, text: String);

    /// Prints bytes straight into the terminal, as the device would be shown.
    ///
    /// While a script holds the line the device is not drawn, so this is the
    /// only way a script shows anything of its own. The bytes are written as
    /// they stand: escape sequences in them do what they say.
    fn echo(&self, bytes: &[u8]);

    /// Writes into `into` the bytes that give a terminal that joins late the
    /// state of the terminal: its modes, character sets, cursor and title,
    /// after every byte [`Self::echo`] was given. False when there is none.
    fn term_state(&self, _into: &mut Vec<u8>) -> bool {
        false
    }

    /// Switches features of the terminal for the time of the run: `false`
    /// switches one off, `true` gives it back as the settings have it. The
    /// error names a feature the application does not know.
    fn features(&self, _switches: &BTreeMap<String, bool>) -> Result<(), String> {
        Ok(())
    }

    /// Says how far along the script is.
    fn progress(&self, progress: Progress);

    /// Says where the run stands. [`Progress::Removed`] puts it away too.
    fn stage(&self, _stage: Stage) {}

    /// Says how many bytes went, and of how many when that is known.
    /// [`Progress::Removed`] puts them away too.
    fn bytes(&self, _done: u64, _whole: Option<u64>) {}

    /// Says how fast the run goes now, as the script measured it. It is shown
    /// as it is said; nothing is worked out from it. [`Progress::Removed`] puts
    /// it away too.
    fn speed(&self, _speed: Speed) {}

    /// Says the script has begun to end (`zyt.run.ending()`), asked to stop
    /// or by itself.
    fn ending(&self) {}
}

/// One thing a running script reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptEvent {
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
    /// How fast it goes now, as the script said it.
    Speed(Speed),
    /// It has begun to end.
    Ending,
    /// It ended, and this is how.
    Finished {
        /// How it ended.
        outcome: crate::detached::Outcome,
    },
}
