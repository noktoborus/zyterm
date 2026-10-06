//! What a script was answered, one file per source, script and form.
//!
//! A script asks what it needs and the answer is kept, so sending a second
//! file to the same board is one press and not the same five fields again. It
//! is the same arrangement the window that asks for the values of a console
//! has, and it is kept apart from the settings of the source for the same
//! reason: a value answered on the way in is not a decision about the device.
//!
//! One file per triple, named `<source>-<script>-<form>`, because the answer
//! belongs to all three: the chunk size of a slow board is not the chunk size
//! of a fast one, the directory on one device is not the directory on another,
//! and a script that asks two things keeps two answers that say nothing about
//! each other.
//!
//! The switch that says to ask every time is kept here as well. It is a thing
//! about this source, this script and this form — a board whose directory
//! never changes is answered once, and the one being set up is asked every
//! time — so it lives beside the answers and not in the settings. Asking is
//! what it says when the file says nothing: a form nobody turned the switch
//! off for comes up.

use crate::error::{AppError, Result};
use crate::sources::SourceKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zyt_config::ConfigStore;
use zyt_script::Value;

/// Directory holding the answers, below the configuration directory.
pub const FORMS_DIR: &str = "forms";

/// Name a form of no name of its own is kept under.
pub const DEFAULT_FORM: &str = "default";

/// What one form of one script was answered for one source.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Answered {
    /// Key of the source: an identity of a console, `usb:…` or `path:…`.
    #[serde(default)]
    pub key: String,
    /// Name of the directory the script stands in.
    #[serde(default)]
    pub script: String,
    /// Name the form is known by inside that script.
    #[serde(default)]
    pub form: String,
    /// Which way the switch that says to ask every time was left.
    ///
    /// Nothing is asking, which is what a form nobody answered yet is, and
    /// `false` is the one thing worth writing down: somebody said this source,
    /// this script and this form are not to be asked again. The button in the
    /// settings takes that word back by taking the key out of every file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_ask: Option<bool>,
    /// What was answered, by the name of the field.
    ///
    /// A value comes back as the shape it was written in, and text is text
    /// whether it was typed or picked from a list: everything that reads one
    /// reads it as text, so the two are not worth telling apart on disk.
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

impl Answered {
    /// True while this source, this script and this form are to be asked every
    /// time.
    pub fn always_ask(&self) -> bool {
        self.always_ask.unwrap_or(true)
    }
}

/// What this source answered this form of this script last time, or nothing.
pub fn load(store: &ConfigStore, key: &SourceKey, script: &str, form: &str) -> Option<Answered> {
    match store.load::<Answered>(&entry(key, script, form)) {
        Ok(found) => found,
        Err(error) => {
            log::warn!("cannot read what {script} {form} was answered for {key}: {error}");
            None
        }
    }
}

/// Writes what this source answered this form of this script.
pub fn save(
    store: &ConfigStore,
    key: &SourceKey,
    script: &str,
    form: &str,
    values: &BTreeMap<String, Value>,
    always_ask: bool,
) -> Result<()> {
    let answered = Answered {
        key: key.to_string(),
        script: script.to_string(),
        form: form.to_string(),
        always_ask: (!always_ask).then_some(false),
        values: values.clone(),
    };
    store
        .save(&entry(key, script, form), &answered)
        .map_err(AppError::from)
}

/// Puts every form that was told not to ask back to asking, and says how many
/// were changed.
///
/// A form is told not to ask once and keeps that word for as long as the file
/// stands, which is a thing nobody can see from the window: the windows that
/// stopped coming up are the ones with nothing to press. This is how they are
/// all brought back, and a file that never carried the switch is left alone —
/// it already asks.
pub fn reset(store: &ConfigStore) -> Result<usize> {
    let directory = store.config_dir().join(FORMS_DIR);
    let listed = match std::fs::read_dir(&directory) {
        Ok(listed) => listed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(source) => return Err(AppError::Forms { source }),
    };

    let mut reset = 0;
    for path in listed.flatten().map(|entry| entry.path()) {
        let Some(name) = yaml_name(&path) else {
            continue;
        };
        let file = format!("{FORMS_DIR}/{name}");
        let Some(mut answered) = store.load::<Answered>(&file).ok().flatten() else {
            log::warn!("{} is not a form that was answered", path.display());
            continue;
        };
        if answered.always_ask.is_none() {
            continue;
        }
        answered.always_ask = None;
        store.save(&file, &answered).map_err(AppError::from)?;
        reset += 1;
    }
    Ok(reset)
}

