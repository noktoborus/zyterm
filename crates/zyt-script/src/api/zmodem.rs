//! ZMODEM, spoken by this crate rather than by a program beside the line.
//!
//! `zmodem2` is the protocol: a state machine that reads and writes nothing
//! itself. This module is its loop — the bytes of the line in and out, the
//! files of this machine read and written, the time a silent line is given —
//! and the one place a script decides anything during a transfer: which of the
//! files the far end offers are taken.
//!
//! The bytes of the files pass through here, unlike a modem program attached
//! to the line. What that buys is the decision: a program here would take
//! whatever the device sends, and a script that knows which files somebody
//! picked could not tell it so.

use crate::api::{Context, external};
use crate::error::ScriptError;
use mlua::{Function, Lua, Table};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use zmodem2::{Action, Event, FileInfo, Position, Receiver, Sender};

/// How long one read of the line waits for the first byte.
const SLICE: Duration = Duration::from_millis(50);

/// How long the line may stay silent before the other end is asked again.
const SILENCE: Duration = Duration::from_secs(10);

/// How many silences in a row end the transfer.
///
/// A minute of nothing is a device that has gone, or one that never started
/// the program it was told to: either way, waiting longer only makes somebody
/// wait.
const SILENCES: u32 = 6;

/// The longest a script waits to be told how many bytes went: a share that
/// moves says it at once, a transfer slower than one share a step says it this
/// often.
const BYTES_EVERY: Duration = Duration::from_millis(200);

/// How many times in a row what the line said may fail to be read.
///
/// `zmodem2` asks for a damaged piece again by itself and says so only once it
/// has given up, or when a header was past reading; a line that keeps failing
/// after that is not one more retry away from working.
const FAILURES: u32 = 32;

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let zmodem = lua.create_table()?;

    zmodem.set(
        "send",
        lua.create_function({
            let context = context.clone();
            move |lua, how: Table| send(lua, &context, &how)
        })?,
    )?;

    zmodem.set(
        "receive",
        lua.create_function({
            let context = context.clone();
            move |lua, how: Table| receive(lua, &context, &how)
        })?,
    )?;

    Ok(zmodem)
}

/// The error a failure of the protocol is carried as.
fn protocol(source: zmodem2::Error) -> mlua::Error {
    external(ScriptError::Zmodem { source })
}

/// The bytes the line said that the protocol has not taken yet, and how long
/// it has been silent.
struct Wire<'a> {
    context: &'a Context,
    input: Vec<u8>,
    spare: Vec<u8>,
    heard: Instant,
    silences: u32,
    failures: u32,
}

impl<'a> Wire<'a> {
    fn new(context: &'a Context) -> Self {
        Self {
            context,
            input: Vec::new(),
            spare: Vec::new(),
            heard: Instant::now(),
            silences: 0,
            failures: 0,
        }
    }

    /// Hands what is waiting to the protocol, and answers whether it took
    /// any of it.
    ///
    /// What could not be read is dropped rather than offered again: the same
    /// bytes would fail the same way, and the other end is sent the answer
    /// that makes it send them again.
    fn offer(
        &mut self,
        take: impl FnOnce(&[u8]) -> Result<usize, zmodem2::Error>,
    ) -> mlua::Result<bool> {
        if self.input.is_empty() {
            return Ok(false);
        }
        match take(&self.input) {
            Ok(0) => Ok(false),
            Ok(used) => {
                self.input.drain(..used.min(self.input.len()));
                self.failures = 0;
                Ok(true)
            }
            Err(source) => {
                self.input.clear();
                self.failures += 1;
                log::debug!("zmodem: what the line said could not be read: {source}");
                if self.failures > FAILURES {
                    return Err(protocol(source));
                }
                Ok(true)
            }
        }
    }

    /// Waits a moment for the line, and answers whether it said anything.
    fn listen(&mut self) -> mlua::Result<bool> {
        self.context.check()?;
        if !self.context.line.is_open() {
            return Err(external(ScriptError::LineClosed));
        }
        self.context.line.read(&mut self.spare, SLICE);
        if self.spare.is_empty() {
            return Ok(false);
        }
        self.input.extend_from_slice(&self.spare);
        self.heard = Instant::now();
        self.silences = 0;
        Ok(true)
    }

