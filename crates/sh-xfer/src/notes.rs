//! What is said to the user while a transfer runs.
//!
//! It goes to the standard error, because the standard output is the line to
//! the device and anything written there would reach the shell instead of the
//! user.
//!
//! Words and nothing else, with one exception the user asked for: the list of
//! the files a `get` saved, where each name is a hyperlink to the file itself.
//! A transfer of this kind is driven as readily by a program collecting the
//! lines as by someone watching a terminal, and an escape sequence meant for
//! the second is text to be shown by the first — which is why that list is
//! said once, at the end, and nothing along the way carries one.

use std::io::Write;

/// Writes what is said about a transfer, or says nothing at all.
pub struct Notes<W: Write> {
    out: W,
    quiet: bool,
}

impl<W: Write> Notes<W> {
    /// Says its lines on `out`, or says nothing at all when `quiet`.
    pub fn new(out: W, quiet: bool) -> Self {
        Self { out, quiet }
    }

    /// Says one line.
    pub fn note(&mut self, text: &str) {
        if self.quiet {
            return;
        }
        let _ = writeln!(self.out, "{text}");
        let _ = self.out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_said_whole_and_at_once() {
        let mut out = Vec::new();
        let mut notes = Notes::new(&mut out, false);
        notes.note("one");
        notes.note("two");
        assert_eq!(
            String::from_utf8(out).expect("the notes are text"),
            "one\ntwo\n"
        );
    }

    #[test]
    fn quiet_says_nothing() {
        let mut out = Vec::new();
        let mut notes = Notes::new(&mut out, true);
        notes.note("something");
        assert!(out.is_empty());
    }
}
