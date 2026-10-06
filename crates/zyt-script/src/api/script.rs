//! `zyt.script`: what the script is, and the files it is made of.
//!
//! A script of any size is more than one file: the shell it sends to a device
//! is worth reading as shell, so it lives in `.sh` files of its own and is
//! read from here. [`fold`] is what turns such a file into the one line a wire
//! takes, and [`fill`] is what puts the values in its holes — both of them
//! here rather than in Lua, so every script folds a template the same way.

use super::{Context, external};
use crate::error::ScriptError;
use mlua::{Lua, Table};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let script = lua.create_table()?;

    script.set("name", context.name.clone())?;
    script.set("direction", context.direction.name())?;
    script.set("root", context.root.to_string_lossy().to_string())?;

    script.set(
        "read_file",
        lua.create_function({
            let context = context.clone();
            move |lua, relative: String| {
                let path = found(&context, &relative).map_err(external)?;
                let bytes = std::fs::read(&path).map_err(|source| {
                    external(ScriptError::Read {
                        path: path.clone(),
                        source,
                    })
                })?;
                lua.create_string(&bytes)
            }
        })?,
    )?;

    script.set(
        "list_files",
        lua.create_function({
            let context = context.clone();
            move |lua, relative: String| {
                let path = found(&context, &relative).map_err(external)?;
                let mut names: Vec<String> = std::fs::read_dir(&path)
                    .map_err(|source| {
                        external(ScriptError::Read {
                            path: path.clone(),
                            source,
                        })
                    })?
                    .flatten()
                    .filter(|entry| entry.path().is_file())
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect();
                names.sort_unstable();
                lua.create_sequence_from(names)
            }
        })?,
    )?;

    script.set(
        "fold",
        lua.create_function(|_, text: String| Ok(fold(&text)))?,
    )?;

    script.set(
        "fill",
        lua.create_function(|_, (text, holes): (String, Table)| {
            let mut values: Vec<(String, String)> = Vec::new();
            for pair in holes.pairs::<String, mlua::Value>() {
                let (name, value) = pair?;
                values.push((name, text_of(&value)));
            }
            Ok(fill(&text, &values))
        })?,
    )?;

    Ok(script)
}

/// The text a value of the interpreter stands for in a hole.
fn text_of(value: &mlua::Value) -> String {
    match value {
        mlua::Value::String(text) => text.to_string_lossy(),
        mlua::Value::Integer(number) => number.to_string(),
        mlua::Value::Number(number) => number.to_string(),
        mlua::Value::Boolean(flag) => flag.to_string(),
        other => other.to_string().unwrap_or_default(),
    }
}

/// The file of that name, in the first root that carries it.
///
/// The directory the script itself came from is asked first, so a script that
/// was put in the configuration directory with templates of its own uses
/// those; after it the other roots in the order they are searched, which is
/// how a script may ship one template and take the rest from beside the
/// program.
fn found(context: &Arc<Context>, relative: &str) -> crate::Result<PathBuf> {
    let relative = safe(relative)?;

    let here = context.root.join(&relative);
    if here.exists() {
        return Ok(here);
    }
    for root in &context.roots {
        let there = root.join(&relative);
        if there.exists() {
            return Ok(there);
        }
    }
    Err(ScriptError::Read {
        path: context.root.join(&relative),
        source: std::io::Error::from(std::io::ErrorKind::NotFound),
    })
}

/// The path as it may be used, or an error when it reaches outside a root.
///
/// A relative path is the whole of what a script may name here: one that is
/// absolute or walks up with `..` would read a file of the machine through a
/// call that is about the files of the script.
fn safe(relative: &str) -> crate::Result<PathBuf> {
    let path = Path::new(relative);
    let outside = path.components().any(|part| {
        matches!(
            part,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if outside {
        return Err(ScriptError::Read {
            path: path.to_path_buf(),
            source: std::io::Error::from(std::io::ErrorKind::InvalidInput),
        });
    }
    Ok(path.to_path_buf())
}

/// Everything of a template but the comments and the blank lines, joined by a
/// space.
///
/// The shell of a device must not be left waiting for the rest of a construct,
/// so a command crosses the wire as one line however it was written down.
pub fn fold(template: &str) -> String {
    template
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Puts a value in every hole, in one pass.
///
/// What is put in is not looked at again, so a path that carries `{size}` in
/// its own name fills nothing. A hole nothing answers to is left as it stands,
/// which shows up in a test rather than in a quietly wrong command.
pub fn fill(template: &str, values: &[(String, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('}') else {
            break;
        };
        match value_of(&rest[1..close], values) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[..=close]),
        }
        rest = &rest[close + 1..];
    }

    out.push_str(rest);
    out
}

/// What answers to the name of a hole.
fn value_of<'a>(name: &str, values: &'a [(String, String)]) -> Option<&'a str> {
    values
        .iter()
        .find(|(hole, _)| hole == name)
        .map(|(_, value)| value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holes(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn a_template_is_folded_into_one_line() {
        let folded = fold("# why\n\n  \\echo one;\n  \\echo two\n");
        assert_eq!(folded, "\\echo one; \\echo two");
    }

    #[test]
    fn a_marker_is_written_by_the_template_and_not_filled_in() {
        assert_eq!(fill("\\echo '##''# 200'", &[]), "\\echo '##''# 200'");
    }

    #[test]
    fn what_was_put_in_is_not_looked_at_again() {
        let filled = fill("a {path} b", &holes(&[("path", "'{size}'"), ("size", "9")]));
        assert_eq!(filled, "a '{size}' b");
    }

    #[test]
    fn a_hole_nothing_answers_to_stays_as_it_is() {
        assert_eq!(fill("a {nobody} b", &[]), "a {nobody} b");
        assert_eq!(fill("a { b", &[]), "a { b");
    }

    #[test]
    fn every_hole_is_filled_not_only_the_first() {
        let filled = fill(
            "{path} {path} {size}",
            &holes(&[("path", "p"), ("size", "9")]),
        );
        assert_eq!(filled, "p p 9");
    }

    #[test]
    fn a_file_outside_the_script_cannot_be_named() {
        assert!(safe("lib/fish/sh/pwd.sh").is_ok());
        assert!(safe("../../etc/passwd").is_err());
        assert!(safe("/etc/passwd").is_err());
    }
}
