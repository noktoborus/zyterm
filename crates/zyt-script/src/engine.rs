//! One interpreter, with one script loaded into it.
//!
//! A script is a Lua file answering with a table: what it says about itself
//! (`manifest`) and a function per direction. The interpreter it is loaded
//! into carries the tables of the host and nothing else — the libraries that
//! could start a process or end the program are taken out, `debug` is never
//! loaded, so the hook that stops a tight loop cannot be removed.
//!
//! Nothing here is read to list what is installed: that is the manifest beside
//! the script, and it is a file. An interpreter is made when a script is about
//! to run, or when somebody asks whether it could run at all — and then the
//! line and the dialog it is given answer every call with an error, so a
//! script doing its work at the top of the file fails there rather than on a
//! device.

use crate::api::{self, Context};
use crate::error::{Result, ScriptError};
use crate::form::{Form, Value};
use crate::host::{Line, NoticeKind, Progress, Prompt};
use crate::interrupt::{self, Cancel};
use crate::manifest::Manifest;
use crate::registry::ProcessRegistry;
use crate::runner::JobRunner;
use crate::target::{Direction, Targets};
use mlua::{Lua, Table};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The three the script talks to: the line, the one asking, and the ask to
/// stop.
///
/// They travel together because they are one thing — who is at the other end
/// of this run — and because what stands at that end is what every kind of
/// run differs by: a window, a pseudo terminal, a conversation written down
/// beforehand.
pub struct Talking {
    /// The line of the session.
    pub line: Arc<dyn Line>,
    /// The one the script is talking to.
    pub prompt: Arc<dyn Prompt>,
    /// The ask to stop.
    pub cancel: Arc<Cancel>,
}

/// Everything a run of one script is given.
pub struct Start {
    /// The name of its directory, which is what everything but a person goes
    /// by.
    pub id: String,
    /// What it says about itself.
    pub manifest: Manifest,
    /// The directory of the script itself.
    pub directory: PathBuf,
    /// The Lua it is started from.
    pub path: PathBuf,
    /// The directory of scripts it was found in.
    pub root: PathBuf,
    /// Every directory scripts are looked for in, in the order they are.
    pub roots: Vec<PathBuf>,
    /// Which way it was asked to go.
    pub direction: Direction,
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
}

impl Start {
    /// A start for one script of a library, going that way.
    pub fn of_entry(
        entry: &crate::search::Entry,
        roots: &[PathBuf],
        direction: Direction,
        target: Targets,
        variables: BTreeMap<String, String>,
        talking: Talking,
    ) -> Self {
        Self {
            id: entry.id.clone(),
            manifest: entry.manifest.clone(),
            directory: entry.directory.clone(),
            path: entry.path.clone(),
            root: entry.root.clone(),
            roots: roots.to_vec(),
            direction,
            target,
            variables,
            line: talking.line,
            prompt: talking.prompt,
            cancel: talking.cancel,
        }
    }

    /// A start for loading a script and running none of it.
    ///
    /// Everything that acts refuses: the ask to stop is already standing, so
    /// the line, the dialog, the clock and anything that starts a process
    /// answer with an error, while reading the files of the script goes on
    /// working — a script whose library reads its templates as it loads is
    /// checked like any other. The hook is not hardened, so the refusal is an
    /// error the file can be blamed for rather than one it cannot catch.
    pub fn for_reading(entry: &crate::search::Entry, roots: &[PathBuf]) -> Self {
        Self::of_entry(
            entry,
            roots,
            Direction::Send,
            Targets::none(),
            BTreeMap::new(),
            Talking {
                line: Arc::new(Nothing),
                prompt: Arc::new(Nothing),
                cancel: already_asked(),
            },
        )
    }
}

/// An ask to stop that was made before anything started.
fn already_asked() -> Arc<Cancel> {
    let cancel = Cancel::new();
    cancel.ask();
    cancel
}

/// A line and a dialog that answer nothing, for loading without running.
#[derive(Debug)]
struct Nothing;

impl Line for Nothing {
    fn write(&self, _: &[u8]) {}

    fn read(&self, into: &mut Vec<u8>, _: Duration) {
        into.clear();
    }

    fn pending_output(&self) -> usize {
        0
    }

    fn is_open(&self) -> bool {
        false
    }
}

impl Prompt for Nothing {
    fn ask(&self, _: Form) -> Option<BTreeMap<String, Value>> {
        None
    }

    fn notice(&self, _: NoticeKind, _: String) {}

    fn echo(&self, _: &[u8]) {}

    fn progress(&self, _: Progress) {}
}

/// One script, loaded and ready to be called.
pub struct Engine {
    lua: Lua,
    script: Table,
    manifest: Manifest,
    context: Arc<Context>,
}

