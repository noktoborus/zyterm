//! `zyt.proc`: the programs a script starts, and their pipes.
//!
//! This is what a transfer profile used to be. A script starts a program with
//! pipes on all three channels and may read and write them itself, which is
//! what makes a protocol scriptable at all; or it hands the program to the
//! line with [`attach_to_line`], and then the bytes of the file never become
//! Lua strings — four threads move them, exactly as the gateway of a transfer
//! program did.
//!
//! Nothing here starts a process outside [`ProcessRegistry`]: `os.execute` and
//! `io.popen` are taken out of the interpreter, so the list of what a script
//! started is complete and stopping the script stops all of it.
//!
//! [`attach_to_line`]: Spawned
//! [`ProcessRegistry`]: crate::ProcessRegistry

use super::{Context, external};
use crate::chunks::ByteSwap;
use crate::error::ScriptError;
use crate::host::{Line, NoticeKind, Prompt};
use crate::interrupt::Cancel;
use crate::process::{own_group, shell_command, spawn_thread};
use crate::registry::ProcId;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::io::{Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How much of a pipe is read at once.
const PIPE: usize = 8 * 1024;

/// How long one wait inside a longer one lasts.
const SLICE: Duration = Duration::from_millis(50);

/// How long the thread feeding a program waits for something to feed it.
const FEED_WAIT: Duration = Duration::from_millis(250);

/// How often a program is asked whether it is still running.
const REAP: Duration = Duration::from_millis(20);

/// One program a script started.
pub(crate) struct Spawned {
    id: ProcId,
    name: String,
    pid: u32,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    out: Arc<ByteSwap>,
    err: Arc<ByteSwap>,
    out_left: Mutex<Vec<u8>>,
    err_left: Mutex<Vec<u8>>,
    code: Arc<Mutex<Option<i32>>>,
    ended: Arc<AtomicBool>,
    drained: Arc<AtomicBool>,
    attached: Arc<AtomicBool>,
    context: Arc<Context>,
}

impl Spawned {
    /// True once the program is gone and its output has been read to the end.
    fn finished(&self) -> bool {
        self.ended.load(Ordering::Acquire)
            && self.drained.load(Ordering::Acquire)
            && self.out.is_empty()
    }

    /// Waits for the program to be gone and answers with its code.
    fn wait(&self, timeout: Duration) -> mlua::Result<Option<i32>> {
        let deadline = Instant::now() + timeout;
        while !self.finished() {
            self.context.check()?;
            if Instant::now() >= deadline {
                return Ok(None);
            }
            let _ = self.context.cancel.sleep(SLICE.min(REAP.max(SLICE)));
            self.context.check()?;
        }
        Ok(self.code.lock().ok().and_then(|code| *code))
    }

    /// Takes what is waiting on one of the pipes.
    fn take(
        &self,
        pipe: &Arc<ByteSwap>,
        left: &Mutex<Vec<u8>>,
        count: Option<usize>,
        timeout: Duration,
    ) -> mlua::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut spare = Vec::new();

        loop {
            {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                let enough = match count {
                    Some(count) => waiting.len() >= count,
                    None => !waiting.is_empty(),
                };
                if enough {
                    let taken = match count {
                        Some(count) => waiting.drain(..count).collect(),
                        None => std::mem::take(&mut *waiting),
                    };
                    return Ok(taken);
                }
            }

            self.context.check()?;
            let left_over = deadline.saturating_duration_since(Instant::now());
            pipe.take_into_wait(&mut spare, SLICE.min(left_over.max(SLICE)));
            if !spare.is_empty() {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                waiting.append(&mut spare);
                continue;
            }

            if Instant::now() >= deadline || self.ended.load(Ordering::Acquire) {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                return Ok(std::mem::take(&mut *waiting));
            }
        }
    }

    /// Takes one line of one of the pipes, without the newline that ends it.
    fn take_line(
        &self,
        pipe: &Arc<ByteSwap>,
        left: &Mutex<Vec<u8>>,
        timeout: Duration,
    ) -> mlua::Result<Option<String>> {
        let deadline = Instant::now() + timeout;
        let mut spare = Vec::new();

        loop {
            {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                if let Some(at) = waiting.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = waiting.drain(..=at).collect();
                    return Ok(Some(text_of(&line)));
                }
                if self.ended.load(Ordering::Acquire)
                    && self.drained.load(Ordering::Acquire)
                    && pipe.is_empty()
                    && !waiting.is_empty()
                {
                    let line = std::mem::take(&mut *waiting);
                    return Ok(Some(text_of(&line)));
                }
            }

            self.context.check()?;
            let left_over = deadline.saturating_duration_since(Instant::now());
            pipe.take_into_wait(&mut spare, SLICE.min(left_over.max(SLICE)));
            if !spare.is_empty() {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                waiting.append(&mut spare);
                continue;
            }

            if Instant::now() >= deadline {
                return Ok(None);
            }
            if self.finished() {
                let mut waiting = left.lock().map_err(|_| closed(&self.name))?;
                if waiting.is_empty() {
                    return Ok(None);
                }
                let line = std::mem::take(&mut *waiting);
                return Ok(Some(text_of(&line)));
            }
        }
    }
}

