//! Values somebody typed into the window that asks for them, one file per
//! source.
//!
//! A console runs a command line and starts in a directory, and both of them
//! may ask for a value by name. Some of those values belong to the console and
//! are written in its settings once — a port, a user, a path that is the same
//! every day. Others are the answer to "which one this time": a host, a board,
//! a branch. Those are asked for when the connection is made and kept here, so
//! the window that asks comes up with what was answered last time already in
//! it.
//!
//! They live apart from the settings of the source on purpose. A value in the
//! settings is a decision about the console; a value here is what somebody
//! typed on the way in, and the two must not be mistaken for one another —
//! writing these into the settings would turn one evening's host into what the
//! console is.
//!
//! One file per source, below the configuration directory and named after the
//! key of the source, the way the history of its commands is. Nothing here is
//! ever reported to the user: a file that cannot be read is a line in the log
//! and an empty answer, because a value nobody can read is a field to fill in
//! again and not a reason to refuse a connection.

use crate::error::{AppError, Result};
use crate::sources::SourceKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zyt_config::ConfigStore;

/// Directory holding the answers, below the configuration directory.
pub const ANSWERS_DIR: &str = "answers";

/// What one source was answered, as it lives on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Answered {
    /// Key of the source: an identity of a console, `usb:…` or `path:…`.
    key: SourceKey,
    /// What was typed, by the name it answers.
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// What this source was answered last time, empty when it was never asked.
pub fn load(store: &ConfigStore, key: &SourceKey) -> BTreeMap<String, String> {
    match store.load::<Answered>(&entry(&file_name(key))) {
        Ok(Some(answered)) => answered.values,
        Ok(None) => BTreeMap::new(),
        Err(error) => {
            log::warn!("cannot read the answers of {key}: {error}");
            BTreeMap::new()
        }
    }
}

/// Writes what this source was answered.
pub fn save(store: &ConfigStore, key: &SourceKey, values: &BTreeMap<String, String>) -> Result<()> {
    let answered = Answered {
        key: key.clone(),
        values: values.clone(),
    };
    store
        .save(&entry(&file_name(key)), &answered)
        .map_err(AppError::from)
}

/// Puts these answers into the memory of a source, where a command line and a
/// directory read them.
///
/// Only a name the source has no value for is answered from here: what stands
/// in the settings of the source is what somebody decided about it, and an
/// answer typed on the way in does not overrule a decision. A value that is
/// there and empty is not a value, so it is answered like a name that is
/// missing altogether.
///
/// The memory this is given is the one the connection is made with and never
/// the one on disk: these values are kept in their own file, and a source whose
/// settings quietly filled up with them would be a source that stopped asking.
pub fn fill(memory: &mut crate::sources::SourceMemory, values: &BTreeMap<String, String>) {
    for (name, value) in values {
        if value.is_empty() {
            continue;
        }
        match memory
            .variables
            .iter_mut()
            .find(|variable| &variable.name == name)
        {
            Some(variable) if variable.value.is_empty() => variable.value = value.clone(),
            Some(_) => {}
            None => memory.variables.push(crate::sources::SourceVariable {
                name: name.clone(),
                value: value.clone(),
            }),
        }
    }
}

/// File the answers of one source are written to, named after its key.
fn file_name(key: &SourceKey) -> String {
    format!("{}.yaml", key.slug())
}

fn entry(file: &str) -> String {
    format!("{ANSWERS_DIR}/{file}")
}

#[cfg(test)]
mod tests {

    /// Two consoles told apart, the way a file name tells them apart.
    fn source(last: u8) -> SourceKey {
        SourceKey::Console(
            format!("00000000-0000-0000-0000-0000000000{last:02}")
                .parse()
                .expect("the identity reads"),
        )
    }

    use super::*;

    fn store(case: &str) -> ConfigStore {
        let root =
            std::env::temp_dir().join(format!("zyterm-answers-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the directory is made");
        ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("locks"))
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// What was answered comes back for that source and for no other: the
    /// window asks about the connection in front of the user, so the answers
    /// are the answers of that one.
    #[test]
    fn the_answers_come_back_for_the_source_they_were_given_for() {
        let store = store("round-trip");

        assert!(load(&store, &source(1)).is_empty());

        save(&store, &source(1), &values(&[("host", "board-7")])).expect("the answers are written");

        assert_eq!(load(&store, &source(1)), values(&[("host", "board-7")]));
        assert!(load(&store, &source(2)).is_empty());
    }

    /// A console is read by its name with the values it was opened with put in.
    /// A name like `SSH {remote_host}` is exactly what the window asks about,
    /// and a message about the console that is gone has to name the one that
    /// was running rather than the hole it was started from.
    #[test]
    fn a_console_is_named_by_what_it_was_opened_with() {
        let mut console = crate::consoles::default_console();
        console.name = "SSH {remote_host}".to_string();

        assert_eq!(
            console.display_name(),
            "SSH {remote_host}",
            "a name nothing answers stands as it was written"
        );

        fill(&mut console.memory, &values(&[("remote_host", "board-7")]));

        assert_eq!(console.display_name(), "SSH board-7");
    }

    /// An answer fills a name the source has no value for and leaves alone the
    /// one it has: what stands in the settings was decided about the console,
    /// and what is typed on the way in is about this evening.
    #[test]
    fn an_answer_fills_what_is_empty_and_overrules_nothing() {
        let mut memory = crate::sources::SourceMemory {
            variables: vec![
                crate::sources::SourceVariable {
                    name: "user".to_string(),
                    value: "root".to_string(),
                },
                crate::sources::SourceVariable {
                    name: "host".to_string(),
                    value: String::new(),
                },
            ],
            ..crate::sources::SourceMemory::default()
        };

        fill(
            &mut memory,
            &values(&[("user", "guest"), ("host", "board-7"), ("port", "2222")]),
        );

        let answered = memory.variable_map();
        assert_eq!(answered.get("user").map(String::as_str), Some("root"));
        assert_eq!(answered.get("host").map(String::as_str), Some("board-7"));
        assert_eq!(answered.get("port").map(String::as_str), Some("2222"));
    }
}
