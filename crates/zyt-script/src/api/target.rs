//! `zyt.target` and `zyt.vars`: what the script carries and what it was told.

use super::{Context, external};
use crate::error::ScriptError;
use mlua::{Lua, Table};
use std::path::Path;
use std::sync::Arc;

/// Builds `zyt.target`.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let target = lua.create_table()?;

    target.set("kind", context.target.kind.name())?;

    target.set(
        "paths",
        lua.create_function({
            let context = context.clone();
            move |lua, ()| {
                let paths: Vec<String> = context
                    .target
                    .paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect();
                lua.create_sequence_from(paths)
            }
        })?,
    )?;

    target.set(
        "path",
        lua.create_function({
            let context = context.clone();
            move |_, ()| {
                Ok(context
                    .target
                    .path()
                    .map(|path| path.to_string_lossy().to_string()))
            }
        })?,
    )?;

    for (call, part) in [
        ("filename", Part::Filename),
        ("stem", Part::Stem),
        ("suffix", Part::Suffix),
    ] {
        target.set(
            call,
            lua.create_function({
                let context = context.clone();
                move |_, ()| Ok(context.target.path().map(|path| name_part(path, part)))
            })?,
        )?;
    }

    Ok(target)
}

/// Builds `zyt.vars`.
pub(crate) fn variables(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let vars = lua.create_table()?;

    vars.set(
        "get",
        lua.create_function({
            let context = context.clone();
            move |_, name: String| Ok(context.variables.get(&name).cloned())
        })?,
    )?;

    vars.set(
        "require",
        lua.create_function({
            let context = context.clone();
            move |_, name: String| match context.variables.get(&name) {
                Some(value) if !value.is_empty() => Ok(value.clone()),
                _ => Err(external(ScriptError::UnsetVariable { name })),
            }
        })?,
    )?;

    vars.set(
        "all",
        lua.create_function({
            let context = context.clone();
            move |lua, ()| {
                let all = lua.create_table()?;
                for (name, value) in &context.variables {
                    all.set(name.as_str(), value.as_str())?;
                }
                Ok(all)
            }
        })?,
    )?;

    Ok(vars)
}

/// Which part of a name is wanted.
#[derive(Debug, Clone, Copy)]
enum Part {
    /// The name with its extension.
    Filename,
    /// The name without it.
    Stem,
    /// The extension alone, without the dot.
    Suffix,
}

/// That part of the name of a path, or nothing of it when it has none.
fn name_part(path: &Path, part: Part) -> String {
    let piece = match part {
        Part::Filename => path.file_name(),
        Part::Stem => path.file_stem(),
        Part::Suffix => path.extension(),
    };
    piece
        .map(|piece| piece.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_of_a_name_are_told_apart() {
        let path = Path::new("/tmp/board.itb");
        assert_eq!(name_part(path, Part::Filename), "board.itb");
        assert_eq!(name_part(path, Part::Stem), "board");
        assert_eq!(name_part(path, Part::Suffix), "itb");
    }

    #[test]
    fn a_name_without_an_extension_has_an_empty_one() {
        let path = Path::new("/tmp/image");
        assert_eq!(name_part(path, Part::Filename), "image");
        assert_eq!(name_part(path, Part::Stem), "image");
        assert_eq!(name_part(path, Part::Suffix), "");
    }
}