    /// True once the line has been silent for long enough to ask again, which
    /// it counts; past the last silence the transfer is over.
    fn silent(&mut self) -> mlua::Result<bool> {
        if self.heard.elapsed() < SILENCE {
            return Ok(false);
        }
        self.heard = Instant::now();
        self.silences += 1;
        if self.silences >= SILENCES {
            return Err(external(ScriptError::Timeout {
                what: "the other end of a ZMODEM transfer".to_string(),
                seconds: SILENCE.as_secs() * u64::from(SILENCES),
            }));
        }
        Ok(true)
    }
}

/// What a transfer tells the script as it goes, when the script handed it
/// `on`.
///
/// The transfer reports no progress of its own: it says what the protocol is
/// doing, and the script says what of that a person sees, the same way for
/// every protocol it speaks.
struct Told<'a> {
    lua: &'a Lua,
    on: Option<Function>,
    direction: &'static str,
    whole: Option<u64>,
    last: Option<(Instant, Option<u64>)>,
}

impl<'a> Told<'a> {
    fn new(lua: &'a Lua, how: &Table, direction: &'static str) -> mlua::Result<Self> {
        Ok(Self {
            lua,
            on: how.get("on")?,
            direction,
            whole: None,
            last: None,
        })
    }

    /// Calls `on` with an event of `kind` that `fill` completes.
    fn event(&self, kind: &str, fill: impl FnOnce(&Table) -> mlua::Result<()>) -> mlua::Result<()> {
        let Some(on) = &self.on else {
            return Ok(());
        };
        let event = self.lua.create_table()?;
        event.set("kind", kind)?;
        fill(&event)?;
        on.call::<()>(event)
    }

    /// The transfer waits for the other end to agree.
    fn negotiating(&self) -> mlua::Result<()> {
        self.event("negotiating", |_| Ok(()))
    }

    /// A file starts, the `index`th of `files` when that is known.
    fn file(
        &mut self,
        name: &str,
        size: Option<u64>,
        index: usize,
        files: Option<usize>,
    ) -> mlua::Result<()> {
        self.whole = size;
        self.last = None;
        let direction = self.direction;
        self.event("file", |event| {
            event.set("direction", direction)?;
            event.set("name", name)?;
            event.set("size", size)?;
            event.set("index", index)?;
            event.set("files", files)
        })
    }

    /// So many bytes of the file went; said when the share moved, when the
    /// last one went, and otherwise every [`BYTES_EVERY`].
    fn bytes(&mut self, done: u64) -> mlua::Result<()> {
        let share = self
            .whole
            .filter(|whole| *whole > 0)
            .map(|whole| done.min(whole) * 100 / whole);
        let due = match self.last {
            None => true,
            Some((at, last)) => {
                last != share || at.elapsed() >= BYTES_EVERY || Some(done) == self.whole
            }
        };
        if !due {
            return Ok(());
        }
        self.last = Some((Instant::now(), share));
        let whole = self.whole;
        self.event("bytes", |event| {
            event.set("done", done)?;
            event.set("whole", whole)
        })
    }

    /// A file ended: arrived or gone whole, or passed over.
    fn file_done(&self, name: &str, taken: bool) -> mlua::Result<()> {
        self.event("file_done", |event| {
            event.set("name", name)?;
            event.set("taken", taken)
        })
    }

    /// The other end is asked again, the `count`th time in a row.
    fn retrying(&self, count: u32) -> mlua::Result<()> {
        self.event("retrying", |event| event.set("count", count))
    }

    /// The transfer is being closed with the other end.
    fn finishing(&self) -> mlua::Result<()> {
        self.event("finishing", |_| Ok(()))
    }
}

/// One file this side offers.
struct Offer {
    path: PathBuf,
    name: Vec<u8>,
    size: u32,
    modified: Option<u32>,
    file: std::fs::File,
}

