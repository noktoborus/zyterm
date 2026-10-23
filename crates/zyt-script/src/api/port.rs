//! `zyt.port`: the serial port behind the line, when there is one.
//!
//! On a console every call says so: `params` and `lines` give nil, and the
//! rest false. A script that serves RFC 2217 refuses option 44 there.

use super::Context;
use crate::host::{PortLines, PortSwitch};
use mlua::{Lua, Table, Value};
use std::sync::Arc;

/// Builds `zyt.port`.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let port = lua.create_table()?;
    let line = || context.line.clone();

    let read = line();
    port.set(
        "params",
        lua.create_function(move |lua, ()| {
            let Some(params) = read.port() else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            table.set("baud", params.baud)?;
            table.set("data_bits", params.data_bits)?;
            table.set("parity", params.parity)?;
            table.set("stop_bits", params.stop_bits)?;
            table.set("flow", params.flow)?;
            Ok(Value::Table(table))
        })?,
    )?;

    let write = line();
    port.set(
        "set_params",
        lua.create_function(move |_, asked: Table| {
            let Some(mut params) = write.port() else {
                return Ok(false);
            };
            params.baud = asked.get::<Option<u32>>("baud")?.unwrap_or(params.baud);
            params.data_bits = asked
                .get::<Option<u8>>("data_bits")?
                .unwrap_or(params.data_bits);
            params.parity = asked
                .get::<Option<String>>("parity")?
                .unwrap_or(params.parity);
            params.stop_bits = asked
                .get::<Option<u8>>("stop_bits")?
                .unwrap_or(params.stop_bits);
            params.flow = asked.get::<Option<String>>("flow")?.unwrap_or(params.flow);
            Ok(write.set_port(&params))
        })?,
    )?;

    let switch = |name: &str, switch: fn(Option<bool>) -> PortSwitch| -> mlua::Result<()> {
        let drive = line();
        port.set(
            name,
            lua.create_function(move |_, on: Option<bool>| Ok(drive.switch_port(switch(on))))?,
        )
    };
    switch("set_rts", PortSwitch::Rts)?;
    switch("set_dtr", PortSwitch::Dtr)?;
    switch("set_break", |on| PortSwitch::Break(on.unwrap_or(false)))?;
    switch("hold", |on| PortSwitch::Hold(on.unwrap_or(false)))?;

    let purge = line();
    port.set(
        "purge",
        lua.create_function(move |_, ()| Ok(purge.switch_port(PortSwitch::Purge)))?,
    )?;

    let lines = line();
    port.set(
        "lines",
        lua.create_function(move |lua, ()| {
            let Some(PortLines {
                rts,
                dtr,
                cts,
                dsr,
                cd,
                ri,
            }) = lines.port_lines()
            else {
                return Ok(Value::Nil);
            };
            let table = lua.create_table()?;
            for (name, on) in [
                ("rts", rts),
                ("dtr", dtr),
                ("cts", cts),
                ("dsr", dsr),
                ("cd", cd),
                ("ri", ri),
            ] {
                table.set(name, on)?;
            }
            Ok(Value::Table(table))
        })?,
    )?;

    Ok(port)
}
