//! The state of a terminal that a program set and the cells do not show:
//! the modes, the character sets, the shape of the cursor and the title.
//!
//! A terminal that joins a stream late gets it as bytes
//! ([`TermState::write_bytes`]) before the stream, and then reads the stream
//! the way the first terminal did. Only what differs from a new terminal is
//! written, and `?1007`, which a new terminal of another program may have
//! off.
//!
//! Not kept: the pen, the colours, the place of the cursor, whether it is
//! shown, the scroll region, the tab stops and which of G0 and G1 is active.
//! The last three are private to the backend.

use alacritty_terminal::Term;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CharsetIndex, CursorShape, CursorStyle, StandardCharset};

/// The modes kept, each with the sequences that set it and reset it, in the
/// order they are written. The alternate screen is first: entering it clears
/// it and keeps the cursor, and the modes after it hold on either screen.
const MODES: [(TermMode, &[u8], &[u8]); 15] = [
    (TermMode::ALT_SCREEN, b"\x1b[?1049h", b"\x1b[?1049l"),
    (TermMode::APP_CURSOR, b"\x1b[?1h", b"\x1b[?1l"),
    (TermMode::APP_KEYPAD, b"\x1b=", b"\x1b>"),
    (TermMode::ORIGIN, b"\x1b[?6h", b"\x1b[?6l"),
    (TermMode::LINE_WRAP, b"\x1b[?7h", b"\x1b[?7l"),
    (TermMode::INSERT, b"\x1b[4h", b"\x1b[4l"),
    (TermMode::LINE_FEED_NEW_LINE, b"\x1b[20h", b"\x1b[20l"),
    (TermMode::MOUSE_REPORT_CLICK, b"\x1b[?1000h", b"\x1b[?1000l"),
    (TermMode::MOUSE_DRAG, b"\x1b[?1002h", b"\x1b[?1002l"),
    (TermMode::MOUSE_MOTION, b"\x1b[?1003h", b"\x1b[?1003l"),
    (TermMode::FOCUS_IN_OUT, b"\x1b[?1004h", b"\x1b[?1004l"),
    (TermMode::UTF8_MOUSE, b"\x1b[?1005h", b"\x1b[?1005l"),
    (TermMode::SGR_MOUSE, b"\x1b[?1006h", b"\x1b[?1006l"),
    (TermMode::ALTERNATE_SCROLL, b"\x1b[?1007h", b"\x1b[?1007l"),
    (TermMode::BRACKETED_PASTE, b"\x1b[?2004h", b"\x1b[?2004l"),
];

/// The mode written whether or not it differs from a new terminal: it is on
/// in one here and may be off in the terminal that reads the bytes, and a
/// mode that is on leaves no trace of who set it.
const ALWAYS: TermMode = TermMode::ALTERNATE_SCROLL;

/// The designation of each of G0 to G3 to the line drawing set.
const LINE_DRAWING: [&[u8]; 4] = [b"\x1b(0", b"\x1b)0", b"\x1b*0", b"\x1b+0"];

/// The state of a terminal that a program set and the cells do not show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermState {
    /// The modes of [`MODES`] as they stand.
    modes: TermMode,
    /// G0 to G3 that hold the line drawing set, not ASCII.
    line_drawing: [bool; 4],
    /// The parameter of `DECSCUSR`, when the shape differs from that of a new
    /// terminal.
    cursor: Option<u8>,
    /// The title a program set.
    title: Option<String>,
}

impl TermState {
    /// The state of `term`, with the title the terminal kept.
    pub(crate) fn of<T>(term: &Term<T>, title: Option<&str>) -> TermState {
        let kept = MODES
            .iter()
            .fold(TermMode::empty(), |kept, mode| kept | mode.0);
        let charsets = &term.grid().cursor.charsets;
        let indexes = [
            CharsetIndex::G0,
            CharsetIndex::G1,
            CharsetIndex::G2,
            CharsetIndex::G3,
        ];
        TermState {
            modes: *term.mode() & kept,
            line_drawing: indexes
                .map(|index| charsets[index] == StandardCharset::SpecialCharacterAndLineDrawing),
            cursor: shape_parameter(term.cursor_style()),
            title: title.map(str::to_string),
        }
    }

