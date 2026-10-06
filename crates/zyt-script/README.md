# zyt-script

Transfer scripts in Lua: the interpreter, the calls a script is given, and the
runner that drives one with no window anywhere.

## Scope

A script replaces the external transfer program. It is a directory: a
`Manifest.yaml` saying what it is and what it needs, and Lua answering with a
function per direction. It is given the line of the session, a way to ask the
user something, and a way to start programs of its own.

```rust
let library = Library::load(&default_roots("zyterm", data, config));
let entry = library.find("shell-transfer")?;   // one directory, one Manifest.yaml

let channel = LineChannel::new();              // the line, as the script sees it
let mut run = ScriptRun::start(Start { .. }, Direction::Send)?;

channel.feed(&from_the_device);                // the application, once a frame
channel.take_output(&mut spare);
channel.set_pending_output(driver_queue);      // so a script can wait for a free line
run.tick();                                    // walks the ladder of stopping
run.cancel();                                   // the flag, then the children, then the hook
```

| call | what it does |
| --- | --- |
| `Library::load(roots)` | reads the manifest of every script of those directories, running nothing of anybody's Lua; a directory that cannot be used becomes a `Problem` and the rest still load |
| `Manifest::of_directory(id, dir)` | the same for one script |
| `check(entry, roots)` | loads the Lua of one script and says whether it could run at all |
| `ScriptRun::start(start, direction)` | loads the script and runs it on a thread of its own |
| `ScriptRun::tick` | once a frame: walks the ladder of stopping and says whether the run is over |
| `ScriptRun::cancel` / `abandon` | ask it to stop; give up on it and shut its line |
| `LineChannel` | the two directions of the line, with the gate that shuts them |
| `ProcessRegistry` | every process one script started, and how they are all stopped |

### What a script may call

Everything is under one global, `zyt`.

| table | calls |
| --- | --- |
| `zyt.line` | `write`, `write_text`, `type`, `write_key`, `read`, `waiting`, `pending`, `busy`, `wait_drained`, `is_open` |
| `zyt.proc` | `spawn`, `run`, `beside`, `job_state`; a handle has `write`, `read`, `read_err`, `read_line`, `read_err_line`, `close_stdin`, `wait`, `kill`, `running`, `pid`, `attach_to_line` |
| `zyt.term` | `write`, `print` — straight into the terminal, which is the only way a script shows anything while it holds the line |
| `zyt.notice` | `info`, `error` — the frame and colour of the application |
| `zyt.progress` | `share`, `indeterminate`, `error`, `paused`, `clear` |
| `zyt.ui` | `ask(form)`, `confirm(question, detail)` |
| `zyt.fs` | `open`, `stat`, `list`, `mkdir`, `remove`, `rename`, `temp_file`, `absolute`, `url`, `walk` |
| `zyt.codec` | `b64_encode`, `b64_decode`, `b64_slice`, `slice`, `lines_of`, `digest`, `digest_data`, `short_size` |
| `zyt.shell` | `quote`, `quote_posix`, `split`, `join`, `finish_bytes` |
| `zyt.script` | `name`, `direction`, `root`, `read_file`, `list_files`, `fold`, `fill` |
| `zyt.target`, `zyt.vars` | `kind`, `paths`, `path`, `filename`, `stem`, `suffix`; `get`, `require`, `all` |
| `zyt.time` | `now_ms`, `sleep`, `deadline` |
| `zyt.log` | `debug`, `info`, `warn`, `error` |

`zyt.proc.spawn` and `attach_to_line` are what a transfer profile used to be:
the script may read and write the pipes itself, or hand the program to the line
and let four threads move whole chunks, so the bytes of a file never become Lua
strings.

A dialog is described and not drawn: `FieldKind` carries one line of text, text
of several lines, a switch, one of several, one picked from a menu, any number
of a list, and the two decorations. `Form::defaults` is what it answers before
anybody touches it and `Form::check` is what counts as answered, so a window
and a run answered from the command line agree.

