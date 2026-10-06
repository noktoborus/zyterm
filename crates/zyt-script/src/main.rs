//! The runner: a script driven with no window anywhere.
//!
//! It is how a script is written and checked. The same engine, the same host
//! calls and the same files as in the application; the line is the standard
//! channels of this process, the pipes of a program or a pseudo terminal, and
//! the dialog is answered from the command line. A script that works here
//! works in the window, and a script that fails says where in a terminal
//! rather than in a log.

use clap::{Parser, Subcommand, ValueEnum};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_script::cli::line::{Piped, Quiet};
use zyt_script::cli::prompt::Console;
use zyt_script::{
    Cancel, Direction, Library, Line, Outcome, ScriptError, ScriptRun, Start, TargetKind, Targets,
};

/// The name the directories of the application are looked for under.
const APP: &str = "zyterm";

/// Driving a transfer script without the window.
#[derive(Debug, Parser)]
#[command(name = "zyt-script", version, about, long_about = None)]
struct Arguments {
    /// Directory of scripts to search before the usual ones; may be repeated.
    #[arg(long, value_name = "DIR", global = true)]
    path: Vec<PathBuf>,

    /// Search only the directories named with --path.
    #[arg(long, global = true)]
    no_default_paths: bool,

    /// Name the directories of the application are looked for under.
    #[arg(long, value_name = "NAME", default_value = APP, global = true)]
    app: String,

    /// Say nothing but what was asked for.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// What to do.
    #[command(subcommand)]
    what: What,
}

/// What the runner was asked to do.
#[derive(Debug, Subcommand)]
enum What {
    /// Every script that was found, where it came from and what it shadows.
    List,
    /// What one script says about itself.
    Show {
        /// Name of the script.
        name: String,
    },
    /// Loads a script and says whether it could be run at all.
    Check {
        /// Name of the script, or nothing with --all.
        name: Option<String>,
        /// Check every script that was found.
        #[arg(long)]
        all: bool,
    },
    /// Runs a script.
    Run {
        /// Name of the script.
        name: String,
        /// Which way it goes.
        #[arg(long, value_enum, default_value_t = Way::Send)]
        direction: Way,
        /// What it carries; may be repeated.
        #[arg(long, value_name = "PATH")]
        target: Vec<PathBuf>,
        /// What kind of paths those are, when it is not what the script says.
        #[arg(long, value_name = "KIND")]
        target_kind: Option<String>,
        /// A value of the source, as NAME=VALUE; may be repeated.
        #[arg(long = "var", value_name = "NAME=VALUE")]
        variables: Vec<String>,
        /// An answer to a field of a dialog, as FIELD=VALUE; may be repeated.
        #[arg(long = "answer", value_name = "FIELD=VALUE")]
        answers: Vec<String>,
        /// What the line is.
        #[arg(long, value_enum, default_value_t = Which::Stdio)]
        line: Which,
        /// The program the line runs, for a line of pipes or a console.
        #[arg(long, value_name = "COMMAND")]
        command: Option<String>,
        /// Ask it to stop after this many milliseconds.
        #[arg(long, value_name = "MS")]
        cancel_after: Option<u64>,
        /// Give up on the whole run after this many seconds.
        #[arg(long, value_name = "SECONDS", default_value_t = 600)]
        timeout: u64,
    },
}

/// Which way a run goes.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Way {
    /// A file goes to the device.
    Send,
    /// A file comes from it.
    Receive,
}

impl From<Way> for Direction {
    fn from(way: Way) -> Self {
        match way {
            Way::Send => Self::Send,
            Way::Receive => Self::Receive,
        }
    }
}

/// What stands at the far end of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Which {
    /// The standard channels of this process, which is a pipe with no shell
    /// of its own on either end.
    Stdio,
    /// The pipes of a program, which is what an `ssh` session is like.
    Pipe,
    /// A pseudo terminal running a program, which is what a console is like.
    Pty,
    /// Nothing: the line carries nothing and answers nothing.
    Null,
}

fn main() {
    let arguments = Arguments::parse();
    if let Err(error) = run(&arguments) {
        let mut err = std::io::stderr();
        let _ = writeln!(err, "zyt-script: {error}");
        complain(&error, arguments.quiet);
        std::process::exit(1);
    }
}