    /// Appends to `into` the bytes that give a new terminal this state: what
    /// differs from a new terminal, and `?1007` always.
    pub fn write_bytes(&self, into: &mut Vec<u8>) {
        let new = TermMode::default();
        for (flag, set, reset) in MODES {
            let on = self.modes.contains(flag);
            if on != new.contains(flag) || flag == ALWAYS {
                into.extend_from_slice(if on { set } else { reset });
            }
        }
        for (designation, held) in LINE_DRAWING.iter().zip(self.line_drawing) {
            if held {
                into.extend_from_slice(designation);
            }
        }
        if let Some(parameter) = self.cursor {
            into.extend_from_slice(format!("\x1b[{parameter} q").as_bytes());
        }
        if let Some(title) = &self.title {
            into.extend_from_slice(format!("\x1b]2;{title}\x1b\\").as_bytes());
        }
    }
}

/// The parameter of `DECSCUSR` for a style, or nothing for the style of a
/// new terminal and for a shape no program sets.
fn shape_parameter(style: CursorStyle) -> Option<u8> {
    if style == CursorStyle::default() {
        return None;
    }
    let steady = match style.shape {
        CursorShape::Block => 2,
        CursorShape::Underline => 4,
        CursorShape::Beam => 6,
        CursorShape::HollowBlock | CursorShape::Hidden => return None,
    };
    Some(if style.blinking { steady - 1 } else { steady })
}

#[cfg(test)]
mod tests {
    use crate::{Terminal, TerminalConfig};

    /// A terminal of the default size.
    fn terminal() -> Terminal {
        Terminal::new(TerminalConfig::default()).expect("the default size is valid")
    }

    /// A new terminal writes `?1007` alone.
    #[test]
    fn a_new_terminal_writes_alternate_scroll_alone() {
        let mut bytes = Vec::new();
        terminal().state().write_bytes(&mut bytes);
        assert_eq!(bytes, b"\x1b[?1007h");
    }

    /// The bytes of a state, fed to a new terminal, give it the same state.
    #[test]
    fn the_bytes_of_a_state_give_the_same_state() {
        let cases: [&[u8]; 9] = [
            b"\x1b[?1049h\x1b[?1h\x1b=\x1b[?2004h",
            b"\x1b[?1000h\x1b[?1006h\x1b[?1004h",
            b"\x1b[?1002h\x1b[?1005h",
            b"\x1b[?1003h",
            b"\x1b[?7l\x1b[4h\x1b[20h\x1b[?6h",
            b"\x1b[?1007l",
            b"\x1b(0\x1b)0\x1b+0",
            b"\x1b[5 q",
            b"\x1b]2;a\x01 title\x07",
        ];
        for case in cases {
            let mut first = terminal();
            first.feed(case);
            let state = first.state();
            assert_ne!(state, terminal().state(), "{case:?} changes the state");

            let mut bytes = Vec::new();
            state.write_bytes(&mut bytes);
            let mut second = terminal();
            second.feed(&bytes);
            assert_eq!(second.state(), state, "{case:?}");
        }
    }

    /// A state undone by the program is the state of a new terminal; a reset
    /// title is no title.
    #[test]
    fn a_state_undone_is_the_new_state() {
        let mut changed = terminal();
        changed.feed(b"\x1b[?1049h\x1b[?1000h\x1b[3 q\x1b]2;t\x07");
        changed.feed(b"\x1b[?1049l\x1b[?1000l\x1b[0 q\x1b]2;\x07");
        assert_eq!(changed.state(), terminal().state());
    }

    /// The pen, the colours and whether the cursor is shown are not kept.
    #[test]
    fn the_pen_the_colours_and_the_cursor_shown_are_not_kept() {
        let mut changed = terminal();
        changed.feed(b"\x1b[1;31m\x1b]4;1;rgb:12/34/56\x07\x1b[?25l");
        assert_eq!(changed.state(), terminal().state());
    }
}