`Form::id` is the name a form is known by inside its script, and `Form::unsaved`
says that what it is answered is not to be kept at all. Neither means anything
to this crate: what a form asks is the same either way. They are what an
application that remembers answers needs in order to tell two forms of one
script apart and to leave one of them out.

### Where scripts are looked for

`default_roots` names four directories, nearest to the person first: the
configuration directory, their own data directory, the shared directories of
the system, and the directory the program was shipped in. The first one
carrying a name wins and the others are remembered as shadowed, so what
somebody wrote beats what was installed for them. The platform is asked for its directories — `xdg` on
unix, the known folders on Windows — and the application hands in its own data
and configuration directories, because the identity they are resolved from is
not this crate's business.

### The runner

`zyt-script` is the same engine with no window: the line is the standard
channels of the process (`--line stdio`, which is how `sh-xfer` was driven),
the pipes of a program (`--line pipe --command sh`, which is what an `ssh`
session is like), a pseudo terminal (`--line pty`, which is what a console is
like) or nothing at all.

| command | what it does |
| --- | --- |
| `list` | every script found, where it came from, what it shadows, what was left out |
| `show <name>` | what one script says about itself |
| `check <name> \| --all` | loads a script and says whether it could run at all |
| `run <name>` | `--direction`, `--target`, `--var NAME=VALUE`, `--answer FIELD=VALUE`, `--line`, `--command`, `--cancel-after MS`, `--timeout S` |

`--answer` fills a field of a dialog, so a window is not needed to test one; a
required field nothing answers stops the run instead of waiting.

## Boundaries

- Nothing here draws, and nothing here reads a configuration file. The
  application implements `Line` and `Prompt`; `Progress` is this crate's own
  type and what a bar or a taskbar makes of it is not its business.
- Every call of `Line` and `Prompt` is made from the thread the script runs on.
  An implementation that draws hands the work to the thread that draws and
  waits; `Prompt::ask` blocking is the point of it.
- The script never opens the port. It writes into a buffer and the application
  moves it, which is what keeps one short lock per frame on the data path.
- Nothing a script can call starts a process outside `ProcessRegistry`: that is
  what makes "stop everything it started" a fact rather than a habit. On unix
  each child is a process group and the group is signalled; on Windows they are
  assigned to a job object, which holds a child that detached itself and kills
  what is left when its handle closes.
- `LineChannel::close` is how a run that cannot be stopped is given up on: a
  byte written afterwards reaches nothing, so an abandoned thread cannot write
  into a session that has moved on.

## Stopping one

A thread cannot be taken off a call it is inside, so stopping is a ladder, and
`ScriptRun::tick` walks it:

```
cancel ─► the flag: every blocking call of the host refuses
       ─► the processes it started, by group, newest first
       ─► cleanup(), with a budget of CLEANUP_BUDGET
       ─► the hook of the interpreter raises, and keeps raising
       ─► past ABANDON_AFTER the thread is left and its line is shut
```

The hook is what catches a script that asks the host nothing, and it keeps
raising because a `pcall` would otherwise swallow the one refusal. `debug` is
never loaded, so it cannot be taken off; `os.execute`, `os.exit` and
`io.popen` are taken out, so nothing starts a process this crate does not know
about.

## Errors

`Result<T, ScriptError>`, one variant per failure case: `EmptyCommand`,
`Spawn`, `MissingPipes`, `ThreadStart`, `Kill`, `Group`, `Log`, `Finished`,
`InvalidFinishKey`, `TargetMismatch`, `UnsetVariable`, `LineClosed`,
`Cancelled`, `NotFound`, `Duplicate`, `Read`, `Load`, `Manifest`,
`NoDirection`, `Runtime`, `Abandoned`, `NoPrompt`, `Timeout`, `Walk`, `Loop`,
`Decode`, `Io`. Errors of the interpreter and of the platform are wrapped with
`#[source]` and never turned into text.
