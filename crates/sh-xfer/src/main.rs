//! Files transferred over SHell, from the command line.
//!
//! The standard input and the standard output are the line to the device; the
//! standard error is what the user reads, so everything said to the user —
//! the list of what will travel, the progress, the complaint — goes there.

mod exec;

use clap::{Parser, Subcommand, ValueEnum};
use sh_xfer::{Digest, Mode, Notes, Reader, Report, Session, ShXferError};
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Files transferred over SHell: a file onto a device that runs nothing but a
/// shell, and back.
#[derive(Debug, Parser)]
#[command(name = "sh-xfer", version, about, long_about = None)]
struct Arguments {
    /// What to do.
    #[command(subcommand)]
    command: Command,

    /// Directory of this machine the files are written into or read from.
    #[arg(short = 'C', long, global = true)]
    directory: Option<PathBuf>,

    /// Directory of the device the files are written into or read from.
    #[arg(short = 'r', long, global = true)]
    remote_directory: Option<String>,

    /// Which set of scripts the device is sent, and with it how the body of a
    /// file travels over the line.
    ///
    /// base64 by default: it crosses a console whatever its settings, while
    /// the raw bytes need the line switched to binary and get there only if
    /// nothing between the two ends touches them.
    #[arg(long, global = true, value_enum, default_value_t = ModeArgument::Base64)]
    mode: ModeArgument,

    /// How the two sides say a file arrived whole.
    ///
    /// `none` compares nothing. `auto` asks the device for each sum in turn,
    /// strongest first, and carries on without a check when it has none. A sum
    /// named outright is asked for by its own probe, and a device that has not
    /// got it is refused rather than carried to unchecked.
    #[arg(long, global = true, value_enum, default_value_t = DigestArgument::None)]
    digest: DigestArgument,

    /// How many bytes of the line one chunk of a file may take.
    ///
    /// It is the size on the wire, not in the file: as base64 a chunk carries
    /// three bytes for every four it costs, and the slice of the file is cut
    /// to whole lines of base64. A small chunk says where a transfer stopped
    /// more often and costs a round trip more often for it.
    ///
    /// Unsaid, it is what the line asks for: a console carries the bytes one
    /// after another and takes small chunks, while a pipe is a shell somewhere
    /// else and every chunk of it costs a round trip, so it takes large ones.
    #[arg(long, global = true, value_name = "BYTES")]
    chunk_size: Option<usize>,

    /// Whether the device is asked how long a file is once it has been
    /// written.
    ///
    /// It is one command at the end of a file, and it is what says that every
    /// chunk landed. Off believes the file as it was written, and then only a
    /// sum has anything to say about it.
    #[arg(long, global = true, value_enum, default_value_t = SizeCheckArgument::On, value_name = "ON|OFF")]
    size_check: SizeCheckArgument,

    /// How many seconds to wait for the device before giving up.
    #[arg(long, global = true, default_value_t = 30)]
    timeout: u64,

    /// Take the whole directory.
    ///
    /// `get` carries what it is named. With this it carries everything the
    /// directory of the device holds, and then it may be named nothing.
    #[arg(long, global = true)]
    all: bool,

    /// Say nothing but what went wrong.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Say what is being done besides how far along it is.
    ///
    /// Without it a transfer says one line per chunk and nothing else, which
    /// is what someone watching a line wants to see. With it, every command
    /// the device is sent is written out as well, and so is what was settled
    /// before anything travelled: the mode, the sum, the size of a chunk and
    /// the files that are about to go.
    #[arg(short, long, global = true)]
    verbose: bool,
}

/// Which set of scripts the device is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ModeArgument {
    /// base64, which any line carries.
    Base64,
    /// The bytes themselves, with the line switched to binary.
    Raw,
}

impl From<ModeArgument> for Mode {
    fn from(argument: ModeArgument) -> Self {
        match argument {
            ModeArgument::Base64 => Self::Base64,
            ModeArgument::Raw => Self::Raw,
        }
    }
}

