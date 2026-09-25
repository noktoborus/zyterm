//! The conversation with the shell on the other end of the line.
//!
//! Every command is a shell script that ends by echoing a reply line. The
//! device is told nothing about the protocol: it runs what it is given, and
//! what it prints is the answer.

use crate::catalog::{Body, Mode, ModeCatalog};
use crate::checksum::Digest;
use crate::error::{Result, ShXferError};
use crate::reader::Reader;
use crate::report::Report;
use crate::wire::{Reply, quote};
use base64::Engine;
use std::io::{Read, Write};

/// How many bytes of the line one chunk takes on a console, until the caller
/// says otherwise.
///
/// A console carries the bytes themselves, one after another, so a chunk costs
/// what it carries and the round trip that ends it is a rounding error. Small
/// chunks are what say where a transfer stopped.
pub const DEFAULT_CHUNK: usize = 2048;

/// How many bytes of the line one chunk takes over a pipe.
///
/// A pipe is usually a shell somewhere else, and then a chunk costs a round
/// trip over whatever stands between: the client sends a chunk, waits to hear
/// that it landed, and sends the next. What travels between two round trips is
/// what the transfer is worth, so a chunk is large where the waiting is.
pub const DEFAULT_PIPE_CHUNK: usize = 65536;

/// What kind of thing an entry of a listing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A file, or a symbolic link that leads to one.
    File,
    /// A directory, or a symbolic link that leads to one.
    Directory,
    /// Anything else, which does not travel.
    Other,
}

/// One entry of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Name inside the directory that was listed.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Size in bytes, zero for anything that is not a file.
    pub size: u64,
}

/// The conversation with the device.
pub struct Session<W: Write> {
    reader: Reader,
    writer: W,
    mode: Mode,
    catalog: ModeCatalog,
    chunk: usize,
    size_check: bool,
    pending: Option<String>,
    report: Box<dyn Report>,
}

impl<W: Write> Session<W> {
    /// A conversation over a line that is read from `reader` and written to
    /// `writer`, sending the scripts of `mode`.
    pub fn new(reader: Reader, writer: W, mode: Mode) -> Self {
        Self {
            reader,
            writer,
            mode,
            catalog: mode.catalog(),
            chunk: DEFAULT_CHUNK,
            size_check: true,
            pending: None,
            report: Box::new(()),
        }
    }

    /// Tells `report` what the session does, command by command and chunk by
    /// chunk. Nothing is said until one is set.
    pub fn listen(&mut self, report: Box<dyn Report>) {
        self.report = report;
    }

    /// Which set of scripts this conversation sends.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// How many bytes of the line one chunk of a body may take.
    ///
    /// It is the size on the wire, not in the file: as base64 a chunk carries
    /// three bytes for every four it costs, and the slice of the file is cut
    /// to whole lines of base64 so no line is ever longer than a terminal will
    /// keep.
    pub fn set_chunk_size(&mut self, bytes: usize) {
        self.chunk = bytes.max(1);
    }

    /// Whether a file that was written is measured on the device afterwards.
    ///
    /// It is one command at the end of a file and it is what catches a chunk
    /// that never landed. Switched off, a file is believed as it was written,
    /// and only a sum — if one was asked for — has anything to say about it.
    pub fn set_size_check(&mut self, check: bool) {
        self.size_check = check;
    }

    /// How many bytes of a file one chunk carries.
    pub fn slice(&self) -> u64 {
        self.mode.slice(self.chunk)
    }

    /// The far half of the line, once the conversation is over.
    ///
    /// A test reads back what was sent this way; nothing else needs it.
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Asks the device whether it has what this mode cannot do without.
    ///
    /// Every tool is tried rather than asked after: `command -v` answers for
    /// what stands in the path, which on a device whose utilities are applets
    /// of one binary is a different question. The probe answers `H<program>`
    /// for one that works and `E<program>` for one that does not, and an `E`
    /// ends the conversation there: a transfer that cannot place a chunk is
    /// better refused before the first byte than halfway through a file.
    ///
    /// Nothing of the line is touched here — that belongs to the command
    /// carrying a body, and to no other.
    pub fn hello(&mut self) -> Result<()> {
        let answer = self.ask(self.catalog.hello, &[], "#VER")?;
        log::debug!("the device answered the probe with {answer:?}");
        match missing(&answer) {
            Some(command) => Err(ShXferError::MissingCommand { command }),
            None => Ok(()),
        }
    }

