//! The scripts of this machine, and the one the window is talking to.
//!
//! A script runs on a thread of its own and says what it is doing through
//! [`Talker`], which is a sender and nothing more: the thread that draws
//! drains what came once a frame. A dialog is the one call that waits — the
//! script has nothing to do until it is answered — so it goes out as a
//! [`Waiting`] carrying the channel the answer comes back on: a form to fill,
//! or paths to pick with the dialog of the file system.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use zyt_config::ConfigStore;
use zyt_script::{
    Form, Library, NoticeKind, Pick, Progress, Prompt, ScriptEvent, Speed, Stage, Value,
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

/// What a running script says, for the thread that draws.
pub struct Talker {
    events: Sender<ScriptEvent>,
    asks: Sender<Waiting>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Prompt for Talker {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        self.wait(form, Waiting::Form)
    }

    fn pick(&self, pick: Pick) -> Option<Vec<PathBuf>> {
        self.wait(pick, Waiting::Pick)
    }

    fn notice(&self, kind: NoticeKind, text: String) {
        self.say(ScriptEvent::Note { kind, text });
    }

    fn echo(&self, bytes: &[u8]) {
        self.say(ScriptEvent::Echo(bytes.to_vec()));
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
        answer.recv().ok().flatten()
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

/// A talker for one run, and the ends the window listens on.
pub fn talker(wake: Arc<dyn Fn() + Send + Sync>) -> (Arc<Talker>, Heard) {
    let (events, heard) = channel();
    let (asks, waiting) = channel();
    (
        Arc::new(Talker { events, asks, wake }),
        Heard {
            events: heard,
            asks: waiting,
        },
    )
}

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
}
