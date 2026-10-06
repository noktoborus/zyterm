//! The harness the tests of a script share: what it said, and how it is run.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zyt_script::{
    Cancel, Direction, Form, LineChannel, NoticeKind, Outcome, Progress, Prompt, ScriptRun, Start,
    Targets, Value,
};

/// What the script said, in the order it said it.
#[derive(Debug, Default)]
pub struct Said {
    pub echoed: Mutex<Vec<u8>>,
    pub notices: Mutex<Vec<String>>,
    pub progress: Mutex<Vec<Progress>>,
    pub asked: Mutex<Vec<Form>>,
    pub answers: Mutex<Option<BTreeMap<String, Value>>>,
}

impl Said {
    /// The text of everything printed into the terminal.
    pub fn echo(&self) -> String {
        String::from_utf8_lossy(&self.echoed.lock().expect("it is not held")).to_string()
    }
}

impl Prompt for Said {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        let mut answers = form.defaults();
        if let Ok(given) = self.answers.lock()
            && let Some(given) = given.as_ref()
        {
            for (name, value) in given {
                answers.insert(name.clone(), value.clone());
            }
        }
        self.asked.lock().expect("it is not held").push(form);
        Some(answers)
    }

    fn notice(&self, _: NoticeKind, text: String) {
        self.notices.lock().expect("it is not held").push(text);
    }

    fn echo(&self, bytes: &[u8]) {
        self.echoed
            .lock()
            .expect("it is not held")
            .extend_from_slice(bytes);
    }

    fn progress(&self, progress: Progress) {
        self.progress.lock().expect("it is not held").push(progress);
    }
}

/// The directory the fixture scripts stand in.
pub fn scripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("scripts")
}

/// A start for one of the fixture scripts.
pub fn start(name: &str, line: Arc<LineChannel>, said: Arc<Said>, cancel: Arc<Cancel>) -> Start {
    let roots = vec![scripts()];
    let library = zyt_script::Library::load(&roots);
    let entry = library
        .find(name)
        .unwrap_or_else(|error| panic!("the fixture {name} is there: {error}"))
        .clone();

    Start::of_entry(
        &entry,
        &roots,
        Direction::Send,
        Targets::none(),
        BTreeMap::new(),
        zyt_script::Talking {
            line,
            prompt: said,
            cancel,
        },
    )
}

/// Runs the script to its end, or fails saying it never ended.
pub fn run_to_end(run: &mut ScriptRun, how_long: Duration) -> Outcome {
    let deadline = Instant::now() + how_long;
    while !run.tick() {
        if Instant::now() >= deadline {
            panic!("the script never ended");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    run.outcome().expect("it says how it ended")
}