    /// Whether the device can say this sum of a file.
    ///
    /// Each sum has a probe of its own, tried the same way as the tools a
    /// transfer needs, and an `E` here is no error: it only means a file
    /// cannot be checked that way afterwards.
    pub fn offers_digest(&mut self, digest: Digest) -> Result<bool> {
        let Some(probe) = self.catalog.probe(digest.program()) else {
            return Ok(false);
        };
        let answer = self.ask(probe, &[], "#PROBE")?;
        Ok(missing(&answer).is_none())
    }

    /// Where the device stands.
    pub fn pwd(&mut self) -> Result<String> {
        let answer = self.ask(self.catalog.pwd, &[], "#PWD")?;
        Ok(answer.first().cloned().unwrap_or_default())
    }

    /// The path a directory really is, with every symbolic link resolved.
    pub fn canonical(&mut self, directory: &str) -> Result<String> {
        let path = quote(directory);
        let answer = self.ask(self.catalog.canonical, &[("path", &path)], "#CANON")?;
        Ok(answer.first().cloned().unwrap_or_default())
    }

    /// What a path of the device is.
    ///
    /// Asking is worth a round trip: a listing of a file names it by the path
    /// it was asked for and a listing of a directory names its entries, so
    /// telling the two apart from a listing alone is guesswork. How big a file
    /// is, is [`Session::size`] and another round trip: a walk asks it of the
    /// files it is going to carry and of nothing else.
    pub fn kind(&mut self, path: &str) -> Result<EntryKind> {
        let quoted = quote(path);
        let answer = self.ask(self.catalog.kind, &[("path", &quoted)], "#KIND")?;
        Ok(match answer.first().map(String::as_str) {
            Some("D") => EntryKind::Directory,
            Some("F") => EntryKind::File,
            _ => EntryKind::Other,
        })
    }

    /// How many bytes a file of the device holds.
    ///
    /// A device that cannot say leaves the reply line bare, and that is the
    /// end of it: a body travels in chunks, and a chunk is cut from a size.
    pub fn size(&mut self, path: &str) -> Result<u64> {
        let quoted = quote(path);
        self.send(self.catalog.size, &[("path", &quoted)])?;
        let (reply, text) = self.reply("#SIZE")?;
        if reply != Reply::Data {
            return Err(refused("#SIZE", reply));
        }
        let size = text.trim().parse::<u64>().ok();
        self.expect("#SIZE", Reply::End)?;
        size.ok_or_else(|| ShXferError::NoSize {
            path: path.to_string(),
        })
    }

    /// What the device says the sum of a file there is.
    ///
    /// Whatever the program writes, only the first field of it is taken: that
    /// is where `md5sum`, `sha1sum` and `sha256sum` all put the sum.
    pub fn digest(&mut self, digest: Digest, path: &str) -> Result<String> {
        let program = format!("\\{}", digest.program());
        let quoted = quote(path);
        let answer = self.ask(
            self.catalog.digest,
            &[("program", &program), ("path", &quoted)],
            "#SUM",
        )?;
        answer
            .iter()
            .find_map(|line| line.trim().strip_prefix('D'))
            .filter(|sum| !sum.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ShXferError::NoDigest {
                program: digest.program().to_string(),
                path: path.to_string(),
            })
    }

    /// What a directory holds, symbolic links resolved to what they lead to.
    pub fn list(&mut self, directory: &str) -> Result<Vec<Entry>> {
        let path = quote(directory);
        Ok(entries(&self.ask(
            self.catalog.list,
            &[("path", &path)],
            "#LIST",
        )?))
    }

    /// Makes a directory on the device, and every directory above it.
    pub fn make_directory(&mut self, directory: &str) -> Result<()> {
        let path = quote(directory);
        self.send(self.catalog.make_directory, &[("path", &path)])?;
        self.expect("#MKD", Reply::Done)
    }

    /// Makes a file of no bytes on the device, throwing away what stood there.
    ///
    /// A body is written chunk by chunk, each one seeking to where it belongs
    /// and leaving the rest of the file alone, so the file has to start empty.
    pub fn create(&mut self, path: &str) -> Result<()> {
        let quoted = quote(path);
        self.send(self.catalog.create, &[("path", &quoted)])?;
        self.expect("#CREA", Reply::Done)
    }

