//! Commands of tmux: what a reply answers, and the line that asks for it.

use std::io::Write as _;

use crate::id::{PaneId, SessionId, WindowId};
use crate::modes::MODES;

/// What a reply answers: ids only, `Copy`, so the queue of what was sent
/// allocates nothing once it stands at its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// The pair that opens the mode, with flags `0`: never sent, never queued.
    /// Its lines carry the reason when the start failed: `no sessions`.
    Opening,
    /// `display -p` of [`MODES`] for a pane, joined by spaces.
    Display(PaneId),
    /// `capture-pane` of a part of a pane.
    Capture {
        /// The pane.
        pane: PaneId,
        /// What of it.
        part: Part,
    },
    /// `list-sessions`: `$S name` a line.
    ListSessions,
    /// `list-windows -a`: `$S @W index active bell %P name` a line, `%P` the
    /// active pane, `bell` the flag of a bell rung in it while not current.
    ListWindows,
    /// `list-panes -a`: `$S @W %P window-index pane-index command` a line.
    ListPanes,
    /// `switch-client`.
    SwitchClient(SessionId),
    /// `select-window`.
    SelectWindow(WindowId),
    /// `new-window` in a session.
    NewWindow(SessionId),
    /// `kill-window`.
    KillWindow(WindowId),
    /// `kill-pane`.
    KillPane(PaneId),
    /// `new-session -t`: a session of the group of this one, sharing its
    /// windows with a current window of its own; the client goes to it.
    Group(SessionId),
    /// `set destroy-unattached on` of the session of the client: it goes
    /// once no client is on it.
    DestroyUnattached,
    /// `resize-pane -Z` of a pane: zoomed, or unzoomed when its window is.
    /// The pane zoomed is made the active one of tmux.
    Zoom(PaneId),
    /// `resize-pane -Z` of a window: unzoomed when it is zoomed.
    Unzoom(WindowId),
    /// `select-pane -Z`: in a window zoomed, the zoom moves to the pane, made
    /// active; tmux 3.1 and later.
    ZoomOver(PaneId),
    /// `resize-pane -x` or `-y` of a pane: the cell it is in made this wide,
    /// or this high.
    ResizePane {
        /// The pane.
        pane: PaneId,
        /// `-x`, its width; `-y`, its height.
        side_by_side: bool,
        /// Columns, or rows.
        cells: u16,
    },
    /// `split-window` of a pane: a new pane beside it, or below it.
    Split {
        /// The pane split.
        pane: PaneId,
        /// `-h`, the panes side by side; `-v`, one above the other.
        side_by_side: bool,
    },
    /// `refresh-client -C`: how large this client is.
    ClientSize {
        /// Columns.
        columns: u16,
        /// Rows.
        rows: u16,
    },
    /// `detach`: the session stays.
    Detach,
    /// `display -p '#{version}'`.
    Version,
    /// `display -p` of the version, the size tmux gave the window shown and
    /// how many clients the session has: `3.3a 120x40 2`.
    WindowInfo,
    /// `refresh-client -B` of the bell flag of every window of the session,
    /// named [`BELLS`]: `%subscription-changed` says each change; tmux 3.2 and
    /// later.
    Bells,
    /// `refresh-client -f pause-after=N`, or `-f '!pause-after'` for none;
    /// tmux 3.2 and later.
    PauseAfter(Option<u32>),
    /// `refresh-client -A '%P:pause'` or `continue`; tmux 3.2 and later.
    PaneFlow {
        /// The pane.
        pane: PaneId,
        /// Paused or let go.
        flow: Flow,
    },
    /// One `send-keys`, written by [`Client::send_keys`](crate::Client::send_keys).
    SendKeys(PaneId),
    /// `rename-window`, written by
    /// [`Client::rename_window`](crate::Client::rename_window).
    RenameWindow(WindowId),
}

/// The name of the subscription of [`Command::Bells`].
pub const BELLS: &str = "zyterm-bells";

/// What of a pane `capture-pane` takes. Every part is taken with `-p -e -J`:
/// colours, and wrapped lines joined with their trailing spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The visible screen.
    Visible,
    /// The normal screen under the alternate one (`-a -q`): empty when there
    /// is none.
    Saved,
    /// `depth` lines of history and the visible screen, one range (`-S -K`).
    Range {
        /// Lines of history above the screen.
        depth: u32,
    },
    /// `depth` lines of history alone (`-S -K -E -1`): on the alternate
    /// screen, where a range would end with the alternate screen.
    History {
        /// Lines of history above the screen.
        depth: u32,
    },
}

/// A pane paused or let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// `pause`: what is queued for it is thrown away.
    Pause,
    /// `continue`.
    Continue,
}