/// Name of a file of answers, when that is what the path is.
fn yaml_name(path: &std::path::Path) -> Option<String> {
    if !path.is_file()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("yaml"))
    {
        return None;
    }
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// What a script is answered without asking, when it may be answered that
/// way.
///
/// Nothing is reused while something says to ask — the switch of this form, a
/// form that is not kept at all — and nothing is reused that does not answer
/// the form in front of it: a script that grew a field since the last time is a
/// script with a question nobody has answered yet, and the window has to come
/// up for it.
pub fn reuse(saved: Option<&Answered>, form: &zyt_script::Form) -> Option<BTreeMap<String, Value>> {
    if form.unsaved {
        return None;
    }
    let saved = saved?;
    if saved.always_ask() {
        return None;
    }

    let mut values = form.defaults();
    for (name, value) in &saved.values {
        if values.contains_key(name) {
            values.insert(name.clone(), value.clone());
        }
    }
    form.check(&values).ok().map(|()| values)
}

/// The file the answers of one source, one script and one form are written to.
fn entry(key: &SourceKey, script: &str, form: &str) -> String {
    format!(
        "{FORMS_DIR}/{}-{}-{}.yaml",
        key.slug(),
        part(script, "script"),
        part(form, DEFAULT_FORM)
    )
}

/// What one name may be inside a file name: itself, with everything a path
/// cannot carry replaced, and never empty.
fn part(text: &str, fallback: &str) -> String {
    let kept: String = text
        .chars()
        .map(
            |character| match character.is_alphanumeric() || character == '_' {
                true => character,
                false => '-',
            },
        )
        .collect();
    let kept = kept.trim_matches('-').to_string();
    match kept.is_empty() {
        true => fallback.to_string(),
        false => kept,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store pointing at directories of this test.
    fn store(case: &str) -> (ConfigStore, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("zyterm-forms-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the directory is made");
        (
            ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("lock")),
            root,
        )
    }

    /// The key of a device, as a source carries one.
    fn device() -> SourceKey {
        "path:/dev/ttyS0".parse().expect("the key is read")
    }

    #[test]
    fn what_was_answered_comes_back_as_it_was_written() {
        let (store, root) = store("round-trip");
        let values = BTreeMap::from([
            ("mode".to_string(), Value::Text("raw".to_string())),
            ("verify".to_string(), Value::Flag(true)),
            (
                "steps".to_string(),
                Value::Many(vec!["sync".to_string(), "reboot".to_string()]),
            ),
        ]);

        save(&store, &device(), "shell-transfer", "how", &values, true).expect("it is written");
        let back = load(&store, &device(), "shell-transfer", "how").expect("it is read");

        assert!(back.always_ask());
        assert_eq!(back.script, "shell-transfer");
        assert_eq!(back.form, "how");
        assert_eq!(back.values.get("verify"), Some(&Value::Flag(true)));
        assert_eq!(
            back.values.get("steps").map(Value::names),
            Some(vec!["sync".to_string(), "reboot".to_string()])
        );
        assert_eq!(
            back.values.get("mode").map(Value::text),
            Some("raw".to_string())
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn asking_every_time_is_what_a_file_saying_nothing_of_it_means() {
        let (store, root) = store("asking");
        let values = BTreeMap::from([("remote".to_string(), Value::Text("/tmp".to_string()))]);

        save(&store, &device(), "zmodem", "how", &values, true).expect("it is written");
        let written = std::fs::read_to_string(store.path(&entry(&device(), "zmodem", "how")))
            .expect("the file is there");
        assert!(
            !written.contains("always_ask"),
            "asking every time is written as nothing: {written}"
        );

        save(&store, &device(), "zmodem", "how", &values, false).expect("it is written");
        let written = std::fs::read_to_string(store.path(&entry(&device(), "zmodem", "how")))
            .expect("the file is there");
        assert!(
            written.contains("always_ask: false"),
            "not asking is the one word written down: {written}"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn one_file_belongs_to_one_source_one_script_and_one_form() {
        let (store, root) = store("apart");
        let one = BTreeMap::from([("remote".to_string(), Value::Text("/tmp/one".to_string()))]);
        let two = BTreeMap::from([("remote".to_string(), Value::Text("/tmp/two".to_string()))]);
        let three = BTreeMap::from([("remote".to_string(), Value::Text("/tmp/three".to_string()))]);

        save(&store, &device(), "zmodem", "how", &one, false).expect("written");
        save(&store, &device(), "ymodem", "how", &two, false).expect("written");
        save(&store, &device(), "zmodem", "which", &three, false).expect("written");

        for (script, form, wanted) in [
            ("zmodem", "how", "/tmp/one"),
            ("ymodem", "how", "/tmp/two"),
            ("zmodem", "which", "/tmp/three"),
        ] {
            assert_eq!(
                load(&store, &device(), script, form)
                    .expect("it is there")
                    .values
                    .get("remote")
                    .map(Value::text),
                Some(wanted.to_string()),
                "{script} {form}"
            );
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_form_that_was_never_asked_answers_with_nothing() {
        let (store, root) = store("never");
        assert_eq!(load(&store, &device(), "zmodem", "how"), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_reset_puts_every_form_that_was_told_not_to_ask_back_to_asking() {
        let (store, root) = store("reset");
        let values = BTreeMap::from([("remote".to_string(), Value::Text("/tmp".to_string()))]);

        save(&store, &device(), "zmodem", "how", &values, false).expect("written");
        save(&store, &device(), "ymodem", "how", &values, true).expect("written");

        assert_eq!(reset(&store).expect("the directory is read"), 1);
        assert!(
            load(&store, &device(), "zmodem", "how")
                .expect("it is there")
                .always_ask()
        );
        assert_eq!(
            reset(&store).expect("the directory is read"),
            0,
            "a file that already asks is left alone"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_directory_of_answers_that_was_never_made_resets_nothing() {
        let (store, root) = store("reset-nothing");
        assert_eq!(reset(&store).expect("nothing to read is no error"), 0);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A form asking for one value, which has to be answered.
    fn asked() -> zyt_script::Form {
        zyt_script::Form::new("Ask").and(
            zyt_script::Field::new(
                "remote",
                zyt_script::FieldKind::Text {
                    value: String::new(),
                    password: false,
                },
            )
            .and_required(),
        )
    }

    /// What one source answered that form.
    fn answered(always_ask: bool, value: &str) -> Answered {
        Answered {
            key: "path:/dev/ttyS0".to_string(),
            script: "zmodem".to_string(),
            form: "how".to_string(),
            always_ask: (!always_ask).then_some(false),
            values: BTreeMap::from([("remote".to_string(), Value::Text(value.to_string()))]),
        }
    }

    #[test]
    fn what_was_answered_before_answers_again_without_a_window() {
        let saved = answered(false, "/tmp/here");
        let values = reuse(Some(&saved), &asked()).expect("it is answered from the file");
        assert_eq!(
            values.get("remote").map(Value::text),
            Some("/tmp/here".to_string())
        );
    }

    #[test]
    fn nothing_is_reused_while_something_says_to_ask() {
        let insisting = answered(true, "/tmp/here");
        assert_eq!(
            reuse(Some(&insisting), &asked()),
            None,
            "this form says to ask"
        );
        assert_eq!(reuse(None, &asked()), None, "nothing was answered");

        let saved = answered(false, "/tmp/here");
        assert_eq!(
            reuse(Some(&saved), &asked().and_unsaved()),
            None,
            "a form that is not kept is a form that is asked"
        );
    }

    #[test]
    fn answers_that_do_not_answer_the_form_in_front_of_them_are_not_reused() {
        let stale = Answered {
            values: BTreeMap::from([("gone".to_string(), Value::Text("x".to_string()))]),
            ..answered(false, "")
        };
        assert_eq!(
            reuse(Some(&stale), &asked()),
            None,
            "a field the file says nothing about is a question to ask"
        );
    }

    #[test]
    fn a_name_the_form_no_longer_asks_is_left_behind() {
        let extra = Answered {
            values: BTreeMap::from([
                ("remote".to_string(), Value::Text("/tmp/here".to_string())),
                ("mode".to_string(), Value::Text("raw".to_string())),
            ]),
            ..answered(false, "")
        };

        let values = reuse(Some(&extra), &asked()).expect("it is answered");
        assert!(
            !values.contains_key("mode"),
            "the form of today is what is answered: {values:?}"
        );
    }

    #[test]
    fn a_name_a_file_cannot_carry_is_not_what_the_file_is_called() {
        let name = entry(&device(), "../escape", "a/b");
        assert_eq!(name, format!("{FORMS_DIR}/path__dev_ttyS0-escape-a-b.yaml"));
        assert_eq!(
            entry(&device(), "", ""),
            format!("{FORMS_DIR}/path__dev_ttyS0-script-{DEFAULT_FORM}.yaml")
        );
    }
}
