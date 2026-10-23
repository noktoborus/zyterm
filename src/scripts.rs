//! The scripts of this machine, and the one the window is talking to.
//!
//! A script runs on a thread of its own and says what it is doing through
//! [`Talker`]. What it shows in the terminal — its echo and the lines of its
//! notices — goes to the screen through a buffer of its own, in the order it
//! was said, and wakes the screen: no frame of the window is on the way. The
//! rest is a sender and nothing more: the thread that draws drains what came
//! once a frame. A dialog is the one call that waits — the
//! script has nothing to do until it is answered — so it goes out as a
//! [`Waiting`] carrying the channel the answer comes back on: a form to fill,
//! or paths to pick with the dialog of the file system.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use zyt_config::ConfigStore;
use zyt_script::{
    ByteSwap, Form, Library, NoticeKind, Pick, Progress, Prompt, ScriptEvent, Speed, Stage, Value,
    default_roots,
};

/// A question a script is waiting on, and where its answer goes.
pub struct Asking<Q, A> {
    /// What it asks.
    pub question: Q,
    /// Where the answer goes.
    answer: Sender<Option<A>>,
    /// The run that asked it, once the session has handed it over: a question
    /// whose run is gone is answered with nothing and put away.
    run: Option<u64>,
}

impl<Q, A> Asking<Q, A> {
    /// Answers it, or waves it away with nothing.
    ///
    /// A script whose question was dropped without an answer is told the same
    /// as one whose window was closed: the sender going away is an answer of
    /// nothing, so there is no state in which it waits for ever.
    pub fn answer(self, answer: Option<A>) {
        let _ = self.answer.send(answer);
    }

    /// The number of the run that asked it, when one did.
    pub fn run(&self) -> Option<u64> {
        self.run
    }

    /// Marks it as asked by that run.
    pub fn of_run(mut self, run: u64) -> Self {
        self.run = Some(run);
        self
    }
}

/// A dialog a script is waiting on.
pub type Pending = Asking<Form, BTreeMap<String, Value>>;

/// A dialog of the file system a script is waiting on.
pub type Picking = Asking<Pick, Vec<PathBuf>>;

/// One of the two things a script waits on the window for.
pub enum Waiting {
    /// A form to fill.
    Form(Pending),
    /// Paths to pick.
    Pick(Picking),
}

impl Waiting {
    /// Waves it away with nothing.
    pub fn dismiss(self) {
        match self {
            Self::Form(pending) => pending.answer(None),
            Self::Pick(picking) => picking.answer(None),
        }
    }

    /// Marks it as asked by that run.
    pub fn of_run(self, run: u64) -> Self {
        match self {
            Self::Form(pending) => Self::Form(pending.of_run(run)),
            Self::Pick(picking) => Self::Pick(picking.of_run(run)),
        }
    }
}

/// How much of what a script shows waits for the screen before a write of
/// it waits for room.
const SHOWN_ROOM: usize = 64 * 1024;
/// How long a write that waits for room sleeps before it looks again.
const SHOWN_SLICE: std::time::Duration = std::time::Duration::from_millis(5);

/// The line of a notice, as the terminal shows it: a mark in the colour of
/// its kind, so it is not mistaken for output of the device.
pub fn notice_line(kind: NoticeKind, text: &str) -> Vec<u8> {
    let color = match kind {
        NoticeKind::Error => "\x1b[31m",
        NoticeKind::Info => "\x1b[36m",
    };
    format!("{color}\u{2502}\x1b[0m {text}\r\n").into_bytes()
}

/// The features of the terminal a run switched off for its time
/// (`zyt.term.features`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Off {
    /// OSC 9 and 777.
    pub notifications: bool,
    /// OSC 7.
    pub directory: bool,
    /// OSC 52.
    pub clipboard: bool,
    /// OSC 9;4.
    pub progress: bool,
    /// `BEL`.
    pub bell: bool,
    /// OSC 133.
    pub marks: bool,
}

impl Off {
    /// The settings with what is switched off taken out.
    pub fn apply(&self, mut osc: crate::config::OscSettings) -> crate::config::OscSettings {
        osc.notification_text &= !self.notifications;
        osc.notification_titled &= !self.notifications;
        osc.directory &= !self.directory;
        osc.progress &= !self.progress;
        osc.marks &= !self.marks;
        if self.clipboard {
            osc.clipboard = crate::config::ClipboardSetting::Disabled;
        }
        osc
    }
}

/// What reads the state of the terminal, and how many bytes of the echo of
/// the run it was fed (`ScreenHandle::state_reader`).
pub type StateReader = Arc<dyn Fn() -> Option<(zyt_term::TermState, u64)> + Send + Sync>;