impl UserData for Spawned {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("pid", |_, this, ()| Ok(this.pid));

        methods.add_method("running", |_, this, ()| {
            Ok(!this.ended.load(Ordering::Acquire))
        });

        methods.add_method("write", |_, this, data: mlua::LuaString| {
            this.context.check()?;
            let bytes = data.as_bytes().to_vec();
            let mut slot = this.stdin.lock().map_err(|_| closed(&this.name))?;
            let stdin = slot.as_mut().ok_or_else(|| closed(&this.name))?;
            stdin
                .write_all(&bytes)
                .and_then(|()| stdin.flush())
                .map_err(|source| {
                    external(ScriptError::Spawn {
                        command: this.name.clone(),
                        source,
                    })
                })
        });

        methods.add_method("close_stdin", |_, this, ()| {
            if let Ok(mut slot) = this.stdin.lock() {
                let _ = slot.take();
            }
            Ok(())
        });

        methods.add_method(
            "read",
            |lua, this, (count, timeout): (Option<usize>, Option<u64>)| {
                let bytes = this.take(
                    &this.out.clone(),
                    &this.out_left,
                    count,
                    Duration::from_millis(timeout.unwrap_or(0)),
                )?;
                if bytes.is_empty() {
                    return Ok(mlua::Value::Nil);
                }
                Ok(mlua::Value::String(lua.create_string(&bytes)?))
            },
        );

        methods.add_method(
            "read_err",
            |lua, this, (count, timeout): (Option<usize>, Option<u64>)| {
                let bytes = this.take(
                    &this.err.clone(),
                    &this.err_left,
                    count,
                    Duration::from_millis(timeout.unwrap_or(0)),
                )?;
                if bytes.is_empty() {
                    return Ok(mlua::Value::Nil);
                }
                Ok(mlua::Value::String(lua.create_string(&bytes)?))
            },
        );

        methods.add_method("read_line", |_, this, timeout: Option<u64>| {
            this.take_line(
                &this.out.clone(),
                &this.out_left,
                Duration::from_millis(timeout.unwrap_or(0)),
            )
        });

        methods.add_method("read_err_line", |_, this, timeout: Option<u64>| {
            this.take_line(
                &this.err.clone(),
                &this.err_left,
                Duration::from_millis(timeout.unwrap_or(0)),
            )
        });

        methods.add_method("wait", |_, this, timeout: Option<u64>| {
            this.wait(Duration::from_millis(timeout.unwrap_or(30_000)))
        });

        methods.add_method("kill", |_, this, ()| {
            let mut registry = this
                .context
                .processes
                .lock()
                .map_err(|_| closed(&this.name))?;
            registry.kill(this.id).map_err(external)
        });

        methods.add_method("attach_to_line", |_, this, how: Option<Table>| {
            attach(this, how)
        });
    }
}

/// A program handed to the line, and the threads moving the bytes.
pub(crate) struct Attached {
    out: Arc<ByteSwap>,
    ended: Arc<AtomicBool>,
    drained: Arc<AtomicBool>,
    code: Arc<Mutex<Option<i32>>>,
    detached: Arc<AtomicBool>,
    context: Arc<Context>,
}

