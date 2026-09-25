//! Every command and error key must exist in both languages.

use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Table(BTreeMap<String, serde_yaml_ng::Value>);

fn table() -> BTreeMap<String, BTreeMap<String, String>> {
    let text = std::fs::read_to_string("locales/app.yml").expect("locale file exists");
    let raw: Table = serde_yaml_ng::from_str(&text).expect("locale file parses");
    raw.0
        .into_iter()
        .filter(|(key, _)| key != "_version")
        .map(|(key, value)| {
            let entry: BTreeMap<String, String> =
                serde_yaml_ng::from_value(value).expect("entry holds one string per locale");
            (key, entry)
        })
        .collect()
}

#[test]
fn every_key_has_english_and_russian() {
    for (key, entry) in table() {
        assert!(entry.contains_key("en"), "missing en for {key}");
        assert!(entry.contains_key("ru"), "missing ru for {key}");
        assert!(!entry["en"].trim().is_empty(), "empty en for {key}");
        assert!(!entry["ru"].trim().is_empty(), "empty ru for {key}");
    }
}

#[test]
fn every_key_used_in_the_code_exists() {
    let table = table();
    let mut missing = Vec::new();

    for entry in walk("src") {
        let text = std::fs::read_to_string(&entry).expect("source is readable");
        for key in keys_of(&text) {
            if !table.contains_key(&key) {
                missing.push(format!("{}: {key}", entry.display()));
            }
        }
    }

    assert!(missing.is_empty(), "missing translations: {missing:?}");
}

/// Nothing in the file is a key nothing names.
///
/// A key the code stopped saying stays in both languages for as long as the
/// file is kept by hand: it is translated, reviewed and read again by whoever
/// comes next, and it says nothing to anybody. The names of the commands are
/// the one thing built rather than written — `command.` and the identifier of
/// the command — and [`locale_file_covers_the_command_titles`] is what watches
/// those.
#[test]
fn every_key_of_the_file_is_named_somewhere() {
    let written: String = walk("src")
        .into_iter()
        .map(|entry| std::fs::read_to_string(&entry).expect("source is readable"))
        .collect();

    let spare: Vec<String> = table()
        .into_keys()
        .filter(|key| !key.starts_with("command."))
        .filter(|key| !written.contains(key.as_str()))
        .collect();

    assert!(spare.is_empty(), "nothing says these: {spare:?}");
}

/// Every source file below one directory.
fn walk(root: &str) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path.to_string_lossy()));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}

/// Keys of every `t!("…")` with a literal key.
fn keys_of(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("t!(\"") {
        rest = &rest[start + 4..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let key = &rest[..end];
        rest = &rest[end..];
        if key.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || ".\u{5f}".contains(character)
        }) && key.contains('.')
        {
            keys.push(key.to_string());
        }
    }

    keys
}

#[test]
fn locale_file_covers_the_command_titles() {
    let table = table();
    let commands = [
        "palette.open",
        "settings.open",
        "view.toggle_status_bar",
        "port.reopen",
        "port.open_previous",
        "app.quit",
        "terminal.copy",
        "terminal.paste",
        "terminal.clear",
        "transfer.send_file",
        "transfer.receive_file",
        "transfer.cancel",
    ];
    for command in commands {
        assert!(
            table.contains_key(&format!("command.{command}")),
            "missing title for {command}"
        );
    }
}
