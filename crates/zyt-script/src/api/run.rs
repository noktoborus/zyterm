//! `zyt.run`: what a script says about its run as a whole.

use super::Context;
use mlua::{Lua, Table};
use std::sync::Arc;

/// The table `zyt.run`.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let run = lua.create_table()?;
    run.set(
        "ending",
        lua.create_function({
            let context = context.clone();
            move |_, ()| {
                context.prompt.ending();
                Ok(())
            }
        })?,
    )?;
    Ok(run)
}