/// Appends the line of `command`, with no newline. `false` for a command
/// with no line of its own: [`Command::Opening`], [`Command::SendKeys`],
/// [`Command::RenameWindow`].
pub(crate) fn write(command: Command, out: &mut Vec<u8>) -> bool {
    let written = match command {
        Command::Opening | Command::SendKeys(_) | Command::RenameWindow(_) => return false,
        Command::Display(pane) => {
            let formats: Vec<String> = MODES.iter().map(|name| format!("#{{{name}}}")).collect();
            write!(out, "display -p -t {pane} '{}'", formats.join(" "))
        }
        Command::Capture { pane, part } => {
            out.extend_from_slice(b"capture-pane -p -e -J ");
            match part {
                Part::Visible => Ok(()),
                Part::Saved => write!(out, "-a -q "),
                Part::Range { depth } => write!(out, "-S -{depth} "),
                Part::History { depth } => write!(out, "-S -{depth} -E -1 "),
            }
            .and_then(|()| write!(out, "-t {pane}"))
        }
        Command::ListSessions => {
            write!(out, "list-sessions -F '#{{session_id}} #{{session_name}}'")
        }
        Command::ListWindows => write!(
            out,
            "list-windows -a -F '#{{session_id}} #{{window_id}} #{{window_index}} \
             #{{window_active}} #{{window_bell_flag}} #{{pane_id}} #{{window_name}}'"
        ),
        Command::ListPanes => write!(
            out,
            "list-panes -a -F '#{{session_id}} #{{window_id}} #{{pane_id}} #{{window_index}} \
             #{{pane_index}} #{{pane_current_command}}'"
        ),
        Command::SwitchClient(session) => write!(out, "switch-client -t {session}"),
        Command::SelectWindow(window) => write!(out, "select-window -t {window}"),
        Command::NewWindow(session) => write!(out, "new-window -t {session}"),
        Command::KillWindow(window) => write!(out, "kill-window -t {window}"),
        Command::KillPane(pane) => write!(out, "kill-pane -t {pane}"),
        Command::Group(session) => write!(out, "new-session -t '{session}'"),
        Command::DestroyUnattached => write!(out, "set destroy-unattached on"),
        Command::Zoom(pane) => write!(out, "resize-pane -Z -t {pane}"),
        Command::Unzoom(window) => write!(out, "resize-pane -Z -t {window}"),
        Command::ZoomOver(pane) => write!(out, "select-pane -Z -t {pane}"),
        Command::ResizePane {
            pane,
            side_by_side,
            cells,
        } => {
            let side = if side_by_side { "-x" } else { "-y" };
            write!(out, "resize-pane -t {pane} {side} {cells}")
        }
        Command::Split { pane, side_by_side } => {
            let side = if side_by_side { "-h" } else { "-v" };
            write!(out, "split-window {side} -t {pane}")
        }
        Command::ClientSize { columns, rows } => write!(out, "refresh-client -C {columns}x{rows}"),
        Command::Detach => write!(out, "detach"),
        Command::Version => write!(out, "display -p '#{{version}}'"),
        Command::Bells => write!(out, "refresh-client -B '{BELLS}:@*:#{{window_bell_flag}}'"),
        Command::WindowInfo => write!(
            out,
            "display -p '#{{version}} #{{window_width}}x#{{window_height}} #{{session_attached}}'"
        ),
        Command::PauseAfter(Some(seconds)) => {
            write!(out, "refresh-client -f pause-after={seconds}")
        }
        Command::PauseAfter(None) => write!(out, "refresh-client -f '!pause-after'"),
        Command::PaneFlow { pane, flow } => {
            let flow = match flow {
                Flow::Pause => "pause",
                Flow::Continue => "continue",
            };
            write!(out, "refresh-client -A '{pane}:{flow}'")
        }
    };
    written.expect("a write into a Vec");
    true
}

/// A piece of input for a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keys<'a> {
    /// Bytes as typed: text, control bytes, escape sequences, sent as they are.
    Bytes(&'a [u8]),
    /// A key by its name in tmux — `Home`, `C-Up`, `F5` — which tmux encodes as
    /// the terminal of the pane expects it. A name with a control byte, or an
    /// empty one, is not sent.
    Named(&'a str),
}

/// One run of input, and how it goes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Run<'a> {
    /// Printable text, by `-l`.
    Text(&'a [u8]),
    /// Anything else, by `-H`.
    Hex(&'a [u8]),
    /// A key by its name.
    Named(&'a str),
}

/// Cuts pieces of input into runs: bytes as [`runs`] cuts them, a name as a
/// run of its own.
pub(crate) fn runs_of<'a>(keys: &[Keys<'a>]) -> Vec<Run<'a>> {
    let mut all = Vec::new();
    for keys in keys {
        match *keys {
            Keys::Bytes(bytes) => all.extend(runs(bytes)),
            Keys::Named(name) if !name.is_empty() && !name.chars().any(char::is_control) => {
                all.push(Run::Named(name));
            }
            Keys::Named(_) => {}
        }
    }
    all
}

