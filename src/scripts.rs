//! The scripts of this machine, and the one the window is talking to.
//!
//! A script runs on a thread of its own and says what it is doing through
//! [`Talker`], which is a sender and nothing more: the thread that draws
//! drains what came once a frame. A dialog is the one call that waits — the
//! script has nothing to do until it is answered — so it goes out as a
//! [`Pending`] carrying the channel the answer comes back on.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use zyt_config::ConfigStore;
use zyt_script::{Form, Library, NoticeKind, Progress, Prompt, ScriptEvent, Value, default_roots};

/// A dialog a script is waiting on.
pub struct Pending {
    /// What it asks.
    pub form: Form,
    /// Where the answer goes.
    answer: Sender<Option<BTreeMap<String, Value>>>,
}

impl Pending {
    /// Answers it, or waves it away with nothing.
    ///
    /// A script whose dialog was dropped without an answer is told the same as
    /// one whose window was closed: the sender going away is an answer of
    /// nothing, so there is no state in which it waits for ever.
    pub fn answer(self, answers: Option<BTreeMap<String, Value>>) {
        let _ = self.answer.send(answers);
    }
}

/// What a running script says, for the thread that draws.
pub struct Talker {
    events: Sender<ScriptEvent>,
    asks: Sender<Pending>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Prompt for Talker {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        let (sender, answer) = channel();
        if self
            .asks
            .send(Pending {
                form,
                answer: sender,
            })
            .is_err()
        {
            return None;
        }
        (self.wake)();
        answer.recv().ok().flatten()
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
}

impl Talker {
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
    pub asks: Receiver<Pending>,
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
