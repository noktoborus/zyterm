//! The client side of control mode: commands sent, and what comes back read
//! and matched to them.

use std::collections::VecDeque;

use memchr::memchr;

use crate::command::{self, Command, Keys};
use crate::error::{Error, Result};
use crate::escape;
use crate::id::{PaneId, WindowId};
use crate::line::{self, Guard, Line, Notice};

/// Where what comes back goes.
pub trait Sink {
    /// Bytes of a pane from `%output` or `%extended-output`, decoded; borrowed
    /// from the chunk or from the line held across chunks.
    /// `age`: milliseconds, from `%extended-output`; `None` from `%output`.
    fn output(&mut self, pane: PaneId, age: Option<u32>, bytes: &[u8]);
    /// One line of a reply, as tmux wrote it. [`Reply::Broken`] says the reply
    /// is longer than it can be: its `%end` was lost.
    fn reply_line(&mut self, command: Command, line: &[u8]) -> Reply;
    /// The end of a reply. `failed`: `%error`, or a reply closed as broken.
    fn reply_end(&mut self, command: Command, failed: bool);
    /// Everything else; rare, and free to allocate.
    fn notice(&mut self, notice: Notice<'_>);
}

/// What the caller says of a line of a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Go on.
    Go,
    /// The reply is longer than it can be: it is closed as failed.
    Broken,
}

/// A reply between its `%begin` and its `%end`.
#[derive(Clone, Copy, Debug)]
struct Open {
    /// `None`: a reply with nothing sent for it, read and thrown away.
    command: Option<Command>,
    stamp: (u64, u64),
    /// Closed as broken; its lines are thrown away until its `%end`.
    closed: bool,
}

/// Commands sent, and replies matched to them.
///
/// tmux answers in order, so the queue of what was sent is the whole
/// matching: a reply with flags `1` takes the command at its head. The pair
/// with flags `0` that opens the mode answers nothing of this side and comes
/// as [`Command::Opening`].
#[derive(Debug)]
pub struct Client {
    sent: VecDeque<Command>,
    partial: Vec<u8>,
    limit: usize,
    overlong: bool,
    reply: Option<Open>,
    lossy: bool,
}

impl Client {
    /// `lossy` on a port, where a byte may be lost: a notification inside a
    /// reply closes it, a `%begin` inside one starts the next, and an `%end`
    /// with no `%begin` takes its command off the queue as failed.
    pub fn new(lossy: bool) -> Self {
        Self {
            sent: VecDeque::new(),
            partial: Vec::new(),
            limit: 0,
            overlong: false,
            reply: None,
            lossy,
        }
    }