    /// Reads a file of the device into `out`, one chunk at a time.
    ///
    /// The size is asked for first, because a chunk is cut from it: how much
    /// is left to ask for is what the device said less what has arrived. Every
    /// chunk is a command of its own, so a transfer that stops says where it
    /// stopped, and the next command is decided by the bytes that really came
    /// rather than by the ones that were asked for.
    pub fn retrieve(&mut self, path: &str, out: &mut impl Write) -> Result<u64> {
        let size = self.size(path)?;
        let quoted = quote(path);
        let slice = self.slice();
        let mut start = 0_u64;

        while start < size {
            let count = slice.min(size - start);
            self.send(
                self.catalog.retrieve,
                &[
                    ("path", &quoted),
                    ("offset", &(start + 1).to_string()),
                    ("count", &count.to_string()),
                ],
            )?;
            self.report.chunk(start, start + count);
            self.expect("#RETR", Reply::Data)?;

            let got = match self.mode {
                Mode::Raw => self.read_raw(out, count)?,
                Mode::Base64 => self.read_base64(out)?,
            };
            self.expect("#RETR", Reply::End)?;
            if got == 0 {
                return Err(ShXferError::Truncated {
                    path: path.to_string(),
                    expected: size,
                    got: start,
                });
            }
            start += got;
            self.report.carried(got);
        }

        Ok(start)
    }

    /// Writes a file onto the device, one chunk at a time.
    ///
    /// The file is emptied first and every chunk is appended, so what is
    /// written is what was read here, whatever the device did with the path
    /// before. When the last chunk has landed the device is asked how long the
    /// file is: a chunk that never arrived is a file of another length, and
    /// asking once costs one command instead of reading the whole file over
    /// again after every chunk.
    pub fn store(&mut self, path: &str, source: &mut impl Read) -> Result<u64> {
        self.create(path)?;
        let quoted = quote(path);
        let mut buffer = vec![0_u8; self.slice() as usize];
        let mut start = 0_u64;

        loop {
            let count = read_filled(source, &mut buffer)?;
            if count == 0 {
                break;
            }
            self.write_chunk(&quoted, &buffer[..count], start)?;
            start += count as u64;
            self.report.carried(count as u64);
        }

        if self.size_check {
            self.measure_stored(path, start)?;
        }
        Ok(start)
    }

    /// Asks the device how long a file it was just given is, and says so.
    ///
    /// One line of the log carries the whole stage: the file, how much was
    /// written and what the device says stands there.
    fn measure_stored(&mut self, path: &str, written: u64) -> Result<()> {
        let there = self.size(path)?;
        let verdict = match there == written {
            true => "whole",
            false => "wrong length",
        };
        log::info!("{path}: {written} bytes written, {there} on the device, {verdict}");

        if there != written {
            return Err(ShXferError::WrongSize {
                path: path.to_string(),
                expected: written,
                got: there,
            });
        }
        Ok(())
    }

    /// Writes one chunk, whatever the body of one is made of in this mode.
    ///
    /// The command is rendered from the same values either way — where the
    /// chunk belongs, and how many bytes of the line its body takes. What
    /// differs is how the body is framed, which is what the script expects.
    fn write_chunk(&mut self, path: &str, bytes: &[u8], start: u64) -> Result<()> {
        let body = self.mode.body(bytes);
        let delimiter = match self.catalog.body {
            Body::Document { delimiter, .. } => delimiter,
            Body::Counted => "",
        };
        self.send(
            self.catalog.store,
            &[
                ("path", path),
                ("count", &body.len().to_string()),
                ("heredoc", delimiter),
            ],
        )?;
        self.report.chunk(start, start + bytes.len() as u64);

        match self.catalog.body {
            Body::Counted => {
                self.expect("#STOR", Reply::Ready)?;
                self.write_body(&body)?;
            }
            Body::Document { delimiter, end } => {
                self.write_body(&body)?;
                self.line(delimiter)?;
                self.send(end, &[])?;
            }
        }
        self.expect("#STOR", Reply::End)
    }

    /// Reads the bytes themselves, exactly as many as were asked for.
    fn read_raw(&mut self, out: &mut impl Write, count: u64) -> Result<u64> {
        let chunk = self.reader.exact(count as usize)?;
        out.write_all(&chunk).map_err(line_error)?;
        Ok(chunk.len() as u64)
    }

    /// Reads lines of base64 until the reply that ends the chunk.
    fn read_base64(&mut self, out: &mut impl Write) -> Result<u64> {
        let mut done = 0_u64;
        loop {
            let line = self.next_line()?;
            if Reply::of_line(&line).is_some() {
                self.pending = Some(line);
                return Ok(done);
            }
            if line.trim().is_empty() {
                continue;
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(line.trim())
                .map_err(|source| ShXferError::Decode { source })?;
            out.write_all(&bytes).map_err(line_error)?;
            done += bytes.len() as u64;
        }
    }

    /// Writes the body of a chunk, which is no command and says nothing.
    fn write_body(&mut self, body: &[u8]) -> Result<()> {
        self.writer.write_all(body).map_err(line_error)?;
        self.writer.flush().map_err(line_error)
    }

    /// Sends a command of the catalogue with its holes filled.
    ///
    /// Nothing sent here ever carries the marker of a reply line: a console
    /// echoes back what it is given, and a script saying it plainly would come
    /// back looking like the answer it asks for. See [`crate::wire`].
    fn send(&mut self, command: crate::script::Command, values: &[(&str, &str)]) -> Result<()> {
        let script = command.render(values);
        log::debug!("-> {script}");
        self.report.command(&script);
        self.line(&script)
    }

    /// Writes one line to the device.
    ///
    /// The newline is what ends a line for both a console and a pipe: the
    /// line discipline of a terminal ends a line on it as readily as on the
    /// carriage return a keyboard sends, and a pipe knows nothing else.
    fn line(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes()).map_err(line_error)?;
        self.writer.write_all(b"\n").map_err(line_error)?;
        self.writer.flush().map_err(line_error)
    }