impl UserData for Attached {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("detach", |_, this, ()| {
            this.detached.store(true, Ordering::Release);
            Ok(())
        });

        methods.add_method("wait", |_, this, timeout: Option<u64>| {
            let deadline = Instant::now() + Duration::from_millis(timeout.unwrap_or(600_000));
            loop {
                let done = this.ended.load(Ordering::Acquire)
                    && this.drained.load(Ordering::Acquire)
                    && this.out.is_empty();
                if done {
                    return Ok(this.code.lock().ok().and_then(|code| *code));
                }
                this.context.check()?;
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                let _ = this.context.cancel.sleep(SLICE);
                this.context.check()?;
            }
        });
    }
}

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let proc = lua.create_table()?;

    proc.set(
        "spawn",
        lua.create_function({
            let context = context.clone();
            move |_, how: Table| spawn(&context, &how)
        })?,
    )?;

    proc.set(
        "run",
        lua.create_function({
            let context = context.clone();
            move |lua, how: Table| {
                let timeout = how.get::<u64>("timeout").unwrap_or(600_000);
                let program = spawn(&context, &how)?;
                if let Ok(input) = how.get::<mlua::LuaString>("input") {
                    let bytes = input.as_bytes().to_vec();
                    if let Ok(mut slot) = program.stdin.lock()
                        && let Some(stdin) = slot.as_mut()
                    {
                        let _ = stdin.write_all(&bytes);
                        let _ = stdin.flush();
                    }
                }
                if let Ok(mut slot) = program.stdin.lock() {
                    let _ = slot.take();
                }

                let code = program.wait(Duration::from_millis(timeout))?;
                let out = program.take(
                    &program.out.clone(),
                    &program.out_left,
                    None,
                    Duration::ZERO,
                )?;
                let err = program.take(
                    &program.err.clone(),
                    &program.err_left,
                    None,
                    Duration::ZERO,
                )?;

                Ok((code, lua.create_string(&out)?, lua.create_string(&err)?))
            }
        })?,
    )?;

    proc.set(
        "beside",
        lua.create_function({
            let context = context.clone();
            move |_, how: Table| {
                let title: String = how.get("title").unwrap_or_else(|_| "job".to_string());
                let line = line_of(&how)?;
                let mut jobs = context.jobs.lock().map_err(|_| closed(&title))?;
                let id = jobs.start(&title, &line, None).map_err(external)?;
                Ok(id.0)
            }
        })?,
    )?;

    proc.set(
        "job_state",
        lua.create_function({
            let context = context.clone();
            move |lua, id: u64| {
                let mut jobs = context.jobs.lock().map_err(|_| closed("a job"))?;
                let _ = jobs.poll();
                let Some(state) = jobs
                    .jobs()
                    .into_iter()
                    .find(|state| state.id == crate::runner::JobId(id))
                else {
                    return Ok(mlua::Value::Nil);
                };
                let table = lua.create_table()?;
                table.set("running", state.is_running())?;
                table.set("log", state.log.to_string_lossy().to_string())?;
                table.set(
                    "outcome",
                    state.outcome.map(|outcome| match outcome {
                        crate::detached::Outcome::Done => "done",
                        crate::detached::Outcome::Failed(_) => "failed",
                        crate::detached::Outcome::Cancelled => "cancelled",
                    }),
                )?;
                Ok(mlua::Value::Table(table))
            }
        })?,
    )?;

    Ok(proc)
}