impl Offer {
    /// Opens the file and reads what the far end is told about it.
    ///
    /// ZMODEM counts in 32 bits, so a file of four gibibytes or more cannot
    /// be offered at all; saying so before the transfer beats a transfer that
    /// stops at the edge.
    fn of(path: &Path) -> mlua::Result<Self> {
        let io = |source| {
            external(ScriptError::Io {
                path: path.to_path_buf(),
                source,
            })
        };
        let file = std::fs::File::open(path).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        let size = u32::try_from(metadata.len()).map_err(|_| {
            external(ScriptError::TooLarge {
                path: path.to_path_buf(),
                size: metadata.len(),
            })
        })?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|since| u32::try_from(since.as_secs()).ok());
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned().into_bytes())
            .unwrap_or_else(|| b"file".to_vec());

        Ok(Self {
            path: path.to_path_buf(),
            name,
            size,
            modified,
            file,
        })
    }

    /// The name the far end is told, as text.
    fn shown_name(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    /// What the far end is told about this file.
    fn info(&self) -> FileInfo<'_> {
        let info = FileInfo::new(&self.name, Some(Position::new(self.size)));
        match self.modified {
            Some(modified) => info.with_modified(modified),
            None => info,
        }
    }

    /// The bytes of the file at that offset, at most `most` of them.
    fn read_at(&mut self, offset: u32, most: usize, into: &mut Vec<u8>) -> mlua::Result<()> {
        let io = |source| {
            external(ScriptError::Io {
                path: self.path.clone(),
                source,
            })
        };
        into.resize(most, 0);
        self.file
            .seek(SeekFrom::Start(u64::from(offset)))
            .map_err(io)?;
        let mut filled = 0;
        while filled < most {
            let read = self.file.read(&mut into[filled..]).map_err(io)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        into.truncate(filled);
        Ok(())
    }
}

/// What `sz` of `lrzsz` writes before its handshake: a shell at the other end
/// reads it as a command and starts the receiver, and a receiver already
/// running skips it as noise.
const ANNOUNCE: &[u8] = b"rz\r";

/// `zyt.zmodem.send{ paths = { ... }, announce = false }`: sends those files,
/// one after the other, to a receiver at the other end.
///
/// `announce` writes `rz` down the line first, the way `sz` does, for a device
/// nobody told to receive.
fn send(lua: &Lua, context: &Context, how: &Table) -> mlua::Result<()> {
    let paths: Vec<String> = how.get("paths")?;
    let announce: bool = how.get("announce").unwrap_or(false);
    let mut offers = paths
        .iter()
        .map(|path| Offer::of(Path::new(path)))
        .collect::<mlua::Result<Vec<_>>>()?;
    let Some(first) = offers.first() else {
        return Ok(());
    };

    let files = offers.len();
    let mut told = Told::new(lua, how, "send")?;
    told.negotiating()?;
    let mut sender = Sender::new().map_err(protocol)?;
    sender.start_file(first.info()).map_err(protocol)?;
    told.file(
        &first.shown_name(),
        Some(u64::from(first.size)),
        1,
        Some(files),
    )?;
    if announce {
        context.line.write(ANNOUNCE);
    }

    let mut wire = Wire::new(context);
    let mut current = 0;
    let mut chunk = Vec::new();
    let mut finished = false;
    loop {
        if context.cancel.asked() {
            sender.abort();
            flush_sender(context, &mut sender);
            return Err(external(ScriptError::Cancelled));
        }

        match sender.poll() {
            Action::WriteWire(bytes) => {
                let written = bytes.len();
                context.line.write(bytes);
                sender.wire_written(written);
            }
            Action::ReadFile { offset, max_len } => {
                let offer = &mut offers[current];
                offer.read_at(offset.get(), max_len, &mut chunk)?;
                sender.submit_file(&chunk).map_err(protocol)?;
                told.bytes(u64::from(offset.get()) + chunk.len() as u64)?;
            }
            Action::Event(Event::FileCompleted) => {
                told.bytes(u64::from(offers[current].size))?;
                told.file_done(&offers[current].shown_name(), true)?;
                current += 1;
                match offers.get(current) {
                    Some(next) => {
                        sender.start_file(next.info()).map_err(protocol)?;
                        told.file(
                            &next.shown_name(),
                            Some(u64::from(next.size)),
                            current + 1,
                            Some(files),
                        )?;
                    }
                    None => {
                        sender.finish().map_err(protocol)?;
                        told.finishing()?;
                    }
                }
            }
            Action::Event(Event::SessionCompleted) => finished = true,
            Action::Event(Event::Aborted) => {
                return Err(external(ScriptError::Aborted));
            }
            Action::Idle if finished => return Ok(()),
            Action::Idle => {
                if wire.offer(|bytes| sender.submit_wire(bytes))? || wire.listen()? {
                    continue;
                }
                if wire.silent()? {
                    told.retrying(wire.silences)?;
                    sender.timeout().map_err(protocol)?;
                }
            }
            _ => {}
        }
    }
}