/// Whether a file that was written is measured afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SizeCheckArgument {
    /// The device is asked how long the file is.
    On,
    /// Nothing is asked.
    Off,
}

impl SizeCheckArgument {
    /// Whether the check is asked for.
    fn wanted(self) -> bool {
        self == Self::On
    }
}

/// Which sum the two sides compare, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DigestArgument {
    /// Compare nothing.
    None,
    /// Whichever of them the device has, strongest first.
    Auto,
    /// What `md5sum` says.
    Md5sum,
    /// What `sha1sum` says.
    Sha1sum,
    /// What `sha256sum` says.
    Sha256sum,
}

impl DigestArgument {
    /// The sum this names, when it names one.
    fn digest(self) -> Option<Digest> {
        match self {
            Self::None | Self::Auto => None,
            Self::Md5sum => Some(Digest::Md5),
            Self::Sha1sum => Some(Digest::Sha1),
            Self::Sha256sum => Some(Digest::Sha256),
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Copy from the device to this machine.
    ///
    /// Without a path the device is asked what is in the directory it stands
    /// in, and everything there is copied.
    Get {
        /// Files and directories of the device.
        paths: Vec<String>,
    },
    /// Show what the device has in a directory, and carry nothing.
    ///
    /// It needs no terminal and no transfer: it is what answers the question
    /// of what the far end actually says when a listing comes back empty.
    List {
        /// Directory of the device, its own by default.
        path: Option<String>,
    },
    /// Copy from this machine to the device.
    Put {
        /// Files and directories of this machine.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Ask the device where it stands, then run a program of this machine
    /// with that directory in its arguments.
    ///
    /// `{}` in any argument becomes the directory, wherever it stands in one,
    /// so `user@host:{}` is a destination of its own. The program gets no
    /// standard input and everything it says goes to the standard error: the
    /// standard output of this process is the line to the device.
    ///
    ///     sh-xfer pwd-exec scp -- -v -O ./one.bin root@host:{}
    PwdExec {
        /// Program of this machine to run.
        program: String,
        /// Arguments of that program, `{}` standing for the directory.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
}

fn main() -> std::process::ExitCode {
    env_logger::init();
    let arguments = Arguments::parse();
    let quiet = arguments.quiet;

    let mut notes = Notes::new(std::io::stderr(), quiet);
    if let Command::PwdExec { .. } = &arguments.command {
        return match pwd_exec(&arguments, &mut notes) {
            Ok(code) => ended(code),
            Err(error) => complain(&error),
        };
    }

    match run(arguments, &mut notes) {
        Ok(count) => {
            notes.note(&format!("{count} file(s) carried"));
            std::process::ExitCode::SUCCESS
        }
        Err(error) => complain(&error),
    }
}

/// Says what went wrong, whether or not the user asked for quiet: a failure is
/// the one thing nobody asked to be spared.
fn complain(error: &ShXferError) -> std::process::ExitCode {
    let mut complaint = Notes::new(std::io::stderr(), false);
    complaint.note(&describe(error));
    std::process::ExitCode::FAILURE
}

/// The code a program of this machine ended on, as the code of this one.
///
/// A program ended by a signal never had a code of its own, and neither did
/// one whose code does not fit in the byte the system carries, so both count
/// as a failure rather than as some other number.
fn ended(code: Option<i32>) -> std::process::ExitCode {
    match code.and_then(|code| u8::try_from(code).ok()) {
        Some(code) => std::process::ExitCode::from(code),
        None => std::process::ExitCode::FAILURE,
    }
}

/// Asks the device where it stands and runs the program there.
fn pwd_exec<W: Write>(arguments: &Arguments, notes: &mut Notes<W>) -> sh_xfer::Result<Option<i32>> {
    let Command::PwdExec {
        program,
        arguments: rest,
    } = &arguments.command
    else {
        return Ok(None);
    };

    let directory = exec::working_directory(arguments.timeout, arguments.mode.into())?;
    let expanded = exec::expand(rest, &directory);
    notes.note(&format!("the device stands in {directory}"));
    if arguments.verbose {
        notes.note(&format!("running {program} {}", expanded.join(" ")));
    }

    exec::run(program, &expanded).map_err(|source| ShXferError::NoProgram {
        program: program.clone(),
        source,
    })
}

/// How large a chunk is when nothing says.
///
/// A console carries the bytes one after another, and the round trip that ends
/// a chunk costs next to nothing beside them; a pipe is a shell somewhere else,
/// and the round trip is the whole cost. The line says which it is.
fn chunk_for_the_line() -> usize {
    match std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        true => sh_xfer::DEFAULT_CHUNK,
        false => sh_xfer::DEFAULT_PIPE_CHUNK,
    }
}

fn run<W: Write>(arguments: Arguments, progress: &mut Notes<W>) -> sh_xfer::Result<usize> {
    let chunk_size = arguments.chunk_size.unwrap_or_else(chunk_for_the_line);
    let reader = Reader::new(std::io::stdin(), Duration::from_secs(arguments.timeout));
    let mut session = Session::new(reader, std::io::stdout(), arguments.mode.into());
    let talk = Rc::new(RefCell::new(Talk::new(arguments.quiet, arguments.verbose)));
    session.listen(Box::new(talk.clone()));
    session.set_chunk_size(chunk_size);
    session.set_size_check(arguments.size_check.wanted());

    session.hello()?;
    let digest = settle_digest(arguments.digest, &mut session)?;
    if arguments.verbose {
        progress.note(&format!(
            "the line carries files as {}",
            session.mode().name()
        ));
        match digest {
            Some(digest) => progress.note(&format!("checked with {}", digest.program())),
            None if arguments.digest == DigestArgument::Auto => {
                progress.note("the device has nothing to check the files with")
            }
            None => {}
        }
        progress.note(&format!(
            "{} bytes of the line to a chunk, {} of a file",
            chunk_size,
            session.slice()
        ));
        if arguments.size_check.wanted() {
            progress.note("a file that was written is measured on the device afterwards");
        }
    }

    match &arguments.command {
        Command::Get { paths } => get(&mut session, &arguments, paths, digest, progress, &talk),
        Command::Put { paths } => put(&mut session, &arguments, paths, digest, progress, &talk),
        Command::List { path } => list(&mut session, &arguments, path.as_deref(), progress),
        Command::PwdExec { .. } => {
            unreachable!("pwd-exec opens no transfer and is run before this")
        }
    }
}

/// What a transfer says about itself while it happens.
///
/// One line per command, so nothing the device is asked is a secret, and one
/// line per chunk, which says where in the file it stands and ends in `OK`,
/// the share of the whole transfer and how long that chunk took — all three
/// once the next chunk has been worked out from the bytes that really moved.
///
/// The time is what says whether a line is slow or a device is: a share
/// creeping up says nothing about which chunk cost what.
///
/// The commands themselves are said only when they are asked for: there are
/// thousands of them in a transfer, and what the user wants to see is how far
/// along it is.
struct Talk {
    out: std::io::Stderr,
    quiet: bool,
    commands: bool,
    direction: &'static str,
    number: usize,
    files: usize,
    name: String,
    total: u64,
    done: u64,
    started: Option<std::time::Instant>,
}

impl Talk {
    /// Says nothing at all when `quiet`, and says the commands when asked to.
    fn new(quiet: bool, commands: bool) -> Self {
        Self {
            out: std::io::stderr(),
            quiet,
            commands,
            direction: "send",
            number: 0,
            files: 0,
            name: String::new(),
            total: 0,
            done: 0,
            started: None,
        }
    }

    /// How much is about to travel altogether, before any of it does.
    fn carrying(&mut self, direction: &'static str, files: usize, total: u64) {
        self.direction = direction;
        self.files = files;
        self.total = total;
        self.done = 0;
        self.number = 0;
    }

    /// The file the chunks from here on belong to.
    fn file(&mut self, name: &str) {
        self.number += 1;
        self.name = name.to_string();
    }

    /// How much of everything that is travelling has travelled, as a share of
    /// a hundred. It is the whole of the transfer, not the file at hand: a
    /// share that went back to nothing with every new file would say less.
    fn share_of_the_whole(&self) -> u64 {
        match self.total {
            0 => 100,
            total => self.done.min(total) * 100 / total,
        }
    }

    fn say(&mut self, text: &str) {
        if self.quiet {
            return;
        }
        let _ = write!(self.out, "{text}");
        let _ = self.out.flush();
    }
}

impl Report for Talk {
    fn command(&mut self, script: &str) {
        if !self.commands {
            return;
        }
        self.say(&format!("$ {script}\n"));
    }

    fn chunk(&mut self, start: u64, end: u64) {
        self.started = Some(std::time::Instant::now());
        self.say(&format!(
            "[{}/{}] {} {} {start}:{end} size {}... ",
            self.number,
            self.files,
            self.direction,
            self.name,
            end - start
        ));
    }

    fn carried(&mut self, bytes: u64) {
        self.done += bytes;
        let Some(started) = self.started.take() else {
            return;
        };
        let share = self.share_of_the_whole();
        let took = started.elapsed().as_secs_f64();
        self.say(&format!("OK {share}% {took:.2}s\n"));
    }
}

/// Which sum says a file arrived whole, once the device has been asked.
///
/// Each sum has a probe of its own. `auto` tries them strongest first and
/// carries on unchecked when the device has none; one named outright is
/// refused when the device has it not, because naming it is asking for it.
fn settle_digest<W: Write>(
    asked: DigestArgument,
    session: &mut Session<W>,
) -> sh_xfer::Result<Option<Digest>> {
    if asked == DigestArgument::None {
        return Ok(None);
    }
    if let Some(named) = asked.digest() {
        return match session.offers_digest(named)? {
            true => Ok(Some(named)),
            false => Err(ShXferError::MissingCommand {
                command: named.program().to_string(),
            }),
        };
    }
    for digest in Digest::every() {
        if session.offers_digest(digest)? {
            return Ok(Some(digest));
        }
    }
    Ok(None)
}

/// Asks both sides what they make of a file that has just travelled.
fn verify<W: Write>(
    session: &mut Session<W>,
    digest: Option<Digest>,
    there: &str,
    here: &std::path::Path,
) -> sh_xfer::Result<()> {
    let Some(digest) = digest else {
        return Ok(());
    };

    let theirs = session.digest(digest, there)?;
    let ours = digest.of_file(here)?;
    let program = digest.program();
    let verdict = match ours == theirs {
        true => "whole",
        false => "mismatch",
    };
    log::info!(
        "{program}: {there} -> {here} device {theirs} here {ours} {verdict}",
        here = here.display()
    );
    if ours != theirs {
        return Err(ShXferError::Mismatch {
            path: here.display().to_string(),
            ours,
            theirs,
        });
    }
    Ok(())
}

/// Copies from the device to this machine.
fn get<W: Write, P: Write>(
    session: &mut Session<W>,
    arguments: &Arguments,
    paths: &[String],
    digest: Option<Digest>,
    progress: &mut Notes<P>,
    talk: &Rc<RefCell<Talk>>,
) -> sh_xfer::Result<usize> {
    let roots = match (paths.is_empty(), arguments.all) {
        (false, _) => paths.to_vec(),
        (true, true) => vec![where_to_look(arguments, None)],
        (true, false) => return Err(ShXferError::NothingToTake),
    };

    let items = sh_xfer::remote_items(session, &roots)?;

    let into = arguments.directory.clone().unwrap_or_default();
    let total: u64 = items.iter().map(|item| item.size).sum();
    announce(progress, arguments.verbose, &items_named(&items), total);
    talk.borrow_mut().carrying("receive", items.len(), total);

    let mut arrived = Vec::with_capacity(items.len());
    for item in &items {
        talk.borrow_mut().file(&item.relative);
        let target = under(&into, &item.relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ShXferError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut file = std::fs::File::create(&target).map_err(|source| ShXferError::Io {
            path: target.clone(),
            source,
        })?;
        session.retrieve(&item.path, &mut file)?;
        drop(file);
        verify(session, digest, &item.path, &target)?;
        arrived.push(target);
    }

    saved(progress, &arrived);
    Ok(items.len())
}

/// Names every file that arrived, each a link to where it lies on this
/// machine.
///
/// A transfer is over when the last file has landed, and what somebody wants
/// then is the files themselves, so the whole list is said at the end with
/// every name written as a hyperlink to the path it was saved at. A terminal
/// that draws links opens the file from the line that says it arrived.
///
/// This is the one place this program writes an escape sequence, and it writes
/// it once per file and not once per line of a transfer: a program collecting
/// the lines sees the name with the sequence around it, which is a price worth
/// paying at the end of a transfer and not in the middle of one.
fn saved<W: Write>(progress: &mut Notes<W>, files: &[PathBuf]) {
    for path in files {
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string();
        progress.note(&format!("  {}", link(path, &name)));
    }
}

/// One name as the hyperlink a terminal reads: `OSC 8`, the address, the text,
/// and `OSC 8` again with nothing in it to end the link.
fn link(path: &Path, text: &str) -> String {
    format!("\x1b]8;;{}\x1b\\{text}\x1b]8;;\x1b\\", file_url(path))
}

/// A path of this machine as a `file://` address.
///
/// The path is made absolute without asking the file system anything, the
/// separators of the platform become the one a URL uses, and every byte a URL
/// may not carry is written as `%XX`. The drive of a windows path keeps its
/// colon and gains the slash that `file:///C:/…` wants.
fn file_url(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy().replace('\\', "/");

    let mut url = String::from("file://");
    if !text.starts_with('/') {
        url.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                url.push(byte as char)
            }
            other => url.push_str(&format!("%{other:02X}")),
        }
    }
    url
}

/// Shows what the device has, and carries nothing.
fn list<W: Write, P: Write>(
    session: &mut Session<W>,
    arguments: &Arguments,
    path: Option<&str>,
    progress: &mut Notes<P>,
) -> sh_xfer::Result<usize> {
    let here = where_to_look(arguments, path);
    let entries = session.list(&here)?;

    progress.note(&format!("{here}:"));
    for entry in &entries {
        let mark = match entry.kind {
            sh_xfer::EntryKind::Directory => "d",
            sh_xfer::EntryKind::File => "-",
            sh_xfer::EntryKind::Other => "?",
        };
        let size = match entry.kind {
            sh_xfer::EntryKind::File => sh_xfer::short_size(entry.size),
            _ => String::new(),
        };
        progress.note(&format!("{mark} {size:>8} {}", entry.name));
    }
    if entries.is_empty() {
        progress.note("nothing there");
    }
    Ok(0)
}

/// The directory of the device a command means.
fn where_to_look(arguments: &Arguments, path: Option<&str>) -> String {
    path.map(str::to_string)
        .or_else(|| arguments.remote_directory.clone())
        .unwrap_or_else(|| ".".to_string())
}

/// Copies from this machine to the device.
fn put<W: Write, P: Write>(
    session: &mut Session<W>,
    arguments: &Arguments,
    paths: &[PathBuf],
    digest: Option<Digest>,
    progress: &mut Notes<P>,
    talk: &Rc<RefCell<Talk>>,
) -> sh_xfer::Result<usize> {
    let roots: Vec<PathBuf> = paths
        .iter()
        .map(|path| match &arguments.directory {
            Some(directory) if path.is_relative() => directory.join(path),
            _ => path.clone(),
        })
        .collect();

    let items = sh_xfer::local_items(&roots)?;

    let into = arguments.remote_directory.clone().unwrap_or_default();
    let total: u64 = items.iter().map(|item| item.size).sum();
    announce(progress, arguments.verbose, &items_named(&items), total);
    talk.borrow_mut().carrying("send", items.len(), total);

    for item in &items {
        talk.borrow_mut().file(&item.relative);
        let target = remote_under(&into, &item.relative);
        if let Some(parent) = target.rsplit_once('/') {
            session.make_directory(parent.0)?;
        }
        let mut file = std::fs::File::open(&item.path).map_err(|source| ShXferError::Io {
            path: item.path.clone(),
            source,
        })?;
        session.store(&target, &mut file)?;
        verify(session, digest, &target, &item.path)?;
    }

    Ok(items.len())
}

/// Says what is about to travel, before any of it does.
///
/// The sizes are gathered whether or not they are said: a chunk is cut from a
/// size, and how far along a transfer is, is a share of the whole of them.
fn announce<W: Write>(progress: &mut Notes<W>, verbose: bool, names: &[&str], total: u64) {
    if !verbose {
        return;
    }
    for name in names {
        progress.note(&format!("  {name}"));
    }
    progress.note(&format!("{} file(s), {total} byte(s)", names.len()));
}

/// The names of what is about to travel, in the order it will.
fn items_named<T: Named>(items: &[T]) -> Vec<&str> {
    items.iter().map(Named::name).collect()
}

/// Something with a path relative to the root it was named under.
trait Named {
    /// That path, as text.
    fn name(&self) -> &str;
}

impl Named for sh_xfer::LocalItem {
    fn name(&self) -> &str {
        &self.relative
    }
}

impl Named for sh_xfer::RemoteItem {
    fn name(&self) -> &str {
        &self.relative
    }
}

/// A path under a directory of this machine, the parts read the way the device
/// writes them.
fn under(directory: &std::path::Path, relative: &str) -> PathBuf {
    let mut path = directory.to_path_buf();
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        path.push(part);
    }
    path
}

/// A path under a directory of the device.
fn remote_under(directory: &str, relative: &str) -> String {
    if directory.is_empty() {
        relative.to_string()
    } else {
        format!("{}/{relative}", directory.trim_end_matches('/'))
    }
}

/// What went wrong, and its causes, in one line each.
fn describe(error: &ShXferError) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_a_link_to_the_file_it_names() {
        let line = link(Path::new("/srv/firmware.bin"), "firmware.bin");

        assert_eq!(
            line,
            "\x1b]8;;file:///srv/firmware.bin\x1b\\firmware.bin\x1b]8;;\x1b\\"
        );
    }

    #[test]
    fn what_an_address_may_not_carry_is_written_out() {
        assert_eq!(
            file_url(Path::new("/srv/one two#three.bin")),
            "file:///srv/one%20two%23three.bin"
        );
        assert_eq!(
            file_url(Path::new("/srv/отчёт.txt")),
            "file:///srv/%D0%BE%D1%82%D1%87%D1%91%D1%82.txt"
        );
    }

    #[test]
    fn a_relative_path_is_made_absolute_without_asking_the_file_system() {
        let here = std::env::current_dir().expect("this process stands somewhere");
        let url = file_url(Path::new("notes.txt"));

        assert!(url.starts_with("file:///"), "{url}");
        assert!(url.ends_with("/notes.txt"), "{url}");
        assert!(
            url.contains(&file_url(&here)["file://".len()..]),
            "{url} is not under {}",
            here.display()
        );
    }
}