/// Says what stands behind an error, for whoever is writing the script.
///
/// A script that failed carries the walk of the interpreter, which is where
/// the line that raised the complaint stands; the first line of it is already
/// the message, so it is left out. Anything else carries a chain of causes
/// instead, and that chain is the whole of what it knows.
fn complain(error: &ScriptError, quiet: bool) {
    if quiet {
        return;
    }
    let mut err = std::io::stderr();

    if let ScriptError::Runtime { source, .. } = error {
        for line in source.to_string().lines().skip(1) {
            let _ = writeln!(err, "  {}", line.trim_end());
        }
        return;
    }

    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(next) = cause {
        let _ = writeln!(err, "  {next}");
        cause = next.source();
    }
}

/// Does what the command line asked for.
fn run(arguments: &Arguments) -> zyt_script::Result<()> {
    let roots = roots(arguments);
    let library = Library::load(&roots);

    match &arguments.what {
        What::List => list(&library, &roots),
        What::Show { name } => show(&library, name),
        What::Check { name, all } => check(&library, name.as_deref(), *all),
        What::Run { .. } => start(arguments, &library, &roots),
    }
}

/// The directories to search, in the order they are searched.
fn roots(arguments: &Arguments) -> Vec<PathBuf> {
    let mut roots = arguments.path.clone();
    if !arguments.no_default_paths {
        let (data, config) = zyt_script::user_dirs(&arguments.app);
        roots.extend(zyt_script::default_roots(
            &arguments.app,
            data.as_deref(),
            config.as_deref(),
        ));
    }
    roots
}

/// Writes every script that was found.
fn list(library: &Library, roots: &[PathBuf]) -> zyt_script::Result<()> {
    let mut out = std::io::stdout();
    for root in roots {
        let _ = writeln!(out, "searched {}", root.display());
    }
    for entry in library.entries() {
        let directions: Vec<&str> = [Direction::Send, Direction::Receive]
            .into_iter()
            .filter(|way| entry.manifest.offers(*way))
            .map(Direction::name)
            .collect();
        let _ = writeln!(
            out,
            "{:<20} {:<20} {:<18} {} {}",
            entry.id,
            entry.name(),
            directions.join(","),
            if entry.manifest.hold_line {
                "on the line"
            } else {
                "beside it  "
            },
            entry.directory.display()
        );
        for shadowed in &entry.shadowed {
            let _ = writeln!(out, "{:<20} stands in for {}", "", shadowed.display());
        }
    }
    for problem in library.problems() {
        let _ = writeln!(
            out,
            "{:<24} left out: {}",
            problem
                .path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            problem.said
        );
    }
    Ok(())
}

/// Writes what one script says about itself.
fn show(library: &Library, name: &str) -> zyt_script::Result<()> {
    let entry = library.find(name)?;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "name       {}", entry.name());
    let _ = writeln!(out, "directory  {}", entry.directory.display());
    let _ = writeln!(out, "manifest   {}", entry.manifest_path().display());
    let _ = writeln!(out, "entry      {}", entry.manifest.entry);
    let _ = writeln!(out, "found in   {}", entry.root.display());
    let _ = writeln!(out, "order      {}", entry.manifest.order);
    let _ = writeln!(
        out,
        "line       {}",
        if entry.manifest.hold_line {
            "held while it runs"
        } else {
            "not touched"
        }
    );
    if !entry.manifest.variables.is_empty() {
        let _ = writeln!(out, "asks for   {}", entry.manifest.variables.join(", "));
    }
    for way in [Direction::Send, Direction::Receive] {
        if let Some(spec) = entry.manifest.direction(way) {
            let _ = writeln!(
                out,
                "{:<10} takes {}, ends with {}",
                way.name(),
                spec.target.name(),
                if spec.finish.is_empty() {
                    "nothing"
                } else {
                    spec.finish.as_str()
                }
            );
        }
    }
    for shadowed in &entry.shadowed {
        let _ = writeln!(out, "stands for {}", shadowed.display());
    }
    Ok(())
}

/// Loads a script, or every one of them, and says nothing when all is well.
fn check(library: &Library, name: Option<&str>, all: bool) -> zyt_script::Result<()> {
    if all {
        let mut out = std::io::stdout();
        for entry in library.entries() {
            zyt_script::check(entry, library.roots())?;
            let _ = writeln!(out, "{:<20} ok", entry.id);
        }
        if let Some(problem) = library.problems().first() {
            return Err(ScriptError::Manifest {
                name: problem
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                what: problem.said.clone(),
            });
        }
        return Ok(());
    }

    let name = name.ok_or_else(|| ScriptError::NotFound {
        name: String::from("a script, or --all"),
    })?;
    zyt_script::check(library.find(name)?, library.roots())
}