/// Writes out the cancel sequence an abort queued, so the far end stops too.
fn flush_sender(context: &Context, sender: &mut Sender) {
    for _ in 0..8 {
        match sender.poll() {
            Action::WriteWire(bytes) => {
                let written = bytes.len();
                context.line.write(bytes);
                sender.wire_written(written);
            }
            _ => return,
        }
    }
}

/// The same for a receiver.
fn flush_receiver(context: &Context, receiver: &mut Receiver) {
    for _ in 0..8 {
        match receiver.poll() {
            Action::WriteWire(bytes) => {
                let written = bytes.len();
                context.line.write(bytes);
                receiver.wire_written(written);
            }
            _ => return,
        }
    }
}

/// What to do with a file of that name that is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    /// Write the new one beside it under a name of its own.
    Rename,
    /// Write over it.
    Overwrite,
    /// Leave it and do not take the new one.
    Skip,
    /// Take it for the start of the one offered, cut short, and ask the
    /// sender for the rest.
    Resume,
}

impl Existing {
    fn of_name(name: &str) -> mlua::Result<Self> {
        match name {
            "rename" => Ok(Self::Rename),
            "overwrite" => Ok(Self::Overwrite),
            "skip" => Ok(Self::Skip),
            "resume" => Ok(Self::Resume),
            other => Err(mlua::Error::runtime(format!(
                "existing must be rename, overwrite, skip or resume, not {other}"
            ))),
        }
    }
}

/// One file the far end offered, and what became of it.
struct Offered {
    name: String,
    size: Option<u32>,
    modified: Option<u32>,
    landing: Option<Landing>,
    completed: bool,
}

impl Offered {
    fn to_lua(&self, lua: &Lua) -> mlua::Result<Table> {
        let table = lua.create_table()?;
        table.set("name", self.name.as_str())?;
        table.set("size", self.size)?;
        table.set("modified", self.modified)?;
        table.set("taken", self.landing.is_some())?;
        if let Some(landing) = &self.landing {
            table.set("path", landing.path.display().to_string())?;
            table.set("resumed", landing.from > 0)?;
        }
        Ok(table)
    }
}

/// The name a file offered under that name is written under: the last part of
/// it, so a name cannot reach out of the directory it is written into.
fn name_of(offered: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(offered);
    let last = text.rsplit(['/', '\\']).next().unwrap_or_default();
    let last = last.trim_matches(char::from(0)).trim();
    match last {
        "" | "." | ".." => None,
        name => Some(name.to_string()),
    }
}

/// Where a taken file is written, and from which byte of it on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Landing {
    path: PathBuf,
    from: u32,
}

impl Landing {
    fn whole(path: PathBuf) -> Self {
        Self { path, from: 0 }
    }

    /// Opens the file to be written from `from` on: what stands before it is
    /// kept and whatever stood after it is cut, so the file is never longer
    /// than what has arrived.
    fn open(&self) -> std::io::Result<std::fs::File> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)?;
        file.set_len(u64::from(self.from))?;
        file.seek(SeekFrom::Start(u64::from(self.from)))?;
        Ok(file)
    }
}

/// Where a file of that name and `size` lands in `into`, or nothing when it is
/// not to be written at all.
///
/// Resuming trusts the name and the length: a file here shorter than the one
/// offered is taken for its start, and one as long or longer for the file
/// itself, already whole. Nothing compares the bytes — `zmodem2` has no ZCRC,
/// and `lrzsz` does not ask for one either. Without a size there is nothing to
/// resume against, and the new file is written beside the old one.
fn landing(into: &Path, name: &str, size: Option<u32>, existing: Existing) -> Option<Landing> {
    let path = into.join(name);
    let Ok(here) = std::fs::metadata(&path) else {
        return Some(Landing::whole(path));
    };
    let beside = || {
        (1..10_000)
            .map(|count| into.join(format!("{name}.{count}")))
            .find(|candidate| !candidate.exists())
            .map(Landing::whole)
    };
    match existing {
        Existing::Overwrite => Some(Landing::whole(path)),
        Existing::Skip => None,
        Existing::Rename => beside(),
        Existing::Resume => match size {
            _ if !here.is_file() => beside(),
            None => beside(),
            Some(size) if here.len() >= u64::from(size) => None,
            Some(_) => Some(Landing {
                path,
                from: u32::try_from(here.len()).unwrap_or(0),
            }),
        },
    }
}

