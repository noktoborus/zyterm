//! Text that reads as a link, and the link a double-click lands on.
//!
//! A link a program marked (OSC 8) says where it leads. Text that only looks like
//! one says nothing, so the shape is all there is: an address with a scheme,
//! `scheme://…`, or a mail, `mailto:…`, and on Windows a path of a drive or of a
//! share. A path of Unix is not taken: `/` and `~/` begin far too much that is
//! not a path.
//!
//! A double-click picks out a word, and the separators of a word take an
//! address apart at its colon. So a double-click on a link takes the link whole:
//! the run of text with no blank in it around the press, less what stands
//! around a link in a sentence — the stop after it, the brackets or quotes it is
//! written in.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use std::ops::Range;

/// Whether the whole of `text` is a link.
///
/// Nothing is trimmed: a selection is what somebody picked out, and a space or a
/// stop at its edge is a sign it is not an address alone.
pub fn is_link(text: &str) -> bool {
    shaped(text, cfg!(windows))
}

/// [`is_link`], with the kind of system given.
fn shaped(text: &str, windows: bool) -> bool {
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return false;
    }
    if windows && (is_drive_path(text) || is_share(text)) {
        return true;
    }
    if let Some(rest) = strip_prefix_ignoring_case(text, "mailto:") {
        return !rest.is_empty();
    }
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let mut letters = scheme.chars();
    letters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && letters.all(|letter| letter.is_ascii_alphanumeric() || "+-.".contains(letter))
        && !rest.is_empty()
}

/// `C:\` or `C:/`, and a name after it.
fn is_drive_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

