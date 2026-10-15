//! Where control mode starts and ends in the bytes of a line.
//!
//! The mode starts on `ESC P1000p` anywhere in the stream. It ends on `ESC \`
//! right after a line that starts with `%exit`, and nowhere else: a reply of
//! `capture-pane -e` carries a raw `ESC \` at the end of a hyperlink from tmux
//! 3.4 on, and the client of tmux writes the end of the mode only after
//! `%exit`.

use memchr::{memchr, memmem, memrchr};

/// The sequence that starts control mode.
const ENTER: &[u8] = b"\x1bP1000p";
/// The head of the line after which the end of the mode comes.
const EXIT: &[u8] = b"%exit";
const ESC: u8 = 0x1b;
/// The byte after `ESC` that ends the mode.
const STRING_TERMINATOR: u8 = b'\\';

/// One piece of a chunk, in the order of the stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Bytes outside control mode, for the terminal.
    Plain(&'a [u8]),
    /// `ESC P1000p` was read; the bytes after it are of the protocol.
    Entered,
    /// Bytes of the protocol, writable so they are decoded where they stand.
    Control(&'a mut [u8]),
    /// `ESC \` after `%exit` was read; the bytes after it are plain.
    Left,
}

/// Where the splitter stands between two bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Outside the mode, with that many bytes of `ENTER` held back.
    Plain(u8),
    /// At the start of a line of the mode, with that many bytes of `EXIT` read.
    Head(u8),
    /// Inside a line that is not `%exit`.
    Line,
    /// Inside the line `%exit`.
    Exit,
    /// At the start of the line after `%exit`.
    AfterExit,
    /// After `%exit` and an `ESC` that was the last byte of a chunk.
    Escape,
}

/// Splits the bytes of a line into plain bytes and control mode.
///
/// A sequence cut by the end of a chunk is held until the next chunk says
/// what it is: the head of `ESC P1000p`, and the `ESC` of the end of the mode.
/// What is held is a known prefix of either, so the splitter owns no buffer.
#[derive(Debug)]
pub struct Splitter {
    state: State,
}

impl Default for Splitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Splitter {
    /// A splitter outside control mode.
    pub fn new() -> Self {
        Self {
            state: State::Plain(0),
        }
    }

    /// Goes back outside the mode, forgetting what is held: the caller gave
    /// up on a start that turned out to be no tmux, and reads the rest as
    /// plain bytes.
    pub fn reset(&mut self) {
        self.state = State::Plain(0);
    }

    /// True between [`Piece::Entered`] and [`Piece::Left`].
    pub fn inside(&self) -> bool {
        !matches!(self.state, State::Plain(_))
    }

    /// Splits one chunk, calling `each` for every piece in order.
    ///
    /// The bytes held back from the chunk before come first, from the
    /// splitter rather than the chunk; a piece borrows no longer than the
    /// call it is given to.
    pub fn split(&mut self, mut bytes: &mut [u8], mut each: impl FnMut(Piece<'_>)) {
        while !bytes.is_empty() {
            let used = match self.state {
                State::Plain(held) => self.plain(held, bytes, &mut each),
                _ => self.control(bytes, &mut each),
            };
            bytes = &mut std::mem::take(&mut bytes)[used..];
        }
    }

    /// Reads plain bytes up to the start of the mode; returns how many.
    fn plain(&mut self, held: u8, bytes: &mut [u8], each: &mut impl FnMut(Piece<'_>)) -> usize {
        if held > 0 {
            return self.resume_enter(usize::from(held), bytes, each);
        }
        if let Some(at) = memmem::find(bytes, ENTER) {
            if at > 0 {
                each(Piece::Plain(&bytes[..at]));
            }
            self.state = State::Head(0);
            each(Piece::Entered);
            return at + ENTER.len();
        }
        let window = bytes.len().saturating_sub(ENTER.len() - 1);
        let held = memrchr(ESC, &bytes[window..])
            .map(|offset| window + offset)
            .filter(|&at| ENTER.starts_with(&bytes[at..]));
        let plain = held.unwrap_or(bytes.len());
        if plain > 0 {
            each(Piece::Plain(&bytes[..plain]));
        }
        if let Some(at) = held {
            self.state = State::Plain(prefix(bytes.len() - at));
        }
        bytes.len()
    }

    /// Goes on with the head of `ENTER` held from the chunk before.
    fn resume_enter(
        &mut self,
        held: usize,
        bytes: &[u8],
        each: &mut impl FnMut(Piece<'_>),
    ) -> usize {
        let wanted = &ENTER[held..];
        let compared = wanted.len().min(bytes.len());
        if bytes[..compared] != wanted[..compared] {
            self.state = State::Plain(0);
            each(Piece::Plain(&ENTER[..held]));
            return 0;
        }
        if compared == wanted.len() {
            self.state = State::Head(0);
            each(Piece::Entered);
        } else {
            self.state = State::Plain(prefix(held + compared));
        }
        compared
    }

    /// Reads bytes of the mode up to its end; returns how many, the end
    /// included.
    fn control(&mut self, bytes: &mut [u8], each: &mut impl FnMut(Piece<'_>)) -> usize {
        if self.state == State::Escape {
            if bytes[0] == STRING_TERMINATOR {
                self.state = State::Plain(0);
                each(Piece::Left);
                return 1;
            }
            self.state = State::Line;
            each(Piece::Control(&mut [ESC]));
            return 0;
        }
        let mut at = 0;
        while at < bytes.len() {
            match self.state {
                State::Head(read) => {
                    let read = usize::from(read);
                    if bytes[at] != EXIT[read] {
                        self.state = State::Line;
                    } else if read + 1 == EXIT.len() {
                        self.state = State::Exit;
                        at += 1;
                    } else {
                        self.state = State::Head(prefix(read + 1));
                        at += 1;
                    }
                }
                State::Line | State::Exit => match memchr(b'\n', &bytes[at..]) {
                    Some(offset) => {
                        at += offset + 1;
                        self.state = match self.state {
                            State::Exit => State::AfterExit,
                            _ => State::Head(0),
                        };
                    }
                    None => at = bytes.len(),
                },
                State::AfterExit if bytes[at] == ESC => match bytes.get(at + 1) {
                    None => {
                        self.state = State::Escape;
                        emit_control(&mut bytes[..at], each);
                        return bytes.len();
                    }
                    Some(&STRING_TERMINATOR) => {
                        self.state = State::Plain(0);
                        emit_control(&mut bytes[..at], each);
                        each(Piece::Left);
                        return at + 2;
                    }
                    Some(_) => self.state = State::Line,
                },
                State::AfterExit => self.state = State::Head(0),
                State::Plain(_) | State::Escape => unreachable!("not a state of the mode"),
            }
        }
        emit_control(bytes, each);
        bytes.len()
    }
}

/// Gives bytes of the mode, unless there are none.
fn emit_control(bytes: &mut [u8], each: &mut impl FnMut(Piece<'_>)) {
    if !bytes.is_empty() {
        each(Piece::Control(bytes));
    }
}

/// A count of bytes of `ENTER` or `EXIT`, which both fit a byte.
fn prefix(count: usize) -> u8 {
    u8::try_from(count).expect("a prefix of a short constant")
}

#[cfg(test)]
mod tests;