/// `zyt.zmodem.receive{ into = dir, decide = function(file), existing = ... }`:
/// takes what a sender at the other end offers into `into`, and answers with
/// every file it offered.
///
/// `decide`, when there is one, is asked about each file — its `name`, `size`
/// and `modified` — and answers `"take"` or `"skip"`. Without it every file is
/// taken. `existing` says what becomes of a file whose name is already in the
/// directory: `"rename"`, the default, writes the new one beside it, and
/// `"resume"` takes a shorter one for the start of the file and asks for the
/// rest.
fn receive(lua: &Lua, context: &Context, how: &Table) -> mlua::Result<Table> {
    let into = PathBuf::from(how.get::<String>("into")?);
    let decide: Option<Function> = how.get("decide").ok();
    let existing = match how.get::<String>("existing") {
        Ok(name) => Existing::of_name(&name)?,
        Err(_) => Existing::Rename,
    };

    let mut receiver = Receiver::new().map_err(protocol)?;
    receiver.set_manual_file_accept(true);

    let mut wire = Wire::new(context);
    let mut told = Told::new(lua, how, "receive")?;
    told.negotiating()?;
    let mut offered: Vec<Offered> = Vec::new();
    let mut open: Option<(std::fs::File, PathBuf, Option<u32>)> = None;
    let mut decision: Option<Option<Landing>> = None;
    let mut written = 0u64;
    let mut finished = false;
    loop {
        if context.cancel.asked() {
            let _ = receiver.abort();
            flush_receiver(context, &mut receiver);
            return Err(external(ScriptError::Cancelled));
        }

        if let Some(landing) = decision.take() {
            let decided = match &landing {
                Some(landing) => receiver.accept_file_at(landing.from),
                None => receiver.skip_file(),
            };
            match decided {
                Ok(()) => {
                    if let Some(landing) = landing {
                        let file = landing.open().map_err(|source| {
                            external(ScriptError::Io {
                                path: landing.path.clone(),
                                source,
                            })
                        })?;
                        let modified = offered.last().and_then(|file| file.modified);
                        written = u64::from(landing.from);
                        told.bytes(written)?;
                        open = Some((file, landing.path, modified));
                    }
                }
                Err(zmodem2::Error::Backpressure) => decision = Some(landing),
                Err(source) => return Err(protocol(source)),
            }
        }

        match receiver.poll() {
            Action::WriteWire(bytes) => {
                let length = bytes.len();
                context.line.write(bytes);
                receiver.wire_written(length);
            }
            Action::WriteFile(bytes) => {
                let length = bytes.len();
                if let Some((file, path, _)) = open.as_mut() {
                    file.write_all(bytes).map_err(|source| {
                        external(ScriptError::Io {
                            path: path.clone(),
                            source,
                        })
                    })?;
                }
                receiver.file_written(length).map_err(protocol)?;
                written += length as u64;
                told.bytes(written)?;
            }
            Action::Event(Event::FileStarted(info)) => {
                let name = name_of(info.name);
                let size = info.size.map(Position::get);
                let modified = info.modified;
                if let Some(again) = offered.last().filter(|last| {
                    !last.completed && Some(&last.name) == name.as_ref() && last.size == size
                }) {
                    decision = Some(again.landing.clone());
                    continue;
                }
                let wanted = match (&name, &decide) {
                    (None, _) => false,
                    (Some(name), Some(decide)) => {
                        let file = lua.create_table()?;
                        file.set("name", name.as_str())?;
                        file.set("size", size)?;
                        file.set("modified", modified)?;
                        decide.call::<String>(file)? == "take"
                    }
                    (Some(_), None) => true,
                };
                let landing = name
                    .as_ref()
                    .filter(|_| wanted)
                    .and_then(|name| landing(&into, name, size, existing));
                told.file(
                    name.as_deref()
                        .unwrap_or(&String::from_utf8_lossy(info.name)),
                    size.map(u64::from),
                    offered.len() + 1,
                    None,
                )?;
                offered.push(Offered {
                    name: name.unwrap_or_else(|| String::from_utf8_lossy(info.name).into_owned()),
                    size,
                    modified,
                    landing: landing.clone(),
                    completed: false,
                });
                decision = Some(landing);
            }
            Action::Event(Event::FileCompleted) => {
                if let Some(last) = offered.last_mut() {
                    last.completed = true;
                    told.file_done(&last.name, last.landing.is_some())?;
                }
                if let Some((file, path, modified)) = open.take() {
                    finish_file(file, &path, modified)?;
                }
            }
            Action::Event(Event::SessionCompleted) => {
                told.finishing()?;
                finished = true;
            }
            Action::Event(Event::Aborted) => {
                return Err(external(ScriptError::Aborted));
            }
            Action::Idle if finished => break,
            Action::Idle => {
                if decision.is_some() {
                    continue;
                }
                if wire.offer(|bytes| receiver.submit_wire(bytes))? || wire.listen()? {
                    continue;
                }
                if wire.silent()? {
                    told.retrying(wire.silences)?;
                    receiver.timeout().map_err(protocol)?;
                }
            }
            _ => {}
        }
    }

    let answer = lua.create_table()?;
    for (at, file) in offered.iter().enumerate() {
        answer.set(at + 1, file.to_lua(lua)?)?;
    }
    Ok(answer)
}