/// `\\server`, a share of the network.
fn is_share(text: &str) -> bool {
    text.strip_prefix("\\\\")
        .is_some_and(|rest| rest.chars().next().is_some_and(|first| first != '\\'))
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// What follows a link in a sentence and is not part of it.
const STOPS: &str = ".,;:!?";

/// The brackets a link is written in.
const BRACKETS: [(char, char); 3] = [('(', ')'), ('<', '>'), ('[', ']')];

/// The part of a run of text with no blank in it that is a link, once what
/// stands around a link in a sentence is taken off; nothing when no part is.
///
/// A bracket is taken off an end while its partner stands at the other end or
/// nowhere, because an address may carry brackets of its own:
/// `…/Rust_(language)`. A quote is
/// taken off either end, since an address never ends with one.
fn link_in(run: &str, windows: bool) -> Option<Range<usize>> {
    let mut range = 0..run.len();
    loop {
        let text = &run[range.clone()];
        let first = text.chars().next()?;
        let last = text.chars().next_back()?;
        let count = |wanted: char| text.chars().filter(|letter| *letter == wanted).count();

        let around = BRACKETS
            .iter()
            .any(|(open, close)| first == *open && last == *close);
        let cut_end = around
            || STOPS.contains(last)
            || matches!(last, '"' | '\'')
            || BRACKETS
                .iter()
                .any(|(open, close)| last == *close && count(*close) > count(*open));
        let cut_start = around
            || matches!(first, '"' | '\'')
            || BRACKETS
                .iter()
                .any(|(open, close)| first == *open && count(*open) > count(*close));
        if !cut_end && !cut_start {
            break;
        }
        if cut_end {
            range.end -= last.len_utf8();
        }
        if cut_start && !range.is_empty() {
            range.start += first.len_utf8();
        }
    }
    shaped(&run[range.clone()], windows).then_some(range)
}

/// The two ends of the link a double-click at `point` lands on, if it lands on
/// one.
///
/// The run is followed across the rows a long line was wrapped onto, because an
/// address is longer than a row as often as not.
pub(crate) fn link_around<T>(term: &Term<T>, point: Point) -> Option<(Point, Point)> {
    let grid = term.grid();
    let last_column = term.last_column();
    let blank = |point: Point| {
        let cell = &grid[point];
        cell.c.is_whitespace() || cell.c == '\0'
    };
    let wrapped = |line| {
        grid[Point::new(line, last_column)]
            .flags
            .contains(Flags::WRAPLINE)
    };
    let before = |point: Point| match point.column.0 {
        0 if point.line > term.topmost_line() && wrapped(point.line - 1) => {
            Some(Point::new(point.line - 1, last_column))
        }
        0 => None,
        column => Some(Point::new(point.line, Column(column - 1))),
    };
    let after = |point: Point| match point.column == last_column {
        true if point.line < term.bottommost_line() && wrapped(point.line) => {
            Some(Point::new(point.line + 1, Column(0)))
        }
        true => None,
        false => Some(Point::new(point.line, point.column + 1)),
    };

    if blank(point) {
        return None;
    }
    let mut start = point;
    while let Some(earlier) = before(start).filter(|earlier| !blank(*earlier)) {
        start = earlier;
    }

    // The characters of the run, each with the cell it starts at; the cell
    // that only holds the second half of a wide character holds none.
    let mut run = String::new();
    let mut cells = Vec::new();
    let mut at = Some(start);
    while let Some(point) = at.filter(|point| !blank(*point)) {
        let cell = &grid[point];
        if !cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            cells.push((run.len(), point));
            run.push(cell.c);
        }
        at = after(point);
    }

    let range = link_in(&run, cfg!(windows))?;
    let first = cells.iter().find(|(offset, _)| *offset == range.start)?.1;
    let last = cells
        .iter()
        .rev()
        .find(|(offset, _)| *offset < range.end)?
        .1;
    Some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_with_a_scheme_is_a_link() {
        for text in [
            "https://example.com/a",
            "ssh://host",
            "svn+ssh://host/repo",
            "file:///etc/hosts",
            "mailto:somebody@example.com",
            "MAILTO:x",
        ] {
            assert!(shaped(text, false), "{text}");
        }
    }

    #[test]
    fn what_only_has_a_colon_is_not() {
        for text in [
            "",
            "key:value",
            "host:22",
            "https://",
            "mailto:",
            "1http://x",
            "ht tp://x",
            "https://example.com/a b",
            "/etc/hosts",
            "~/notes",
            "C:\\notes.txt",
            "\\\\server\\share",
        ] {
            assert!(!shaped(text, false), "{text:?}");
        }
    }

    #[test]
    fn a_path_of_windows_is_a_link_on_windows() {
        for text in ["C:\\notes.txt", "z:/notes.txt", "\\\\server\\share"] {
            assert!(shaped(text, true), "{text}");
        }
        for text in ["C:\\", "C:notes", "\\\\", "\\\\\\server"] {
            assert!(!shaped(text, true), "{text:?}");
        }
    }

    fn found(run: &str) -> Option<&str> {
        link_in(run, false).map(|range| &run[range])
    }

    #[test]
    fn what_stands_around_a_link_in_a_sentence_is_left_out() {
        assert_eq!(found("https://x.org/a."), Some("https://x.org/a"));
        assert_eq!(found("https://x.org/a)."), Some("https://x.org/a"));
        assert_eq!(found("(https://x.org/a),"), Some("https://x.org/a"));
        assert_eq!(found("<https://x.org/a>"), Some("https://x.org/a"));
        assert_eq!(found("\"https://x.org/a\""), Some("https://x.org/a"));
        assert_eq!(found("'https://x.org/a';"), Some("https://x.org/a"));
    }

    #[test]
    fn brackets_of_the_address_itself_are_kept() {
        assert_eq!(
            found("https://en.wikipedia.org/wiki/Rust_(language)"),
            Some("https://en.wikipedia.org/wiki/Rust_(language)")
        );
        assert_eq!(
            found("(https://en.wikipedia.org/wiki/Rust_(language))."),
            Some("https://en.wikipedia.org/wiki/Rust_(language)")
        );
    }

    #[test]
    fn a_run_that_is_no_link_has_none() {
        assert_eq!(found("key:value"), None);
        assert_eq!(found("...."), None);
        assert_eq!(found("\"\""), None);
    }
}
