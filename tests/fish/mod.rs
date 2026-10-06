//! The harness the tests of the shipped scripts share.
//!
//! A script is run the way the application runs it: the engine, the host calls
//! and the files of `scripts/`. What stands at the far end of the line is what
//! each test decides — a string of replies written down beforehand, a real
//! shell over pipes, or a shell under a pseudo terminal.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zyt_script::{
    Cancel, Direction, Form, LineChannel, NoticeKind, Outcome, Progress, Prompt, ScriptRun, Start,
    TargetKind, Targets, Value,
};

/// What a script said, and what it was answered.
#[derive(Debug, Default)]
pub struct Said {
    pub echoed: Mutex<Vec<u8>>,
    pub notices: Mutex<Vec<String>>,
    pub errors: Mutex<Vec<String>>,
    pub progress: Mutex<Vec<Progress>>,
    pub answers: Mutex<BTreeMap<String, Value>>,
}

impl Said {
    /// A prompt answering the fields of every dialog with these values.
    pub fn answering(answers: &[(&str, Value)]) -> Arc<Self> {
        let said = Self::default();
        if let Ok(mut slot) = said.answers.lock() {
            for (name, value) in answers {
                slot.insert((*name).to_string(), value.clone());
            }
        }
        Arc::new(said)
    }

    /// Everything printed into the terminal.
    pub fn echo(&self) -> String {
        String::from_utf8_lossy(&self.echoed.lock().expect("it is not held")).to_string()
    }

    /// Every notice, in the order they were said.
    pub fn notices(&self) -> Vec<String> {
        self.notices.lock().expect("it is not held").clone()
    }

    /// Every complaint, in the order they were said.
    pub fn errors(&self) -> Vec<String> {
        self.errors.lock().expect("it is not held").clone()
    }
}

impl Prompt for Said {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        let mut answers = form.defaults();
        if let Ok(given) = self.answers.lock() {
            for field in &form.fields {
                if let Some(value) = given.get(&field.name) {
                    answers.insert(field.name.clone(), value.clone());
                }
            }
        }
        Some(answers)
    }

    fn notice(&self, kind: NoticeKind, text: String) {
        let into = match kind {
            NoticeKind::Info => &self.notices,
            NoticeKind::Error => &self.errors,
        };
        into.lock().expect("it is not held").push(text);
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

/// The directory the shipped scripts stand in.
pub fn shipped() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts")
}

/// The directory the scripts written for these tests stand in.
pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fish")
        .join("scripts")
}

/// A run of one script, with everything it is given.
pub struct Run {
    pub name: String,
    pub direction: Direction,
    pub target: Targets,
    pub variables: BTreeMap<String, String>,
    pub roots: Vec<PathBuf>,
}

impl Run {
    /// A run of a shipped script.
    pub fn shipped(name: &str) -> Self {
        Self {
            name: name.to_string(),
            direction: Direction::Send,
            target: Targets::none(),
            variables: BTreeMap::new(),
            roots: vec![shipped()],
        }
    }

    /// A run of a script written for these tests, with the shipped library
    /// behind it.
    pub fn fixture(name: &str) -> Self {
        Self {
            name: name.to_string(),
            direction: Direction::Send,
            target: Targets::none(),
            variables: BTreeMap::new(),
            roots: vec![fixtures(), shipped()],
        }
    }

    /// The same run, going the other way.
    pub fn receiving(mut self) -> Self {
        self.direction = Direction::Receive;
        self
    }

    /// The same run, carrying those paths.
    pub fn carrying(mut self, kind: TargetKind, paths: &[PathBuf]) -> Self {
        self.target = Targets {
            kind,
            paths: paths.to_vec(),
        };
        self
    }

    /// The same run, told that value.
    pub fn told(mut self, name: &str, value: &str) -> Self {
        self.variables.insert(name.to_string(), value.to_string());
        self
    }