/// Closes a file that arrived whole and gives it the time the sender said it
/// was last changed.
fn finish_file(file: std::fs::File, path: &Path, modified: Option<u32>) -> mlua::Result<()> {
    let io = |source| {
        external(ScriptError::Io {
            path: path.to_path_buf(),
            source,
        })
    };
    if let Some(modified) = modified {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(modified));
        file.set_modified(time).map_err(io)?;
    }
    file.sync_all().map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name the far end sends is cut to its last part, so it cannot write
    /// anywhere but the directory picked.
    #[test]
    fn a_name_cannot_reach_out_of_the_directory() {
        assert_eq!(name_of(b"notes.txt").as_deref(), Some("notes.txt"));
        assert_eq!(name_of(b"/etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(name_of(b"../../x").as_deref(), Some("x"));
        assert_eq!(name_of(b"a\\b\\c.bin").as_deref(), Some("c.bin"));
        assert_eq!(name_of(b".."), None);
        assert_eq!(name_of(b"dir/"), None);
    }

    /// A name already in the directory is renamed, written over or left alone,
    /// as asked.
    #[test]
    fn a_name_already_there_is_dealt_with_as_asked() {
        let into = tempfile::tempdir().expect("a directory");
        std::fs::write(into.path().join("a.log"), b"old").expect("written");

        let whole = |name: &str| Some(Landing::whole(into.path().join(name)));

        assert_eq!(
            landing(into.path(), "b.log", Some(9), Existing::Skip),
            whole("b.log"),
            "a new name lands as it is"
        );
        assert_eq!(
            landing(into.path(), "a.log", Some(9), Existing::Rename),
            whole("a.log.1")
        );
        assert_eq!(
            landing(into.path(), "a.log", Some(9), Existing::Overwrite),
            whole("a.log")
        );
        assert_eq!(landing(into.path(), "a.log", Some(9), Existing::Skip), None);
    }

    /// A shorter file is the start of the one offered, a file as long is the
    /// one offered, and without a size there is nothing to tell.
    #[test]
    fn a_shorter_file_already_there_is_resumed() {
        let into = tempfile::tempdir().expect("a directory");
        std::fs::write(into.path().join("a.log"), b"old").expect("written");

        assert_eq!(
            landing(into.path(), "a.log", Some(9), Existing::Resume),
            Some(Landing {
                path: into.path().join("a.log"),
                from: 3
            })
        );
        assert_eq!(
            landing(into.path(), "a.log", Some(3), Existing::Resume),
            None,
            "it is already whole"
        );
        assert_eq!(
            landing(into.path(), "a.log", None, Existing::Resume),
            Some(Landing::whole(into.path().join("a.log.1")))
        );
        assert_eq!(
            landing(into.path(), "b.log", Some(9), Existing::Resume),
            Some(Landing::whole(into.path().join("b.log")))
        );
    }

    /// Opening to resume keeps what stands before the start and cuts what
    /// stands after it.
    #[test]
    fn a_resumed_file_is_cut_where_the_rest_begins() {
        let into = tempfile::tempdir().expect("a directory");
        let path = into.path().join("a.log");
        std::fs::write(&path, b"abcdef").expect("written");

        let mut file = Landing {
            path: path.clone(),
            from: 4,
        }
        .open()
        .expect("opened");
        file.write_all(b"XY").expect("written");
        drop(file);
        assert_eq!(std::fs::read(&path).expect("read"), b"abcdXY");

        let mut file = Landing::whole(path.clone()).open().expect("opened");
        file.write_all(b"z").expect("written");
        drop(file);
        assert_eq!(std::fs::read(&path).expect("read"), b"z");
    }
}
