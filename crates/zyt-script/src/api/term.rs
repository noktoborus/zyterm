//! `zyt.term`, `zyt.notice`, `zyt.progress` and `zyt.log`: what a script says.
//!
//! Three of these reach the screen and they are not the same thing. While a
//! script holds the line the device is not drawn at all, so `zyt.term` is how
//! a script shows what it is doing: the bytes go into the terminal as they
//! stand and an escape sequence in them does what it says. `zyt.notice` is the
//! frame and the colour of the application, for the few lines that are about
//! the run rather than about the device. `zyt.log` goes where the log of the
//! program goes and nowhere near the screen.

use super::Context;
use crate::host::{NoticeKind, Progress, Speed, Stage, StageKind};
use mlua::{Lua, Table};
use std::sync::Arc;

/// Builds `zyt.term`.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let term = lua.create_table()?;

    term.set(
        "write",
        lua.create_function({
            let context = context.clone();
            move |_, data: mlua::LuaString| {
                context.prompt.echo(&data.as_bytes());
                Ok(())
            }
        })?,
    )?;

    term.set(
        "print",
        lua.create_function({
            let context = context.clone();
            move |_, text: String| {
                let mut bytes = text.into_bytes();
                bytes.extend_from_slice(b"\r\n");
                context.prompt.echo(&bytes);
                Ok(())
            }
        })?,
    )?;

    Ok(term)
}

/// Builds `zyt.notice`.
pub(crate) fn notices(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let notice = lua.create_table()?;

    notice.set(
        "info",
        lua.create_function({
            let context = context.clone();
            move |_, text: String| {
                context.prompt.notice(NoticeKind::Info, text);
                Ok(())
            }
        })?,
    )?;

    notice.set(
        "error",
        lua.create_function({
            let context = context.clone();
            move |_, text: String| {
                context.prompt.notice(NoticeKind::Error, text);
                Ok(())
            }
        })?,
    )?;

    Ok(notice)
}

/// Builds `zyt.progress`.
pub(crate) fn progress(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let progress = lua.create_table()?;

    progress.set(
        "share",
        lua.create_function({
            let context = context.clone();
            move |_, share: i64| {
                context.prompt.progress(Progress::share(share));
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "indeterminate",
        lua.create_function({
            let context = context.clone();
            move |_, ()| {
                context.prompt.progress(Progress::Indeterminate);
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "error",
        lua.create_function({
            let context = context.clone();
            move |_, share: Option<i64>| {
                let share = share.unwrap_or(0).clamp(0, 100) as u8;
                context.prompt.progress(Progress::Error(share));
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "paused",
        lua.create_function({
            let context = context.clone();
            move |_, share: Option<i64>| {
                let share = share.unwrap_or(0).clamp(0, 100) as u8;
                context.prompt.progress(Progress::Paused(share));
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "clear",
        lua.create_function({
            let context = context.clone();
            move |_, ()| {
                context.prompt.progress(Progress::Removed);
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "stage",
        lua.create_function({
            let context = context.clone();
            move |_, how: Table| {
                context.prompt.stage(stage_of(&how)?);
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "speed",
        lua.create_function({
            let context = context.clone();
            move |_, how: Table| {
                let way = |name: &str| -> mlua::Result<Option<u64>> {
                    Ok(how
                        .get::<Option<f64>>(name)?
                        .map(|rate| rate.max(0.0) as u64))
                };
                context.prompt.speed(Speed {
                    received: way("received")?,
                    sent: way("sent")?,
                });
                Ok(())
            }
        })?,
    )?;

    progress.set(
        "bytes",
        lua.create_function({
            let context = context.clone();
            move |_, (done, whole): (u64, Option<u64>)| {
                let whole = whole.filter(|whole| *whole > 0);
                context.prompt.bytes(done, whole);
                context.prompt.progress(match whole {
                    Some(whole) => Progress::Share((done.min(whole) * 100 / whole) as u8),
                    None => Progress::Indeterminate,
                });
                Ok(())
            }
        })?,
    )?;

    Ok(progress)
}

/// The stage a script described: a kind out of the list, or a text alone.
///
/// A kind that is none of them is refused rather than shown as something
/// else: it is a name a script got wrong, and it is wrong on every run.
fn stage_of(how: &Table) -> mlua::Result<Stage> {
    let text: Option<String> = how.get("text")?;
    let kind = match how.get::<Option<String>>("kind")? {
        Some(name) => StageKind::of_name(&name).ok_or_else(|| {
            mlua::Error::runtime(format!("zyt.progress.stage: no stage is called {name}"))
        })?,
        None if text.is_some() => StageKind::Other,
        None => {
            return Err(mlua::Error::runtime(
                "zyt.progress.stage: a stage has a kind or a text",
            ));
        }
    };
    Ok(Stage {
        kind,
        name: how.get("name")?,
        file: how.get("file")?,
        files: how.get("files")?,
        count: how.get("count")?,
        text,
    })
}

/// Builds `zyt.log`.
pub(crate) fn journal(lua: &Lua) -> mlua::Result<Table> {
    let journal = lua.create_table()?;

    journal.set(
        "debug",
        lua.create_function(|_, text: String| {
            log::debug!("script: {text}");
            Ok(())
        })?,
    )?;
    journal.set(
        "info",
        lua.create_function(|_, text: String| {
            log::info!("script: {text}");
            Ok(())
        })?,
    )?;
    journal.set(
        "warn",
        lua.create_function(|_, text: String| {
            log::warn!("script: {text}");
            Ok(())
        })?,
    )?;
    journal.set(
        "error",
        lua.create_function(|_, text: String| {
            log::error!("script: {text}");
            Ok(())
        })?,
    )?;

    Ok(journal)
}