/// Cuts input into runs: printable text — ASCII `0x20`–`0x7E` and valid UTF-8
/// past it — goes by `-l`; control bytes, an escape sequence whole with its
/// printable tail, bytes that are not UTF-8, and a run of text of one byte go
/// by `-H`. By `-l` the `ESC` of a sequence would be the key Escape.
pub(crate) fn runs(bytes: &[u8]) -> Vec<Run<'_>> {
    let mut spans: Vec<(usize, usize, bool)> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let (length, text) = unit(&bytes[at..]);
        match spans.last_mut() {
            Some((_, end, kind)) if *kind == text => *end = at + length,
            _ => spans.push((at, at + length, text)),
        }
        at += length;
    }
    let mut merged: Vec<(usize, usize, bool)> = Vec::with_capacity(spans.len());
    for (start, end, text) in spans {
        let text = text && end - start > 1;
        match merged.last_mut() {
            Some((_, last_end, false)) if !text => *last_end = end,
            _ => merged.push((start, end, text)),
        }
    }
    merged
        .into_iter()
        .map(|(start, end, text)| {
            if text {
                Run::Text(&bytes[start..end])
            } else {
                Run::Hex(&bytes[start..end])
            }
        })
        .collect()
}

/// The length of the unit at the start of `bytes`, and whether it is
/// printable text.
fn unit(bytes: &[u8]) -> (usize, bool) {
    match bytes[0] {
        0x1b => (escape_length(bytes), false),
        0x20..=0x7e => (1, true),
        0x00..=0x1f | 0x7f => (1, false),
        lead => {
            let length = match lead {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return (1, false),
            };
            match bytes.get(..length).map(std::str::from_utf8) {
                Some(Ok(text)) if !text.chars().any(char::is_control) => (length, true),
                _ => (1, false),
            }
        }
    }
}

/// The length of the escape sequence at the start of `bytes`: CSI to its final
/// byte, SS3 and the rest with the byte after them, OSC and DCS to their end;
/// a sequence cut short is what there is of it.
fn escape_length(bytes: &[u8]) -> usize {
    match bytes.get(1) {
        None => 1,
        Some(b'[') => bytes[2..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map_or(bytes.len(), |at| at + 3),
        Some(b']' | b'P' | b'_' | b'^' | b'X') => string_end(bytes),
        Some(b'O' | b'(' | b')' | b'#' | b' ') => bytes.len().min(3),
        Some(_) => 2,
    }
}

/// The length of a string sequence to its `BEL` or `ESC \`.
fn string_end(bytes: &[u8]) -> usize {
    let mut at = 2;
    while at < bytes.len() {
        match bytes[at] {
            0x07 => return at + 1,
            0x1b if bytes.get(at + 1) == Some(&b'\\') => return at + 2,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// Appends one `send-keys` of a run. Text goes after `--`: tmux reads the
/// options after the quotes are gone, so a piece of input that begins with
/// `-` would be a flag, refused, and the keys lost.
pub(crate) fn write_run(pane: PaneId, run: &Run<'_>, out: &mut Vec<u8>) {
    write!(out, "send-keys -t {pane} ").expect("a write into a Vec");
    match run {
        Run::Text(text) => {
            out.extend_from_slice(b"-l -- ");
            quote(text, out);
        }
        Run::Hex(bytes) => {
            out.extend_from_slice(b"-H");
            for byte in *bytes {
                write!(out, " {byte:02x}").expect("a write into a Vec");
            }
        }
        Run::Named(name) => {
            out.extend_from_slice(b"-- ");
            quote(name.as_bytes(), out);
        }
    }
}

/// Appends `rename-window` with the name quoted. `rename-window` expands
/// formats in its argument, so a `#` is written `##`; control bytes of the
/// name are left out, since a newline would end the command.
pub(crate) fn write_rename(window: WindowId, name: &str, out: &mut Vec<u8>) {
    write!(out, "rename-window -t {window} -- ").expect("a write into a Vec");
    let mut escaped = Vec::with_capacity(name.len());
    for byte in name.bytes().filter(|byte| !byte.is_ascii_control()) {
        if byte == b'#' {
            escaped.push(b'#');
        }
        escaped.push(byte);
    }
    quote(&escaped, out);
}

/// Appends `text` in single quotes, a `'` inside written `'\''`. Inside them
/// the parser of tmux expands nothing: `;`, `~` and `$` stay what they are.
fn quote(text: &[u8], out: &mut Vec<u8>) {
    out.push(b'\'');
    for &byte in text {
        if byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(byte);
        }
    }
    out.push(b'\'');
}