/// Starts a program and takes it into the list of what this script started.
fn spawn(context: &Arc<Context>, how: &Table) -> mlua::Result<Spawned> {
    context.check()?;

    let (name, mut command) = command_of(how)?;
    command
        .stdin(pipe_of(how, "stdin"))
        .stdout(pipe_of(how, "stdout"))
        .stderr(pipe_of(how, "stderr"));
    if let Ok(cwd) = how.get::<String>("cwd") {
        command.current_dir(cwd);
    }
    if let Ok(environment) = how.get::<Table>("env") {
        for pair in environment.pairs::<String, String>() {
            let (key, value) = pair?;
            command.env(key, value);
        }
    }

    let mut child = command.spawn().map_err(|source| {
        external(ScriptError::Spawn {
            command: name.clone(),
            source,
        })
    })?;

    let stdin = Arc::new(Mutex::new(child.stdin.take()));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let pid = child.id();

    let (id, child) = {
        let mut registry = context.processes.lock().map_err(|_| closed(&name))?;
        registry.adopt(child).map_err(external)?
    };

    let out = ByteSwap::with_capacity(PIPE);
    let err = ByteSwap::with_capacity(PIPE);
    let ended = Arc::new(AtomicBool::new(false));
    let drained = Arc::new(AtomicBool::new(stdout.is_none()));
    let code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

    if let Some(mut pipe) = stdout {
        let out = out.clone();
        let drained = drained.clone();
        spawn_thread("zyt-script-out", move || {
            let mut buffer = vec![0u8; PIPE];
            while let Ok(read) = pipe.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                out.push(&buffer[..read]);
            }
            drained.store(true, Ordering::Release);
            out.wake();
        })
        .map_err(external)?;
    }

    if let Some(mut pipe) = stderr {
        let err = err.clone();
        spawn_thread("zyt-script-err", move || {
            let mut buffer = vec![0u8; PIPE];
            while let Ok(read) = pipe.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                err.push(&buffer[..read]);
            }
            err.wake();
        })
        .map_err(external)?;
    }

    spawn_thread("zyt-script-wait", {
        let child = child.clone();
        let ended = ended.clone();
        let code = code.clone();
        let out = out.clone();
        let err = err.clone();
        move || {
            loop {
                let status = {
                    let Ok(mut guard) = child.lock() else {
                        break;
                    };
                    guard.try_wait()
                };
                match status {
                    Ok(Some(status)) => {
                        if let Ok(mut slot) = code.lock() {
                            *slot = status.code();
                        }
                        break;
                    }
                    Ok(None) => std::thread::sleep(REAP),
                    Err(_) => break,
                }
            }
            ended.store(true, Ordering::Release);
            out.wake();
            err.wake();
        }
    })
    .map_err(external)?;

    Ok(Spawned {
        id,
        name,
        pid,
        stdin,
        out,
        err,
        out_left: Mutex::new(Vec::new()),
        err_left: Mutex::new(Vec::new()),
        code,
        ended,
        drained,
        attached: Arc::new(AtomicBool::new(false)),
        context: context.clone(),
    })
}

/// Hands a program to the line, so the bytes never pass through the script.
fn attach(program: &Spawned, how: Option<Table>) -> mlua::Result<Attached> {
    if program.attached.swap(true, Ordering::AcqRel) {
        return Err(external(ScriptError::MissingPipes));
    }

    let notices = how
        .as_ref()
        .and_then(|how| how.get::<String>("stderr").ok())
        .unwrap_or_else(|| "notice".to_string())
        == "notice";
    let detached = Arc::new(AtomicBool::new(false));

    spawn_thread("zyt-script-to-line", {
        let out = program.out.clone();
        let line: Arc<dyn Line> = program.context.line.clone();
        let ended = program.ended.clone();
        let drained = program.drained.clone();
        let detached = detached.clone();
        move || {
            let mut spare = Vec::new();
            loop {
                out.take_into_wait(&mut spare, FEED_WAIT);
                if !spare.is_empty() {
                    line.write(&spare);
                }
                if detached.load(Ordering::Acquire) {
                    break;
                }
                if ended.load(Ordering::Acquire)
                    && drained.load(Ordering::Acquire)
                    && out.is_empty()
                {
                    break;
                }
            }
        }
    })
    .map_err(external)?;

    spawn_thread("zyt-script-to-program", {
        let line: Arc<dyn Line> = program.context.line.clone();
        let stdin = program.stdin.clone();
        let ended = program.ended.clone();
        let detached = detached.clone();
        let cancel: Arc<Cancel> = program.context.cancel.clone();
        move || {
            let mut spare = Vec::new();
            while !ended.load(Ordering::Acquire)
                && !detached.load(Ordering::Acquire)
                && !cancel.asked()
            {
                line.read(&mut spare, FEED_WAIT);
                if spare.is_empty() {
                    continue;
                }
                let Ok(mut slot) = stdin.lock() else { break };
                let Some(pipe) = slot.as_mut() else { break };
                if pipe.write_all(&spare).and_then(|()| pipe.flush()).is_err() {
                    break;
                }
            }
        }
    })
    .map_err(external)?;

    if notices {
        spawn_thread("zyt-script-notices", {
            let err = program.err.clone();
            let prompt: Arc<dyn Prompt> = program.context.prompt.clone();
            let ended = program.ended.clone();
            let detached = detached.clone();
            move || {
                let mut spare = Vec::new();
                let mut left: Vec<u8> = Vec::new();
                loop {
                    err.take_into_wait(&mut spare, FEED_WAIT);
                    left.append(&mut spare);
                    while let Some(at) = left
                        .iter()
                        .position(|byte| *byte == b'\n' || *byte == b'\r')
                    {
                        let line: Vec<u8> = left.drain(..=at).collect();
                        let said = text_of(&line);
                        if !said.is_empty() {
                            prompt.notice(NoticeKind::Info, said);
                        }
                    }
                    if detached.load(Ordering::Acquire)
                        || (ended.load(Ordering::Acquire) && err.is_empty())
                    {
                        break;
                    }
                }
            }
        })
        .map_err(external)?;
    }

    Ok(Attached {
        out: program.out.clone(),
        ended: program.ended.clone(),
        drained: program.drained.clone(),
        code: program.code.clone(),
        detached,
        context: program.context.clone(),
    })
}

