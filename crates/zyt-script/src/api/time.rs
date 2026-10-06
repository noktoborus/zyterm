//! `zyt.time`: waiting, and knowing how long is left.

use super::{Context, external};
use mlua::{Lua, Table};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let time = lua.create_table()?;

    time.set(
        "now_ms",
        lua.create_function(|_, ()| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            Ok(now.as_millis() as i64)
        })?,
    )?;

    time.set(
        "sleep",
        lua.create_function({
            let context = context.clone();
            move |_, how_long: u64| {
                context
                    .cancel
                    .sleep(Duration::from_millis(how_long))
                    .map_err(external)
            }
        })?,
    )?;

    time.set(
        "deadline",
        lua.create_function({
            let context = context.clone();
            move |lua, how_long: u64| {
                let ends = Instant::now() + Duration::from_millis(how_long);
                let deadline = lua.create_table()?;
                deadline.set(
                    "left",
                    lua.create_function(move |_, ()| {
                        Ok(ends.saturating_duration_since(Instant::now()).as_millis() as i64)
                    })?,
                )?;
                deadline.set(
                    "passed",
                    lua.create_function(move |_, ()| Ok(Instant::now() >= ends))?,
                )?;
                let context = context.clone();
                deadline.set(
                    "check",
                    lua.create_function(move |_, ()| {
                        context.check()?;
                        Ok(Instant::now() < ends)
                    })?,
                )?;
                Ok(deadline)
            }
        })?,
    )?;

    Ok(time)
}