    /// Sends a command and gathers the lines it frames.
    fn ask(
        &mut self,
        command: crate::script::Command,
        values: &[(&str, &str)],
        name: &str,
    ) -> Result<Vec<String>> {
        self.send(command, values)?;
        self.collect(name)
    }

    /// Reads until the device answers, skipping everything that is not a reply.
    fn reply(&mut self, command: &str) -> Result<(Reply, String)> {
        loop {
            let line = self.next_line()?;
            let Some(reply) = Reply::of_line(&line) else {
                log::debug!("<- {line}");
                continue;
            };
            if reply == Reply::Failed {
                return Err(refused(command, reply));
            }
            return Ok((reply, Reply::text_of_line(&line)));
        }
    }

    /// Reads until the device answers and insists on one reply.
    fn expect(&mut self, command: &str, expected: Reply) -> Result<()> {
        let (reply, _) = self.reply(command)?;
        if reply == expected || (expected == Reply::Done && reply == Reply::End) {
            Ok(())
        } else {
            Err(ShXferError::Unexpected {
                command: command.to_string(),
                expected: expected.code(),
                code: reply.code(),
            })
        }
    }

    /// The lines between the two replies that frame them.
    fn collect(&mut self, command: &str) -> Result<Vec<String>> {
        let (reply, _) = self.reply(command)?;
        if reply == Reply::End || reply == Reply::Done {
            return Ok(Vec::new());
        }
        if reply != Reply::Data {
            return Err(refused(command, reply));
        }

        let mut lines = Vec::new();
        loop {
            let line = self.next_line()?;
            match Reply::of_line(&line) {
                Some(Reply::Failed) => return Err(refused(command, Reply::Failed)),
                Some(_) => {
                    log::debug!("{command} answered {lines:?}");
                    return Ok(lines);
                }
                None => lines.push(line),
            }
        }
    }

    /// The next line, which may be one that was read and handed back.
    fn next_line(&mut self) -> Result<String> {
        match self.pending.take() {
            Some(line) => Ok(line),
            None => self.reader.line(),
        }
    }
}

/// The first program the device answered the probe with an `E` for.
///
/// The probe writes `H<program>` for one that works and `E<program>` for one
/// that does not, so a complaint can name what is missing without the client
/// keeping a list of what it asked for.
fn missing(answer: &[String]) -> Option<String> {
    answer
        .iter()
        .find_map(|line| line.trim().strip_prefix('E'))
        .filter(|program| !program.is_empty())
        .map(str::to_string)
}

/// The lines of a listing turned into entries.
fn entries(lines: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut kind = EntryKind::Other;
    let mut size = 0_u64;

    for line in lines {
        let Some((mark, rest)) = line.split_at_checked(1) else {
            continue;
        };
        match mark {
            "P" => {
                kind = match rest.chars().next() {
                    Some('d') => EntryKind::Directory,
                    Some('-') => EntryKind::File,
                    _ => EntryKind::Other,
                }
            }
            "S" => size = rest.trim().parse().unwrap_or(0),
            ":" => {
                let name = rest.to_string();
                if name != "." && name != ".." && !name.is_empty() {
                    out.push(Entry { name, kind, size });
                }
                kind = EntryKind::Other;
                size = 0;
            }
            _ => {}
        }
    }
    out
}

/// Fills the buffer as far as the source goes, so a short read does not cut a
/// line of base64 into a shorter one than it should be.
fn read_filled(source: &mut impl Read, buffer: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let count = source.read(&mut buffer[filled..]).map_err(line_error)?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    Ok(filled)
}

fn refused(command: &str, reply: Reply) -> ShXferError {
    ShXferError::Refused {
        command: command.to_string(),
        code: reply.code(),
    }
}

fn line_error(source: std::io::Error) -> ShXferError {
    ShXferError::Line { source }
}