impl Engine {
    /// Loads the script and reads what it says about itself.
    pub fn load(start: Start) -> Result<Self> {
        let lua = Lua::new();
        lua.set_memory_limit(MEMORY_LIMIT).ok();
        api::restrict(&lua)?;

        let context = Arc::new(Context {
            name: start.id.clone(),
            direction: start.direction,
            root: start.directory.clone(),
            roots: start.roots.clone(),
            target: start.target,
            variables: start.variables,
            line: start.line,
            prompt: start.prompt,
            cancel: start.cancel.clone(),
            processes: Arc::new(Mutex::new(ProcessRegistry::new()?)),
            jobs: Arc::new(Mutex::new(JobRunner::new())),
        });
        api::install(&lua, &context)?;
        set_path(&lua, &start.directory, &start.roots, &context.name)?;
        interrupt::install(&lua, &start.cancel)?;

        let text = std::fs::read_to_string(&start.path).map_err(|source| ScriptError::Read {
            path: start.path.clone(),
            source,
        })?;
        let script: Table = lua
            .load(&text)
            .set_name(format!("@{}", start.path.display()))
            .eval()
            .map_err(|source| ScriptError::Load {
                path: start.path.clone(),
                source,
            })?;

        for direction in [Direction::Send, Direction::Receive] {
            if start.manifest.offers(direction)
                && !script
                    .get::<mlua::Value>(direction.name())
                    .is_ok_and(|value| value.is_function())
            {
                return Err(ScriptError::Manifest {
                    name: start.id.clone(),
                    what: format!(
                        "its manifest offers {} and the script carries no such function",
                        direction.name()
                    ),
                });
            }
        }

        Ok(Self {
            lua,
            script,
            manifest: start.manifest,
            context,
        })
    }

    /// What the script says about itself.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Calls the function of that direction.
    pub fn call(&self, direction: Direction) -> Result<()> {
        if !self.manifest.offers(direction) {
            return Err(ScriptError::NoDirection {
                name: self.manifest.name.clone(),
                direction: direction.name().to_string(),
            });
        }

        let entry: mlua::Function =
            self.script
                .get(direction.name())
                .map_err(|source| ScriptError::Runtime {
                    name: self.manifest.name.clone(),
                    said: said_of(&source),
                    source,
                })?;
        entry.call::<()>(()).map_err(|source| self.failed(source))
    }

    /// Calls the function a script tidies up with, when it carries one.
    ///
    /// It is called with the ask to stop already standing, so every call of
    /// the line it makes is refused: a script that has something to say to the
    /// device says it here and is given a budget for that and nothing more.
    pub fn cleanup(&self) -> Result<()> {
        let Ok(entry) = self.script.get::<mlua::Function>("cleanup") else {
            return Ok(());
        };
        entry.call::<()>(()).map_err(|source| self.failed(source))
    }

    /// Every process this script started.
    pub fn processes(&self) -> Arc<Mutex<ProcessRegistry>> {
        self.context.processes.clone()
    }

    /// The jobs it started beside the line.
    pub fn jobs(&self) -> Arc<Mutex<JobRunner>> {
        self.context.jobs.clone()
    }

    /// Leaves the interpreter standing for ever.
    ///
    /// It is what a run that was given up on does with it: the thread may
    /// still be inside the interpreter, and dropping the state under it would
    /// be reading freed memory. One such state is the price of a script that
    /// answered neither the flag nor the hook.
    pub fn abandon(self) {
        std::mem::forget(self.script);
        std::mem::forget(self.lua);
    }

    /// The error of a script that failed while it ran.
    ///
    /// An error this crate made and handed to the interpreter comes back as
    /// itself, so a cancelled run reads as cancelled rather than as a script
    /// that went wrong.
    fn failed(&self, source: mlua::Error) -> ScriptError {
        if let mlua::Error::CallbackError { cause, .. } = &source
            && let mlua::Error::ExternalError(error) = cause.as_ref()
            && let Some(ScriptError::Cancelled) = error.downcast_ref::<ScriptError>()
        {
            return ScriptError::Cancelled;
        }
        if let mlua::Error::ExternalError(error) = &source
            && let Some(ScriptError::Cancelled) = error.downcast_ref::<ScriptError>()
        {
            return ScriptError::Cancelled;
        }
        ScriptError::Runtime {
            name: self.manifest.name.clone(),
            said: said_of(&source),
            source,
        }
    }
}

/// The one line a script raised, out of everything the interpreter says.
///
/// What it says is the message, then where it was raised, then the walk of the
/// stack. A script raising its complaint with no position — which is what
/// `error(text, 0)` does — leaves the first line as the whole of it.
fn said_of(error: &mlua::Error) -> String {
    let whole = match error {
        mlua::Error::CallbackError { cause, .. } => said_of(cause),
        mlua::Error::ExternalError(error) => error.to_string(),
        other => other.to_string(),
    };

    whole
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .trim_start_matches("runtime error: ")
        .to_string()
}

/// How much memory one script may hold.
///
/// A script that asks for more than this is refused by the allocator, which is
/// what catches a single call of the interpreter that would otherwise run for
/// minutes with the hook never getting a turn.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

/// Points `require` at the directory of the script and the libraries beside
/// the others.
///
/// Its own directory first, so a script of several files finds them by name,
/// and then every directory of scripts in the order they are searched, which
/// is where `lib/` stands.
fn set_path(lua: &Lua, directory: &Path, roots: &[PathBuf], name: &str) -> Result<()> {
    let mut ordered: Vec<PathBuf> = vec![directory.to_path_buf()];
    ordered.extend(roots.iter().filter(|other| *other != directory).cloned());

    let path = api::package_path(&ordered);
    let set = || -> mlua::Result<()> {
        let package: Table = lua.globals().get("package")?;
        package.set("path", path)?;
        package.set("cpath", "")?;
        Ok(())
    };
    set().map_err(|source| ScriptError::Runtime {
        name: name.to_string(),
        said: source.to_string(),
        source,
    })
}

/// Loads a script and says nothing when it could be run.
pub fn check(entry: &crate::search::Entry, roots: &[PathBuf]) -> Result<()> {
    Engine::load(Start::for_reading(entry, roots)).map(|_| ())
}
