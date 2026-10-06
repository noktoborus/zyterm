//! `zyt.line`: the line of the session.

use super::{Context, external};
use crate::error::ScriptError;
use mlua::{Lua, Table};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long one wait inside a longer one lasts.
///
/// A read waits in slices rather than in one go, because the buffer it waits
/// on knows nothing of the ask to stop: between two slices the flag is read,
/// and that is what makes a script waiting on a silent device answer the
/// button at once.
const SLICE: Duration = Duration::from_millis(50);

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let line = lua.create_table()?;

    line.set(
        "write",
        lua.create_function({
            let context = context.clone();
            move |_, data: mlua::LuaString| {
                open(&context)?;
                context.line.write(&data.as_bytes());
                Ok(())
            }
        })?,
    )?;

    line.set(
        "write_text",
        lua.create_function({
            let context = context.clone();
            move |_, text: String| {
                open(&context)?;
                context.line.write(text.as_bytes());
                Ok(())
            }
        })?,
    )?;

    line.set(
        "type",
        lua.create_function({
            let context = context.clone();
            move |_, text: String| {
                open(&context)?;
                context.line.write(typed(&text).as_bytes());
                Ok(())
            }
        })?,
    )?;

    line.set(
        "write_key",
        lua.create_function({
            let context = context.clone();
            move |_, name: String| {
                open(&context)?;
                let bytes = crate::finish::finish_bytes(&name).map_err(external)?;
                context.line.write(&bytes);
                Ok(())
            }
        })?,
    )?;

    line.set(
        "read",
        lua.create_function({
            let context = context.clone();
            move |lua, timeout: Option<u64>| {
                let read = wait(&context, timeout.unwrap_or(0))?;
                if read.is_empty() {
                    return Ok(mlua::Value::Nil);
                }
                Ok(mlua::Value::String(lua.create_string(&read)?))
            }
        })?,
    )?;

    line.set(
        "pending",
        lua.create_function({
            let context = context.clone();
            move |_, ()| Ok(context.line.pending_output())
        })?,
    )?;

    line.set(
        "waiting",
        lua.create_function({
            let context = context.clone();
            move |_, ()| Ok(context.line.waiting())
        })?,
    )?;

    line.set(
        "busy",
        lua.create_function({
            let context = context.clone();
            move |_, ()| Ok(context.line.pending_output() > 0)
        })?,
    )?;

    line.set(
        "is_open",
        lua.create_function({
            let context = context.clone();
            move |_, ()| Ok(context.line.is_open())
        })?,
    )?;

    line.set(
        "wait_drained",
        lua.create_function({
            let context = context.clone();
            move |_, timeout: Option<u64>| {
                let deadline = Instant::now() + Duration::from_millis(timeout.unwrap_or(30_000));
                while context.line.pending_output() > 0 {
                    context.check()?;
                    if Instant::now() >= deadline {
                        return Ok(false);
                    }
                    context.cancel.sleep(SLICE).map_err(external)?;
                }
                Ok(true)
            }
        })?,
    )?;

    Ok(line)
}

/// Answers with an error unless the line is there to be written to.
///
/// A line that is shut is one the session has moved on from — the run was
/// given up on, or the device was disconnected — and a byte written into it
/// would quietly vanish. It is also what makes loading a script to read its
/// manifest safe: the line it is given then is not open, so a script that
/// writes at the top of its file fails there.
fn open(context: &Arc<Context>) -> mlua::Result<()> {
    context.check()?;
    if !context.line.is_open() {
        return Err(external(ScriptError::LineClosed));
    }
    Ok(())
}

/// Reads for as long as it is given, in slices, and answers with what came.
///
/// A timeout of nothing is one slice: a script that polls the line in a loop
/// of its own asks for what is there.
fn wait(context: &Arc<Context>, timeout: u64) -> mlua::Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_millis(timeout);
    let mut read = Vec::new();

    loop {
        context.check()?;
        if !context.line.is_open() {
            return Err(external(ScriptError::LineClosed));
        }

        let left = deadline.saturating_duration_since(Instant::now());
        context.line.read(&mut read, SLICE.min(left.max(SLICE)));
        if !read.is_empty() || Instant::now() >= deadline {
            return Ok(read);
        }
    }
}

/// Types a command line the way a person would.
///
/// Every line is closed with a carriage return, which is what a keyboard
/// sends, and an empty line is dropped: a script writing a block of text means
/// the commands in it, not the blank lines between them.
pub(crate) fn typed(text: &str) -> String {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.is_empty())
        .map(|line| format!("{line}\r"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_typed_as_a_keyboard_sends_it() {
        assert_eq!(typed("pwd"), "pwd\r");
        assert_eq!(typed("cd /tmp\nls"), "cd /tmp\rls\r");
    }

    #[test]
    fn a_blank_line_of_a_block_is_not_typed() {
        assert_eq!(typed("one\n\ntwo\n"), "one\rtwo\r");
    }

    #[test]
    fn a_line_already_ending_in_a_return_is_not_given_a_second_one() {
        assert_eq!(typed("pwd\r\n"), "pwd\r");
    }
}
