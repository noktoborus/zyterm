//! What a script is talking to when nobody is there.
//!
//! The answers are given before the run starts, which is what makes a dialog
//! testable: the same description a window would draw is filled from the
//! command line, and a required field nothing answers stops the run rather
//! than waiting for somebody who is not coming.
//!
//! What the script prints goes to the diagnostic channel and not to the
//! standard output, because the standard output may be the line.

use crate::form::{Form, Value};
use crate::host::{NoticeKind, Pick, Progress, Prompt, Stage};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The console, standing in for a window.
#[derive(Debug, Default)]
pub struct Console {
    answers: BTreeMap<String, String>,
    picks: Vec<PathBuf>,
    quiet: bool,
    asked: Mutex<Vec<Form>>,
}

impl Console {
    /// A console answering with these values, by the name of the field.
    pub fn new(answers: BTreeMap<String, String>, quiet: bool) -> Self {
        Self {
            answers,
            picks: Vec::new(),
            quiet,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The same, answering the dialog of the file system with these paths.
    pub fn picking(mut self, picks: Vec<PathBuf>) -> Self {
        self.picks = picks;
        self
    }

    /// Every form the script asked, in the order it asked them.
    pub fn asked(&self) -> Vec<Form> {
        self.asked
            .lock()
            .map(|asked| asked.clone())
            .unwrap_or_default()
    }

    /// Says one line on the diagnostic channel.
    fn say(&self, line: &str) {
        if self.quiet {
            return;
        }
        let mut err = std::io::stderr();
        let _ = writeln!(err, "{line}");
        let _ = err.flush();
    }
}

impl Prompt for Console {
    fn ask(&self, form: Form) -> Option<BTreeMap<String, Value>> {
        let mut answers = form.defaults();

        for field in &form.fields {
            let Some(given) = self.answers.get(&field.name) else {
                continue;
            };
            answers.insert(field.name.clone(), answered(field, given));
        }

        self.say(&format!(
            "asked: {} [{}]",
            form.title,
            form.fields
                .iter()
                .filter(|field| !field.kind.is_decoration())
                .map(|field| field.name.as_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ));
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(form.clone());
        }

        if form.check(&answers).is_err() {
            self.say("a field of the dialog was not answered");
            return None;
        }
        Some(answers)
    }

    fn pick(&self, pick: Pick) -> Option<Vec<PathBuf>> {
        self.say(&format!("picks {}: {}", pick.kind.name(), pick.title));
        if self.picks.is_empty() {
            self.say("nothing was given to pick");
            return None;
        }
        let picked = match pick.kind.is_multiple() {
            true => self.picks.clone(),
            false => self.picks[..1].to_vec(),
        };
        Some(picked)
    }

    fn notice(&self, kind: NoticeKind, text: String) {
        let mark = match kind {
            NoticeKind::Info => "note",
            NoticeKind::Error => "fail",
        };
        self.say(&format!("{mark}: {text}"));
    }

    fn echo(&self, bytes: &[u8]) {
        let mut err = std::io::stderr();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    }

    fn progress(&self, progress: Progress) {
        if let Progress::Share(share) = progress {
            self.say(&format!("progress: {share}%"));
        }
    }

    fn ending(&self) {
        self.say("ending");
    }

    fn stage(&self, stage: Stage) {
        let mut line = format!("stage: {:?}", stage.kind).to_lowercase();
        for part in [stage.name, stage.text].into_iter().flatten() {
            line.push(' ');
            line.push_str(&part);
        }
        if let (Some(file), Some(files)) = (stage.file, stage.files) {
            line.push_str(&format!(" ({file}/{files})"));
        }
        if let Some(count) = stage.count {
            line.push_str(&format!(" #{count}"));
        }
        self.say(&line);
    }
}

/// The answer a word stands for, in the shape that field asks for.
fn answered(field: &crate::form::Field, given: &str) -> Value {
    match &field.kind {
        crate::form::FieldKind::Switch { .. } => {
            Value::Flag(matches!(given, "true" | "yes" | "on" | "1"))
        }
        crate::form::FieldKind::ManyOf { .. } => Value::Many(
            given
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        crate::form::FieldKind::OneOf { .. } | crate::form::FieldKind::Select { .. } => {
            Value::One(given.to_string())
        }
        _ => Value::Text(given.to_string()),
    }
}
