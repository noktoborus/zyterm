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
use crate::host::{NoticeKind, Progress, Prompt};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Mutex;

/// The console, standing in for a window.
#[derive(Debug, Default)]
pub struct Console {
    answers: BTreeMap<String, String>,
    quiet: bool,
    asked: Mutex<Vec<Form>>,
}

impl Console {
    /// A console answering with these values, by the name of the field.
    pub fn new(answers: BTreeMap<String, String>, quiet: bool) -> Self {
        Self {
            answers,
            quiet,
            asked: Mutex::new(Vec::new()),
        }
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
