//! One line of the protocol, read where it stands.

use memchr::{memchr, memmem};

use crate::id::{PaneId, SessionId, WindowId, number};

/// A notification of tmux other than output: rare, and free to allocate.
#[derive(Debug, PartialEq, Eq)]
pub enum Notice<'a> {
    /// `%layout-change`: the layout of a window, and what of it is seen.
    Layout {
        /// The window.
        window: WindowId,
        /// Every pane of the window, as `layout::parse` reads it.
        layout: &'a [u8],
        /// The panes seen: one pane over the whole window when it is zoomed.
        /// `None` from a tmux that does not say.
        visible: Option<&'a [u8]>,
        /// The window is zoomed.
        zoomed: bool,
    },
    /// `%window-add`: a window of the session of this client.
    WindowAdded(WindowId),
    /// `%window-close`.
    WindowClosed(WindowId),
    /// `%window-renamed`.
    WindowRenamed(WindowId, &'a str),
    /// `%session-window-changed`: the current window of a session.
    SessionWindowChanged(SessionId, WindowId),
    /// `%window-pane-changed`: the active pane of a window.
    PaneChanged(WindowId, PaneId),
    /// `%session-changed`: this client is on another session.
    SessionChanged(SessionId, &'a str),
    /// `%sessions-changed`: a session made or gone.
    SessionsChanged,
    /// `%session-renamed`.
    SessionRenamed(SessionId, &'a str),
    /// `%unlinked-window-add`: a window of another session.
    UnlinkedWindowAdded(WindowId),
    /// `%unlinked-window-close`; also how tmux before 3.8 says a window of the
    /// session shown closed.
    UnlinkedWindowClosed(WindowId),
    /// `%unlinked-window-renamed`.
    UnlinkedWindowRenamed(WindowId, &'a str),
    /// `%pause`: tmux 3.2 and later.
    Paused(PaneId),
    /// `%continue`.
    Continued(PaneId),
    /// `%exit`, with its reason when it has one.
    Exit(Option<&'a str>),
    /// `%subscription-changed` of a subscription of windows: its name, the
    /// window, and the value of its format. tmux 3.2 and later.
    SubscriptionChanged {
        /// The name the subscription was made with.
        name: &'a str,
        /// The window the value is of.
        window: WindowId,
        /// The format, expanded for the window.
        value: &'a str,
    },
    /// A `%` word this crate does not know: skipped, not an error.
    Unknown(&'a [u8]),
}

/// The head of a reply: `%begin`, `%end`, `%error` and their three numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Guard {
    /// The time and the number, which `%end` repeats from its `%begin`.
    pub(crate) stamp: (u64, u64),
    /// `1` for a command of this client.
    pub(crate) flags: u32,
}

/// One line, its `\r\n` taken off.
#[derive(Debug)]
pub(crate) enum Line<'a> {
    Begin(Guard),
    End(Guard),
    Error(Guard),
    /// `%output` or `%extended-output`; its bytes, still escaped, are the
    /// line from `start` on.
    Output {
        pane: PaneId,
        age: Option<u32>,
        start: usize,
    },
    Notice(Notice<'a>),
    /// A line that is no `%` line: a line of a reply.
    Text,
    /// A `%` line that cannot be read.
    Malformed,
}

impl Line<'_> {
    /// A line tmux writes outside a reply only, known by its whole head; an
    /// unknown `%` word may be a line of a screen.
    pub(crate) fn is_notification(&self) -> bool {
        match self {
            Line::Begin(_) | Line::Output { .. } => true,
            Line::Notice(notice) => !matches!(notice, Notice::Unknown(_)),
            Line::End(_) | Line::Error(_) | Line::Text | Line::Malformed => false,
        }
    }
}

/// Reads a line; `line` has no `\n` and no `\r` at its end.
pub(crate) fn parse(line: &[u8]) -> Line<'_> {
    if line.first() != Some(&b'%') {
        return Line::Text;
    }
    let word_end = memchr(b' ', line).unwrap_or(line.len());
    let word = &line[..word_end];
    let rest_start = (word_end + 1).min(line.len());
    let rest = &line[rest_start..];
    match &word[1..] {
        b"output" => output(rest, rest_start, false),
        b"extended-output" => output(rest, rest_start, true),
        b"begin" => guard(rest).map_or(Line::Malformed, Line::Begin),
        b"end" => guard(rest).map_or(Line::Malformed, Line::End),
        b"error" => guard(rest).map_or(Line::Malformed, Line::Error),
        _ => notice(word, rest).map_or(Line::Malformed, Line::Notice),
    }
}

/// `%output %P bytes` and `%extended-output %P age … : bytes`; `offset` is
/// where `rest` stands in the line.
fn output(rest: &[u8], offset: usize, extended: bool) -> Line<'static> {
    let id_end = memchr(b' ', rest).unwrap_or(rest.len());
    let Some(pane) = PaneId::parse(&rest[..id_end]) else {
        return Line::Malformed;
    };
    let after = (id_end + 1).min(rest.len());
    if !extended {
        return Line::Output {
            pane,
            age: None,
            start: offset + after,
        };
    }
    let tail = &rest[after..];
    let Some(colon) = memmem::find(tail, b" : ") else {
        return Line::Malformed;
    };
    let Some(age) = tail[..colon]
        .split(|&byte| byte == b' ')
        .next()
        .and_then(number)
    else {
        return Line::Malformed;
    };
    Line::Output {
        pane,
        age: Some(age),
        start: offset + after + colon + 3,
    }
}

/// `time number flags`.
fn guard(rest: &[u8]) -> Option<Guard> {
    let mut words = words(rest);
    let time = long(words.next()?)?;
    let count = long(words.next()?)?;
    let flags = number(words.next()?)?;
    words.next().is_none().then_some(Guard {
        stamp: (time, count),
        flags,
    })
}

fn notice<'a>(word: &'a [u8], rest: &'a [u8]) -> Option<Notice<'a>> {
    let notice = match &word[1..] {
        b"layout-change" => {
            let mut words = words(rest);
            let window = WindowId::parse(words.next()?)?;
            let layout = words.next()?;
            let visible = words.next();
            let zoomed = words.next().is_some_and(|flags| flags.contains(&b'Z'));
            Notice::Layout {
                window,
                layout,
                visible,
                zoomed,
            }
        }
        b"window-add" => Notice::WindowAdded(one(rest, WindowId::parse)?),
        b"window-close" => Notice::WindowClosed(one(rest, WindowId::parse)?),
        b"window-renamed" => {
            let (window, name) = named(rest)?;
            Notice::WindowRenamed(WindowId::parse(window)?, name)
        }
        b"unlinked-window-add" => Notice::UnlinkedWindowAdded(one(rest, WindowId::parse)?),
        b"unlinked-window-close" => Notice::UnlinkedWindowClosed(one(rest, WindowId::parse)?),
        b"unlinked-window-renamed" => {
            let (window, name) = named(rest)?;
            Notice::UnlinkedWindowRenamed(WindowId::parse(window)?, name)
        }
        b"session-window-changed" => {
            let (session, window) = two(rest)?;
            Notice::SessionWindowChanged(SessionId::parse(session)?, WindowId::parse(window)?)
        }
        b"window-pane-changed" => {
            let (window, pane) = two(rest)?;
            Notice::PaneChanged(WindowId::parse(window)?, PaneId::parse(pane)?)
        }
        b"session-changed" => {
            let (session, name) = named(rest)?;
            Notice::SessionChanged(SessionId::parse(session)?, name)
        }
        b"session-renamed" => {
            let (session, name) = named(rest)?;
            Notice::SessionRenamed(SessionId::parse(session)?, name)
        }
        b"sessions-changed" => Notice::SessionsChanged,
        b"subscription-changed" => subscription(rest).unwrap_or(Notice::Unknown(word)),
        b"pause" => Notice::Paused(one(rest, PaneId::parse)?),
        b"continue" => Notice::Continued(one(rest, PaneId::parse)?),
        b"exit" => {
            let reason = std::str::from_utf8(rest).ok()?.trim_end();
            Notice::Exit((!reason.is_empty()).then_some(reason))
        }
        _ => Notice::Unknown(word),
    };
    Some(notice)
}

/// `name $S @W index pane : value`, the value after ` : ` with its spaces;
/// a subscription of a session or a pane, with no window, is passed over.
fn subscription(rest: &[u8]) -> Option<Notice<'_>> {
    let at = memmem::find(rest, b" : ")?;
    let (head, value) = (&rest[..at], &rest[at + 3..]);
    let mut words = words(head);
    let name = std::str::from_utf8(words.next()?).ok()?;
    SessionId::parse(words.next()?)?;
    let window = WindowId::parse(words.next()?)?;
    Some(Notice::SubscriptionChanged {
        name,
        window,
        value: std::str::from_utf8(value).ok()?,
    })
}

/// The words of a line, the spaces between and after them dropped.
fn words(rest: &[u8]) -> impl Iterator<Item = &[u8]> {
    rest.split(|&byte| byte == b' ')
        .filter(|word| !word.is_empty())
}

/// The one identifier a line carries.
fn one<Id>(rest: &[u8], parse: fn(&[u8]) -> Option<Id>) -> Option<Id> {
    let mut words = words(rest);
    let id = parse(words.next()?)?;
    words.next().is_none().then_some(id)
}

/// The two words a line carries.
fn two(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut words = words(rest);
    let pair = (words.next()?, words.next()?);
    words.next().is_none().then_some(pair)
}

/// An identifier and the name after it, spaces of the name kept.
fn named(rest: &[u8]) -> Option<(&[u8], &str)> {
    let (id, name) = match rest.iter().position(|&byte| byte == b' ') {
        Some(at) => (&rest[..at], &rest[at + 1..]),
        None => (rest, &[][..]),
    };
    Some((id, std::str::from_utf8(name).ok()?))
}

fn long(word: &[u8]) -> Option<u64> {
    if word.is_empty() || !word.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(word).ok()?.parse().ok()
}

#[cfg(test)]
mod tests;
