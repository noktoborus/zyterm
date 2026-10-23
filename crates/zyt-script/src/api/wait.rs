//! `zyt.wait`: one wait for the line, the sockets and a timer at once.
//!
//! ```lua
//! local ready = zyt.wait{ line = true, sockets = { listener, client },
//!                         writable = { client }, timeout = 1000 }
//! -- ready.line, ready[listener], ready[client], ready.writable[client]
//! ```
//!
//! The sockets are waited for by the multiplexer of the system (`polling`:
//! epoll, kqueue, IOCP). The line is no socket: it wakes the wait itself when
//! the device says something ([`crate::Line::set_wake`]). The wait goes in
//! slices, and the ask to stop is read between them, as every wait of the
//! host does. A socket that is closed is ready at once, so the script reads
//! that it is gone.

use super::{Context, external};
use crate::error::ScriptError;
use crate::sockets::Held;
use mlua::{AnyUserData, Lua, Table, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long one wait inside a longer one lasts.
const SLICE: Duration = Duration::from_millis(50);

/// Builds the function `zyt.wait`.
pub(crate) fn function(lua: &Lua, context: &Arc<Context>) -> mlua::Result<mlua::Function> {
    lua.create_function({
        let context = context.clone();
        move |lua, how: Table| wait(lua, &context, &how)
    })
}

/// One socket asked about, and what it is asked for.
struct Asked {
    value: AnyUserData,
    held: Option<Held>,
    readable: bool,
    writable: bool,
}

fn wait(lua: &Lua, context: &Arc<Context>, how: &Table) -> mlua::Result<Table> {
    context.check()?;
    let line = how.get::<Option<bool>>("line")?.unwrap_or(false);
    let timeout = how
        .get::<Option<u64>>("timeout")?
        .map(Duration::from_millis);
    let mut asked: Vec<Asked> = Vec::new();
    take_sockets(how, "sockets", &mut asked, |asked| asked.readable = true)?;
    take_sockets(how, "writable", &mut asked, |asked| asked.writable = true)?;

    let poller = context.poller.clone();
    if line {
        let poller = poller.clone();
        context.line.set_wake(Some(Arc::new(move || {
            let _ = poller.notify();
        })));
    }
    let added = register(&poller, &asked);
    let found = match added {
        Ok(()) => watch(context, &poller, &asked, line, timeout),
        Err(source) => Err(external(ScriptError::Wait { source })),
    };
    unregister(&poller, &asked);
    if line {
        context.line.set_wake(None);
    }
    let (line_ready, readable, writable, timed_out) = found?;

    let ready = lua.create_table()?;
    ready.set("line", line_ready)?;
    ready.set("timeout", timed_out)?;
    let ready_to_write = lua.create_table()?;
    for (index, socket) in asked.iter().enumerate() {
        if readable[index] {
            ready.set(Value::UserData(socket.value.clone()), true)?;
        }
        if writable[index] {
            ready_to_write.set(Value::UserData(socket.value.clone()), true)?;
        }
    }
    ready.set("writable", ready_to_write)?;
    Ok(ready)
}

/// Adds the sockets of `how[field]` to what is asked, once each.
fn take_sockets(
    how: &Table,
    field: &str,
    asked: &mut Vec<Asked>,
    mark: impl Fn(&mut Asked),
) -> mlua::Result<()> {
    let Some(list) = how.get::<Option<Table>>(field)? else {
        return Ok(());
    };
    for value in list.sequence_values::<AnyUserData>() {
        let value = value?;
        let Some(held) = super::net::held_of(&value) else {
            return Err(mlua::Error::external(format!(
                "zyt.wait: {field} holds something that is no socket"
            )));
        };
        match asked.iter_mut().find(|known| known.value == value) {
            Some(known) => mark(known),
            None => {
                let mut fresh = Asked {
                    value,
                    held,
                    readable: false,
                    writable: false,
                };
                mark(&mut fresh);
                asked.push(fresh);
            }
        }
    }
    Ok(())
}

/// Hands every open socket to the multiplexer, its index as the key.
///
/// Every socket added here is deleted by [`unregister`] before the wait
/// returns, and the `Arc` of [`Asked::held`] keeps it open meanwhile, which is
/// what `Poller::add` asks of its caller.
fn register(poller: &polling::Poller, asked: &[Asked]) -> std::io::Result<()> {
    for (key, socket) in asked.iter().enumerate() {
        let interest = polling::Event::new(key, socket.readable, socket.writable);
        let added = match &socket.held {
            Some(Held::Listener(listener)) => unsafe { poller.add(&**listener, interest) },
            Some(Held::Stream(stream)) => unsafe { poller.add(&**stream, interest) },
            None => Ok(()),
        };
        added?;
    }
    Ok(())
}

/// Takes every socket back from the multiplexer.
fn unregister(poller: &polling::Poller, asked: &[Asked]) {
    for socket in asked {
        let _ = match &socket.held {
            Some(Held::Listener(listener)) => poller.delete(&**listener),
            Some(Held::Stream(stream)) => poller.delete(&**stream),
            None => Ok(()),
        };
    }
}

/// What a wait found: the line, the sockets ready to read and to write, and
/// whether the time was up first.
type Found = (bool, Vec<bool>, Vec<bool>, bool);

/// Waits in slices until something is ready, the time is up or the run is
/// asked to stop.
fn watch(
    context: &Context,
    poller: &polling::Poller,
    asked: &[Asked],
    line: bool,
    timeout: Option<Duration>,
) -> mlua::Result<Found> {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut readable: Vec<bool> = asked
        .iter()
        .map(|socket| socket.held.is_none() && socket.readable)
        .collect();
    let mut writable: Vec<bool> = asked
        .iter()
        .map(|socket| socket.held.is_none() && socket.writable)
        .collect();
    let mut events = polling::Events::new();
    loop {
        let line_ready = line && (context.line.waiting() > 0 || !context.line.is_open());
        if line_ready || readable.contains(&true) || writable.contains(&true) {
            return Ok((line_ready, readable, writable, false));
        }
        context.check()?;
        let left = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if left.is_some_and(|left| left.is_zero()) {
            return Ok((false, readable, writable, true));
        }
        events.clear();
        poller
            .wait(
                &mut events,
                Some(left.map_or(SLICE, |left| left.min(SLICE))),
            )
            .map_err(|source| external(ScriptError::Wait { source }))?;
        for event in events.iter() {
            if event.key < asked.len() {
                readable[event.key] |= event.readable;
                writable[event.key] |= event.writable;
            }
        }
    }
}