    /// Commands sent whose reply has not ended, oldest first.
    pub fn sent(&self) -> impl ExactSizeIterator<Item = Command> + '_ {
        self.sent.iter().copied()
    }

    /// Brings the line held across chunks to `bytes`, as `ByteSwap` brings its
    /// buffers: a line longer is dropped as [`Error::MalformedLine`]. Zero is
    /// no limit.
    pub fn fit(&mut self, bytes: usize) {
        self.limit = bytes;
        if bytes > 0 && self.partial.len() > bytes {
            self.partial.clear();
            self.overlong = true;
        }
        self.partial.shrink_to(bytes.max(self.partial.len()));
    }

    /// Forgets what was sent and the line held: the line was lost, and no
    /// reply on its way will come.
    pub fn forget(&mut self) {
        self.sent.clear();
        self.partial.clear();
        self.overlong = false;
        self.reply = None;
    }

    /// Appends the line of `command` to `out` and queues what it answers.
    ///
    /// [`Command::SendKeys`] and [`Command::RenameWindow`] are written by
    /// [`send_keys`](Self::send_keys) and [`rename_window`](Self::rename_window),
    /// and [`Command::Opening`] is never sent: given here, they write and queue
    /// nothing.
    pub fn send(&mut self, command: Command, out: &mut Vec<u8>) {
        let mark = out.len();
        if command::write(command, out) {
            out.push(b'\n');
            self.sent.push_back(command);
        } else {
            out.truncate(mark);
        }
    }

    /// Appends several commands as one line, joined by ` ; `, and queues
    /// each. tmux runs the commands of a line with no `%output` between their
    /// replies, so what they ask agrees with itself: the modes of a pane and
    /// its screens.
    pub fn send_line(&mut self, commands: &[Command], out: &mut Vec<u8>) {
        let mut first = true;
        for &command in commands {
            let mark = out.len();
            if !first {
                out.extend_from_slice(b" ; ");
            }
            if command::write(command, out) {
                self.sent.push_back(command);
                first = false;
            } else {
                out.truncate(mark);
            }
        }
        if !first {
            out.push(b'\n');
        }
    }

    /// Appends `send-keys -t %N`: `-l -- '…'` for each run of printable text and
    /// `-H …` for the rest, joined by ` ; `, and queues a `SendKeys` for each —
    /// its reply is empty, read and thrown away, but it takes its place.
    pub fn send_keys(&mut self, pane: PaneId, bytes: &[u8], out: &mut Vec<u8>) {
        self.send_input(pane, &[Keys::Bytes(bytes)], out);
    }

    /// Appends pieces of input as [`send_keys`](Self::send_keys) does, in
    /// their order, a key by its name as `send-keys -t %N -- 'Name'`: tmux
    /// encodes it for the terminal of the pane, as for a key typed in a client
    /// that is not in control mode.
    pub fn send_input(&mut self, pane: PaneId, keys: &[Keys<'_>], out: &mut Vec<u8>) {
        let runs = command::runs_of(keys);
        if runs.is_empty() {
            return;
        }
        for (index, run) in runs.iter().enumerate() {
            if index > 0 {
                out.extend_from_slice(b" ; ");
            }
            command::write_run(pane, run, out);
            self.sent.push_back(Command::SendKeys(pane));
        }
        out.push(b'\n');
    }

    /// Appends `rename-window`, quoting the name for a command line, and
    /// queues `RenameWindow` as `send_keys` queues its own.
    pub fn rename_window(&mut self, window: WindowId, name: &str, out: &mut Vec<u8>) {
        command::write_rename(window, name, out);
        out.push(b'\n');
        self.sent.push_back(Command::RenameWindow(window));
    }

    /// Reads a chunk of the protocol, decoding `%output` in place.
    ///
    /// A bad line does not stop the chunk: it is skipped, and the first error
    /// of the chunk is returned once the rest has been read.
    pub fn feed(&mut self, bytes: &mut [u8], sink: &mut impl Sink) -> Result<()> {
        let mut first_error = None;
        let mut rest = bytes;
        while let Some(at) = memchr(b'\n', rest) {
            let (head, tail) = std::mem::take(&mut rest).split_at_mut(at + 1);
            rest = tail;
            let outcome = if self.overlong {
                self.overlong = false;
                Ok(())
            } else if self.partial.is_empty() {
                self.line(head, sink)
            } else {
                self.held_line(head, sink)
            };
            if let Err(error) = outcome {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.hold(rest) {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Keeps the head of a line cut by the end of a chunk.
    fn hold(&mut self, head: &[u8]) -> Result<()> {
        if self.overlong || head.is_empty() {
            return Ok(());
        }
        if self.limit > 0 && self.partial.len() + head.len() > self.limit {
            self.partial.clear();
            self.overlong = true;
            return Err(Error::MalformedLine);
        }
        self.partial.extend_from_slice(head);
        Ok(())
    }

    /// Ends the line held across chunks with `tail` and reads it.
    fn held_line(&mut self, tail: &[u8], sink: &mut impl Sink) -> Result<()> {
        let mut line = std::mem::take(&mut self.partial);
        let outcome = if self.limit > 0 && line.len() + tail.len() > self.limit {
            Err(Error::MalformedLine)
        } else {
            line.extend_from_slice(tail);
            self.line(&mut line, sink)
        };
        line.clear();
        self.partial = line;
        outcome
    }

    /// Reads one whole line, its `\n` included.
    fn line(&mut self, line: &mut [u8], sink: &mut impl Sink) -> Result<()> {
        let line = trim_end(line);
        let parsed = line::parse(line);
        if let Some(open) = self.reply {
            match parsed {
                Line::End(guard) | Line::Error(guard) if guard.stamp == open.stamp => {
                    self.reply = None;
                    if let (Some(command), false) = (open.command, open.closed) {
                        sink.reply_end(command, matches!(parsed, Line::Error(_)));
                    }
                    return Ok(());
                }
                ref notification if self.lossy && notification.is_notification() => {
                    self.close(open, sink);
                }
                _ => {
                    self.reply_line(open, line, sink);
                    return Ok(());
                }
            }
        }
        match parsed {
            Line::Begin(guard) => self.begin(guard),
            Line::Output { pane, age, start } => {
                let bytes = &mut line[start..];
                let length = escape::decode(bytes);
                sink.output(pane, age, &bytes[..length]);
                Ok(())
            }
            Line::Notice(notice) => {
                sink.notice(notice);
                Ok(())
            }
            Line::End(guard) | Line::Error(guard) if self.lossy && guard.flags & 1 == 1 => {
                if let Some(command) = self.sent.pop_front() {
                    sink.reply_end(command, true);
                }
                Ok(())
            }
            Line::End(_) | Line::Error(_) | Line::Text | Line::Malformed => {
                Err(Error::MalformedLine)
            }
        }
    }

    /// Gives a line of an open reply to the caller, unless nobody waits for it.
    fn reply_line(&mut self, open: Open, line: &[u8], sink: &mut impl Sink) {
        let (Some(command), false) = (open.command, open.closed) else {
            return;
        };
        if sink.reply_line(command, line) == Reply::Broken {
            self.reply = Some(Open {
                closed: true,
                ..open
            });
            sink.reply_end(command, true);
        }
    }

    /// Takes a reply off the queue as failed, its `%end` lost.
    fn close(&mut self, open: Open, sink: &mut impl Sink) {
        self.reply = None;
        if let (Some(command), false) = (open.command, open.closed) {
            sink.reply_end(command, true);
        }
    }

    /// A `%begin`: the reply of the command at the head of the queue, or of
    /// the opening of the mode.
    fn begin(&mut self, guard: Guard) -> Result<()> {
        let command = if guard.flags & 1 == 0 {
            Some(Command::Opening)
        } else {
            self.sent.pop_front()
        };
        self.reply = Some(Open {
            command,
            stamp: guard.stamp,
            closed: false,
        });
        command.map(|_| ()).ok_or(Error::UnexpectedReply)
    }
}

/// The line without its `\n` and one `\r` before it.
fn trim_end(line: &mut [u8]) -> &mut [u8] {
    let mut length = line.len();
    if line[..length].ends_with(b"\n") {
        length -= 1;
    }
    if line[..length].ends_with(b"\r") {
        length -= 1;
    }
    &mut line[..length]
}

#[cfg(test)]
mod tests;