/// What a running script says, for the thread that draws.
pub struct Talker {
    events: Sender<ScriptEvent>,
    asks: Sender<Waiting>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// What the script shows in the terminal, for the screen (`to_screen`).
    shown: Arc<ByteSwap>,
    /// What wakes the screen.
    screen: Arc<dyn Fn() + Send + Sync>,
    /// What reads the state of the terminal, and how much of `shown` it holds.
    state: StateReader,
    /// How many bytes went into `shown`.
    written: std::sync::atomic::AtomicU64,
    /// What the run switched off.
    off: Arc<std::sync::Mutex<Off>>,
    /// The ask of the run to stop: a dialog waited on is let go of when it
    /// comes, as every other wait of the host is.
    cancel: Arc<zyt_script::Cancel>,
}

impl Prompt for Talker {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        self.wait(form, Waiting::Form)
    }

    fn pick(&self, pick: Pick) -> Option<Vec<PathBuf>> {
        self.wait(pick, Waiting::Pick)
    }

    fn notice(&self, kind: NoticeKind, text: String) {
        log::info!("script: {text}");
        self.show(&notice_line(kind, &text));
    }

    fn echo(&self, bytes: &[u8]) {
        self.show(bytes);
    }

    fn features(&self, switches: &BTreeMap<String, bool>) -> Result<(), String> {
        let mut off = *self.off.lock().unwrap_or_else(|error| error.into_inner());
        for (name, on) in switches {
            let field = match name.as_str() {
                "notifications" => &mut off.notifications,
                "directory" => &mut off.directory,
                "clipboard" => &mut off.clipboard,
                "progress" => &mut off.progress,
                "bell" => &mut off.bell,
                "marks" => &mut off.marks,
                _ => return Err(name.clone()),
            };
            *field = !on;
        }
        *self.off.lock().unwrap_or_else(|error| error.into_inner()) = off;
        (self.wake)();
        Ok(())
    }

    /// Waits until the terminal was fed every byte shown, in slices, and
    /// reads the ask to stop between them.
    fn term_state(&self, into: &mut Vec<u8>) -> bool {
        let written = self.written.load(std::sync::atomic::Ordering::Acquire);
        loop {
            match (self.state)() {
                Some((state, fed)) if fed >= written => {
                    state.write_bytes(into);
                    return true;
                }
                _ if self.cancel.asked() => return false,
                _ => std::thread::sleep(SHOWN_SLICE),
            }
        }
    }

    fn progress(&self, progress: Progress) {
        self.say(ScriptEvent::Progress(progress));
    }

    fn stage(&self, stage: Stage) {
        self.say(ScriptEvent::Stage(stage));
    }

    fn bytes(&self, done: u64, whole: Option<u64>) {
        self.say(ScriptEvent::Bytes { done, whole });
    }

    fn speed(&self, speed: Speed) {
        self.say(ScriptEvent::Speed(speed));
    }

    fn ending(&self) {
        self.say(ScriptEvent::Ending);
    }
}

