//! The harness the tests of a script share: what it said, and how it is run.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zyt_script::{
    Cancel, Direction, Form, LineChannel, NoticeKind, Outcome, Pick, Progress, Prompt, ScriptRun,
    Speed, Stage, Start, Targets, Value,
};

/// What the script said, in the order it said it.
#[derive(Debug, Default)]
pub struct Said {
    pub echoed: Mutex<Vec<u8>>,
    pub notices: Mutex<Vec<String>>,
    pub progress: Mutex<Vec<Progress>>,
    pub stages: Mutex<Vec<Stage>>,
    pub bytes: Mutex<Vec<(u64, Option<u64>)>>,
    pub asked: Mutex<Vec<Form>>,
    /// What the dialog of the file system answers, every time it is asked.
    pub picks: Mutex<Vec<PathBuf>>,
    /// Every dialog of the file system the script asked, in order.
    pub picked: Mutex<Vec<Pick>>,
    /// Every speed the script said.
    pub speeds: Mutex<Vec<Speed>>,
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

    fn pick(&self, pick: Pick) -> Option<Vec<PathBuf>> {
        let multiple = pick.kind.is_multiple();
        self.picked.lock().expect("it is not held").push(pick);
        let picks = self.picks.lock().expect("it is not held").clone();
        match (picks.is_empty(), multiple) {
            (true, _) => None,
            (false, true) => Some(picks),
            (false, false) => Some(picks[..1].to_vec()),
        }
    }

    fn speed(&self, speed: Speed) {
        self.speeds.lock().expect("it is not held").push(speed);
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

    fn stage(&self, stage: Stage) {
        self.stages.lock().expect("it is not held").push(stage);
    }

    fn bytes(&self, done: u64, whole: Option<u64>) {
        self.bytes
            .lock()
            .expect("it is not held")
            .push((done, whole));
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