    /// Starts it over that line, with that prompt.
    pub fn start(
        &self,
        line: Arc<dyn zyt_script::Line>,
        said: Arc<Said>,
        cancel: Arc<Cancel>,
    ) -> zyt_script::Result<ScriptRun> {
        let library = zyt_script::Library::load(&self.roots);
        let entry = library.find(&self.name)?;
        let start = Start::of_entry(
            entry,
            &self.roots,
            self.direction,
            self.target.clone(),
            self.variables.clone(),
            zyt_script::Talking {
                line,
                prompt: said,
                cancel,
            },
        );
        ScriptRun::start(start, self.direction)
    }
}

/// Runs a script against replies written down beforehand and answers with
/// everything it wrote and said.
pub fn against(
    replies: &str,
    run: &Run,
    answers: &[(&str, Value)],
) -> (String, Arc<Said>, Outcome) {
    let line = LineChannel::new();
    line.feed(replies.as_bytes());
    let said = Said::answering(answers);

    let mut running = run
        .start(line.clone(), said.clone(), Cancel::new())
        .expect("the script loads");
    let outcome = finish(&mut running, Duration::from_secs(20));

    let mut written = Vec::new();
    line.take_output(&mut written);
    (String::from_utf8_lossy(&written).to_string(), said, outcome)
}

/// Walks a run to its end, or says it never ended.
pub fn finish(run: &mut ScriptRun, how_long: Duration) -> Outcome {
    let deadline = Instant::now() + how_long;
    while !run.tick() {
        if Instant::now() >= deadline {
            run.cancel();
            panic!("the script never ended");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    run.outcome().expect("it says how it ended")
}

/// A line that is a real shell reading its commands from a pipe.
///
/// It is what an `ssh` session is like: no line discipline, nothing to switch
/// to binary, and a shell that reads ahead. Raw has to be refused over this.
pub fn piped_shell() -> Arc<dyn zyt_script::Line> {
    piped_shell_in(None)
}

/// The same pipe, with the shell standing in that directory.
///
/// Where the shell stands is what the scripts work below: none of them asks
/// for a path of the device, so a case that is about a directory puts its
/// shell in it.
pub fn piped_shell_in(cwd: Option<&Path>) -> Arc<dyn zyt_script::Line> {
    let mut command = std::process::Command::new("sh");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("a shell runs");

    let reader = child.stdout.take().expect("it has a pipe");
    let writer = child.stdin.take().expect("it has a pipe");
    std::mem::forget(child);
    zyt_script::cli::line::Piped::new(reader, writer).expect("the line is made")
}

/// A line that is a real shell under a pseudo terminal.
///
/// It is what a console is like: an echo of everything written to it, and a
/// line that `stty` can switch to binary and back.
pub fn console_shell() -> Arc<dyn zyt_script::Line> {
    console_shell_in(None)
}

/// The same console, with the shell standing in that directory.
pub fn console_shell_in(cwd: Option<&Path>) -> Arc<dyn zyt_script::Line> {
    let system = portable_pty::native_pty_system();
    let pair = system
        .openpty(portable_pty::PtySize::default())
        .expect("a pseudo terminal is made");

    let mut builder = portable_pty::CommandBuilder::new("sh");
    builder.env("PS1", "# ");
    if let Some(cwd) = cwd {
        builder.cwd(cwd);
    }
    let child = pair.slave.spawn_command(builder).expect("a shell runs");
    std::mem::forget(child);
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().expect("it reads");
    let writer = pair.master.take_writer().expect("it writes");
    std::mem::forget(pair.master);
    zyt_script::cli::line::Piped::new(reader, writer).expect("the line is made")
}

/// A directory of its own for one case, empty to begin with.
pub fn workspace(case: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("zyterm-fish-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the directory is made");
    root
}

/// Runs a script over that line and answers with what it said.
pub fn over(
    line: Arc<dyn zyt_script::Line>,
    run: &Run,
    answers: &[(&str, Value)],
    how_long: Duration,
) -> (Arc<Said>, Outcome, String) {
    let said = Said::answering(answers);
    let mut running = run
        .start(line, said.clone(), Cancel::new())
        .expect("the script loads");
    let outcome = finish(&mut running, how_long);
    let failure = failure(&running);
    (said, outcome, failure)
}

/// Why the run failed, as the script said it.
pub fn failure(run: &ScriptRun) -> String {
    run.take_failure()
        .map(|error| error.to_string())
        .unwrap_or_default()
}