/// Runs the script the command line named.
fn start(arguments: &Arguments, library: &Library, roots: &[PathBuf]) -> zyt_script::Result<()> {
    let What::Run {
        name,
        direction,
        target,
        target_kind,
        variables,
        answers,
        line,
        command,
        cancel_after,
        timeout,
    } = &arguments.what
    else {
        return Ok(());
    };

    let entry = library.find(name)?;
    let direction = Direction::from(*direction);
    let kind = match target_kind {
        Some(named) => TargetKind::of_name(named).ok_or_else(|| ScriptError::Manifest {
            name: name.clone(),
            what: format!("{named} is no kind of path"),
        })?,
        None => entry.manifest.target_kind(direction),
    };

    let cancel = Cancel::new();
    let line: Arc<dyn Line> = match line {
        Which::Stdio => Piped::new(std::io::stdin(), std::io::stdout())?,
        Which::Pipe => piped(command.as_deref())?,
        Which::Pty => console(command.as_deref())?,
        Which::Null => Arc::new(Quiet),
    };
    let prompt = Arc::new(Console::new(pairs(answers), arguments.quiet));

    let begin = Start::of_entry(
        entry,
        roots,
        direction,
        Targets {
            kind,
            paths: target.clone(),
        },
        pairs(variables),
        zyt_script::Talking {
            line,
            prompt,
            cancel: cancel.clone(),
        },
    );

    let mut run = ScriptRun::start(begin, direction)?;
    let deadline = Instant::now() + Duration::from_secs(*timeout);
    let stop_at = cancel_after.map(|after| Instant::now() + Duration::from_millis(after));

    while !run.tick() {
        if let Some(stop_at) = stop_at
            && Instant::now() >= stop_at
            && !cancel.asked()
        {
            run.cancel();
        }
        if Instant::now() >= deadline {
            run.cancel();
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    if let Some(error) = run.take_failure() {
        return Err(error);
    }
    match run.outcome() {
        Some(Outcome::Done) | None => Ok(()),
        Some(Outcome::Cancelled) => Err(ScriptError::Cancelled),
        Some(Outcome::Failed(code)) => Err(ScriptError::Runtime {
            name: entry.id.clone(),
            said: format!("it ended with {code:?}"),
            source: mlua::Error::external(std::io::Error::other(format!(
                "the script ended with {code:?}"
            ))),
        }),
    }
}

/// A line that is the pipes of a program.
fn piped(command: Option<&str>) -> zyt_script::Result<Arc<Piped>> {
    let line = command.ok_or(ScriptError::EmptyCommand)?;
    let mut child = zyt_script::shell_command(line)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|source| ScriptError::Spawn {
            command: line.to_string(),
            source,
        })?;

    let reader = child.stdout.take().ok_or(ScriptError::MissingPipes)?;
    let writer = child.stdin.take().ok_or(ScriptError::MissingPipes)?;
    Piped::new(reader, writer)
}

/// A line that is a program under a pseudo terminal.
///
/// It is started where the runner was started. A transfer works below the
/// directory the device stands in, and a pseudo terminal left to itself stands
/// in the home directory of the user: a run from a directory of test files
/// would carry them somewhere nobody was looking.
fn console(command: Option<&str>) -> zyt_script::Result<Arc<Piped>> {
    let line = command.unwrap_or("sh");
    let system = portable_pty::native_pty_system();
    let pair = system
        .openpty(portable_pty::PtySize::default())
        .map_err(|error| ScriptError::Spawn {
            command: line.to_string(),
            source: std::io::Error::other(error),
        })?;

    let (program, flag) = zyt_script::shell();
    let mut builder = portable_pty::CommandBuilder::new(program);
    builder.arg(flag);
    builder.arg(line);
    if let Ok(here) = std::env::current_dir() {
        builder.cwd(here);
    }
    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|error| ScriptError::Spawn {
            command: line.to_string(),
            source: std::io::Error::other(error),
        })?;
    std::mem::forget(child);
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| ScriptError::Spawn {
            command: line.to_string(),
            source: std::io::Error::other(error),
        })?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| ScriptError::Spawn {
            command: line.to_string(),
            source: std::io::Error::other(error),
        })?;
    std::mem::forget(pair.master);
    Piped::new(reader, writer)
}

/// The pairs of `NAME=VALUE` a repeated argument carries.
fn pairs(given: &[String]) -> BTreeMap<String, String> {
    given
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}