/// The program a script asked for, and what to call it in a message.
///
/// A list of words is run as it stands, which is what keeps a path with a
/// space in it one argument; a line asks for the shell of the platform and is
/// then the script's business to have written safely.
fn command_of(how: &Table) -> mlua::Result<(String, Command)> {
    if let Ok(line) = how.get::<String>("line") {
        if line.trim().is_empty() {
            return Err(external(ScriptError::EmptyCommand));
        }
        return Ok((line.clone(), shell_command(&line)));
    }

    let program: String = how
        .get("command")
        .or_else(|_| how.get::<String>(1))
        .map_err(|_| external(ScriptError::EmptyCommand))?;
    if program.trim().is_empty() {
        return Err(external(ScriptError::EmptyCommand));
    }

    let mut arguments: Vec<String> = how.get("args").unwrap_or_default();
    if arguments.is_empty() {
        let mut at = 2;
        while let Ok(word) = how.get::<String>(at) {
            arguments.push(word);
            at += 1;
        }
    }

    let mut command = Command::new(&program);
    command.args(&arguments);
    own_group(&mut command);

    let named = if arguments.is_empty() {
        program.clone()
    } else {
        format!("{program} {}", arguments.join(" "))
    };
    Ok((named, command))
}

/// The command line of a job beside the line.
fn line_of(how: &Table) -> mlua::Result<String> {
    if let Ok(line) = how.get::<String>("line") {
        return Ok(line);
    }
    let program: String = how
        .get("command")
        .map_err(|_| external(ScriptError::EmptyCommand))?;
    let arguments: Vec<String> = how.get("args").unwrap_or_default();
    let mut words = vec![crate::quote::quote_for_shell(&program)];
    words.extend(
        arguments
            .iter()
            .map(|word| crate::quote::quote_for_shell(word)),
    );
    Ok(words.join(" "))
}

/// Which way a channel of the program goes.
fn pipe_of(how: &Table, which: &str) -> Stdio {
    match how.get::<String>(which).as_deref() {
        Ok("null") => Stdio::null(),
        Ok("inherit") => Stdio::inherit(),
        _ => Stdio::piped(),
    }
}

/// One line of a pipe as text, without what ends it.
fn text_of(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\n', '\r'])
        .to_string()
}

/// The error of a handle whose program is gone.
fn closed(name: &str) -> mlua::Error {
    external(ScriptError::Spawn {
        command: name.to_string(),
        source: std::io::Error::from(std::io::ErrorKind::BrokenPipe),
    })
}
