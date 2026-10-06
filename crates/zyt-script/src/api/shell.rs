//! `zyt.shell`: turning values into words a shell reads.

use super::external;
use mlua::{Lua, Table};

/// Builds the table.
pub(crate) fn table(lua: &Lua) -> mlua::Result<Table> {
    let shell = lua.create_table()?;

    shell.set(
        "quote",
        lua.create_function(|_, value: String| Ok(crate::quote::quote_for_shell(&value)))?,
    )?;

    shell.set(
        "quote_posix",
        lua.create_function(|_, value: String| Ok(crate::quote::quote_posix(&value)))?,
    )?;

    shell.set(
        "split",
        lua.create_function(|lua, line: String| {
            let words = shlex::split(&line).unwrap_or_default();
            lua.create_sequence_from(words)
        })?,
    )?;

    shell.set(
        "join",
        lua.create_function(|_, words: Vec<String>| {
            Ok(words
                .iter()
                .map(|word| crate::quote::quote_for_shell(word))
                .collect::<Vec<String>>()
                .join(" "))
        })?,
    )?;

    shell.set(
        "finish_bytes",
        lua.create_function(|lua, text: String| {
            let bytes = crate::finish::finish_bytes(&text).map_err(external)?;
            lua.create_string(&bytes)
        })?,
    )?;

    Ok(shell)
}