impl Talker {
    /// Hands a question to the window and waits for its answer.
    fn wait<Q, A>(&self, question: Q, waiting: fn(Asking<Q, A>) -> Waiting) -> Option<A> {
        let (sender, answer) = channel();
        let asking = Asking {
            question,
            answer: sender,
            run: None,
        };
        if self.asks.send(waiting(asking)).is_err() {
            return None;
        }
        (self.wake)();
        loop {
            match answer.recv_timeout(DIALOG_SLICE) {
                Ok(answered) => return answered,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if self.cancel.asked() => {
                    return None;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Hands bytes to the screen, and wakes it. While the screen has not
    /// taken [`SHOWN_ROOM`] bytes yet, the write waits, and reads the ask to
    /// stop between its slices.
    fn show(&self, bytes: &[u8]) {
        while self.shown.waiting() >= SHOWN_ROOM && !self.cancel.asked() {
            std::thread::sleep(SHOWN_SLICE);
        }
        self.shown.push(bytes);
        self.written
            .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Release);
        (self.screen)();
    }

    /// Sends one thing and wakes the window for it.
    fn say(&self, event: ScriptEvent) {
        if self.events.send(event).is_ok() {
            (self.wake)();
        }
    }
}

/// What the window drains while a script runs.
pub struct Heard {
    /// Everything the script said.
    pub events: Receiver<ScriptEvent>,
    /// Every dialog it is waiting on.
    pub asks: Receiver<Waiting>,
}

/// A talker for one run, and the ends the window listens on. What it shows
/// goes into `shown` and rings `screen`; `state` reads the terminal back. A dialog it waits on is let go of
/// once `cancel` is asked.
pub fn talker(
    wake: Arc<dyn Fn() + Send + Sync>,
    shown: Arc<ByteSwap>,
    screen: Arc<dyn Fn() + Send + Sync>,
    state: StateReader,
    off: Arc<std::sync::Mutex<Off>>,
    cancel: Arc<zyt_script::Cancel>,
) -> (Arc<Talker>, Heard) {
    let (events, heard) = channel();
    let (asks, waiting) = channel();
    (
        Arc::new(Talker {
            events,
            asks,
            wake,
            shown,
            screen,
            state,
            written: Default::default(),
            off,
            cancel,
        }),
        Heard {
            events: heard,
            asks: waiting,
        },
    )
}

/// How long a dialog is waited on before the ask to stop is looked at again.
const DIALOG_SLICE: std::time::Duration = std::time::Duration::from_millis(50);

/// The directories scripts are looked for in, in the order they are searched.
pub fn roots(store: &ConfigStore) -> Vec<std::path::PathBuf> {
    default_roots(
        crate::APP_NAME,
        Some(store.data_dir()),
        Some(store.config_dir()),
    )
}

/// Every script of this machine, and what could not be read as one.
///
/// A file that fails to load is kept as a problem rather than taking the list
/// with it: a script somebody is writing is broken most of the time, and the
/// others have to go on working while it is.
pub fn load(store: &ConfigStore) -> Library {
    let library = Library::load(&roots(store));
    for problem in library.problems() {
        log::warn!(
            "{} is not a script: {}",
            problem.path.display(),
            problem.said
        );
    }
    library
}

/// The progress of a script as the terminal and the taskbar take it.
pub fn progress_of(progress: Progress) -> Option<zyt_term::ProgressState> {
    match progress {
        Progress::Removed => None,
        Progress::Share(share) => Some(zyt_term::ProgressState::Set(share)),
        Progress::Error(share) => Some(zyt_term::ProgressState::Error(share)),
        Progress::Paused(share) => Some(zyt_term::ProgressState::Paused(share)),
        Progress::Indeterminate => Some(zyt_term::ProgressState::Indeterminate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store pointing at directories of this test, with nothing in them.
    fn store(case: &str) -> (ConfigStore, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("zyterm-scripts-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config")).expect("the directory is made");
        std::fs::create_dir_all(root.join("data")).expect("the directory is made");
        (
            ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("lock")),
            root,
        )
    }

    #[test]
    fn the_directory_somebody_writes_in_is_searched_first() {
        let (store, root) = store("order");
        let roots = roots(&store);

        assert_eq!(
            roots.first(),
            Some(&store.config_dir().join(zyt_script::SCRIPTS)),
            "a script of one's own is the one that runs: {roots:?}"
        );
        assert_eq!(
            roots.get(1),
            Some(&store.data_dir().join(zyt_script::SCRIPTS))
        );
        assert!(roots.len() >= 3, "{roots:?}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_scripts_shipped_with_the_program_are_found_from_a_build_of_it() {
        let (store, root) = store("shipped");
        let library = load(&store);

        let names: Vec<&str> = library
            .entries()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect();
        assert!(
            names.contains(&"shell-transfer"),
            "nothing of the program was found: {names:?}"
        );
        assert!(
            library.problems().is_empty(),
            "nothing shipped is broken: {:?}",
            library.problems()
        );

        let entry = library.find("shell-transfer").expect("it is there");
        assert!(entry.manifest.hold_line);
        assert!(entry.manifest.offers(zyt_script::Direction::Send));

        let _ = std::fs::remove_dir_all(root);
    }

    /// A run switches features off and gives them back; a feature nobody
    /// knows is named in the error.
    #[test]
    fn a_run_switches_features_and_names_an_unknown_one() {
        let off = Arc::new(std::sync::Mutex::new(Off::default()));
        let (talker, _heard) = talker(
            Arc::new(|| {}),
            ByteSwap::with_capacity(0),
            Arc::new(|| {}),
            Arc::new(|| None),
            off.clone(),
            zyt_script::Cancel::new(),
        );
        let switches = |pairs: &[(&str, bool)]| -> BTreeMap<String, bool> {
            pairs
                .iter()
                .map(|(name, on)| (name.to_string(), *on))
                .collect()
        };

        assert!(
            talker
                .features(&switches(&[("bell", false), ("clipboard", false)]))
                .is_ok()
        );
        let switched = *off.lock().unwrap();
        assert!(switched.bell && switched.clipboard && !switched.marks);
        let osc = switched.apply(crate::config::OscSettings::trusted());
        assert_eq!(osc.clipboard, crate::config::ClipboardSetting::Disabled);

        assert!(talker.features(&switches(&[("bell", true)])).is_ok());
        assert!(!off.lock().unwrap().bell, "given back");
        assert_eq!(
            talker.features(&switches(&[("colours", false)])),
            Err("colours".to_string())
        );
    }
}
