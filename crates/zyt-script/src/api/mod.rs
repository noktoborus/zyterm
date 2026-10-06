//! What a script may call, and the state those calls share.
//!
//! Every table is installed under one global, `zyt`, so a script says what it
//! is reaching for: `zyt.line`, `zyt.proc`, `zyt.term`. Nothing of the host is
//! reachable any other way — the libraries that could start a process or end
//! the program are taken out — which is what makes the list of processes a
//! script started complete rather than conscientious.

pub(crate) mod codec;
pub(crate) mod dialog;
pub(crate) mod fs;
pub(crate) mod line;
pub(crate) mod proc;
pub mod script;
pub(crate) mod shell;
pub(crate) mod target;
pub(crate) mod term;
pub(crate) mod time;

use crate::error::{Result, ScriptError};
use crate::host::{Line, Prompt};
use crate::interrupt::Cancel;
use crate::registry::ProcessRegistry;
use crate::runner::JobRunner;
use crate::target::{Direction, Targets};
use mlua::{Lua, Table};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Everything the calls of one running script share.
pub(crate) struct Context {
    /// What the script is called.
    pub name: String,
    /// Which way it was asked to go.
    pub direction: Direction,
    /// The directory its own file came from.
    pub root: PathBuf,
    /// Every directory scripts are looked for in, in the order they are.
    pub roots: Vec<PathBuf>,
    /// What the user picked for it to carry.
    pub target: Targets,
    /// The values of the source, by name.
    pub variables: BTreeMap<String, String>,
    /// The line of the session.
    pub line: Arc<dyn Line>,
    /// The one the script is talking to.
    pub prompt: Arc<dyn Prompt>,
    /// The ask to stop.
    pub cancel: Arc<Cancel>,
    /// Every process this script started.
    pub processes: Arc<Mutex<ProcessRegistry>>,
    /// The programs it started beside the line.
    pub jobs: Arc<Mutex<JobRunner>>,
}

impl Context {
    /// The error a call answers with when stopping was asked for.
    pub fn check(&self) -> mlua::Result<()> {
        self.cancel.check().map_err(external)
    }
}

/// Turns an error of this crate into one the interpreter carries.
pub(crate) fn external(error: ScriptError) -> mlua::Error {
    mlua::Error::external(error)
}

/// Installs every table of the host under the one global `zyt`.
pub(crate) fn install(lua: &Lua, context: &Arc<Context>) -> Result<()> {
    let build = || -> mlua::Result<()> {
        let zyt = lua.create_table()?;
        zyt.set("line", line::table(lua, context)?)?;
        zyt.set("term", term::table(lua, context)?)?;
        zyt.set("notice", term::notices(lua, context)?)?;
        zyt.set("progress", term::progress(lua, context)?)?;
        zyt.set("log", term::journal(lua)?)?;
        zyt.set("ui", dialog::table(lua, context)?)?;
        zyt.set("proc", proc::table(lua, context)?)?;
        zyt.set("fs", fs::table(lua)?)?;
        zyt.set("codec", codec::table(lua)?)?;
        zyt.set("shell", shell::table(lua)?)?;
        zyt.set("script", script::table(lua, context)?)?;
        zyt.set("target", target::table(lua, context)?)?;
        zyt.set("vars", target::variables(lua, context)?)?;
        zyt.set("time", time::table(lua, context)?)?;
        lua.globals().set("zyt", zyt)?;
        Ok(())
    };

    build().map_err(|source| ScriptError::Runtime {
        name: context.name.clone(),
        said: source.to_string(),
        source,
    })
}

/// Takes out of the interpreter everything that would reach past these tables.
///
/// A script that could call `os.execute` would start a process this crate
/// never heard of, and stopping the script would leave it running; one that
/// could call `os.exit` would take the window with it. `debug` is not loaded
/// at all, so the hook that stops a tight loop cannot be taken off.
pub(crate) fn restrict(lua: &Lua) -> Result<()> {
    let strip = || -> mlua::Result<()> {
        let globals = lua.globals();
        if let Ok(os) = globals.get::<Table>("os") {
            os.set("execute", mlua::Value::Nil)?;
            os.set("exit", mlua::Value::Nil)?;
            os.set("setlocale", mlua::Value::Nil)?;
        }
        if let Ok(io) = globals.get::<Table>("io") {
            io.set("popen", mlua::Value::Nil)?;
        }
        globals.set("dofile", mlua::Value::Nil)?;
        globals.set("loadfile", mlua::Value::Nil)?;
        Ok(())
    };

    strip().map_err(|source| ScriptError::Runtime {
        name: String::from("the sandbox"),
        said: source.to_string(),
        source,
    })
}

/// Where `require` looks, for a script of this root.
///
/// Its own directory first and then the others in the order they are searched,
/// so a library of the configuration directory stands in for the one shipped
/// beside the program, exactly as a script of that directory does.
pub(crate) fn package_path(roots: &[PathBuf]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for root in roots {
        let root = root.to_string_lossy();
        parts.push(format!("{root}/?.lua"));
        parts.push(format!("{root}/lib/?.lua"));
        parts.push(format!("{root}/lib/?/init.lua"));
    }
    parts.join(";")
}
