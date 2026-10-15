//! The reply of `display -p` of [`MODES`]: what a program switched on in a
//! pane, which `capture-pane` does not carry.

use crate::error::{Error, Result};
use crate::id::number;

/// The formats `Command::Display` asks, in the order the reply gives them.
///
/// A tab cannot part them: tmux writes a control byte of a format as `_`, so
/// they go with spaces, which none of them holds.
pub const MODES: &[&str] = &[
    "pane_width",
    "pane_height",
    "history_size",
    "alternate_on",
    "alternate_saved_x",
    "alternate_saved_y",
    "cursor_x",
    "cursor_y",
    "scroll_region_upper",
    "scroll_region_lower",
    "origin_flag",
    "wrap_flag",
    "insert_flag",
    "keypad_cursor_flag",
    "keypad_flag",
    "cursor_flag",
    "mouse_standard_flag",
    "mouse_button_flag",
    "mouse_all_flag",
    "mouse_sgr_flag",
    "mouse_utf8_flag",
    "pane_tabs",
];

/// What tmux writes for a position that is none: `alternate_saved_x` of a
/// pane that was never on the alternate screen. One that left it keeps the
/// last cursor saved.
const NONE: u32 = u32::MAX;

/// The modes of a pane. Every position is inside the pane: tmux is the other
/// end of a line, and a number past the pane would move a cursor nowhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modes {
    /// Width of the pane, at least one.
    pub columns: u32,
    /// Height of the pane, at least one.
    pub rows: u32,
    /// Lines of history above the screen.
    pub history: u32,
    /// The alternate screen is on.
    pub alternate: bool,
    /// The cursor saved by `?1049h`, column and row; `None` off the alternate
    /// screen, whatever tmux kept of it.
    pub alternate_saved: Option<(u32, u32)>,
    /// The cursor, column and row.
    pub cursor: (u32, u32),
    /// The scroll region, its top and bottom rows; the whole screen when tmux
    /// gives one upside down.
    pub scroll_region: (u32, u32),
    /// `?6h`.
    pub origin: bool,
    /// `?7h`.
    pub wrap: bool,
    /// `4h`.
    pub insert: bool,
    /// `?1h`.
    pub keypad_cursor: bool,
    /// `ESC =`.
    pub keypad: bool,
    /// `?25h`.
    pub cursor_visible: bool,
    /// `?1000h`.
    pub mouse_standard: bool,
    /// `?1002h`.
    pub mouse_button: bool,
    /// `?1003h`.
    pub mouse_all: bool,
    /// `?1006h`.
    pub mouse_sgr: bool,
    /// `?1005h`.
    pub mouse_utf8: bool,
    /// Columns of the tab stops inside the pane; a stop past it is not kept.
    pub tabs: Vec<u32>,
}

impl Modes {
    /// Reads the one line of the reply.
    pub fn parse(line: &[u8]) -> Result<Self> {
        let fields: Vec<&[u8]> = line.split(|&byte| byte == b' ').collect();
        let &[
            columns,
            rows,
            history,
            alternate,
            saved_x,
            saved_y,
            cursor_x,
            cursor_y,
            upper,
            lower,
            origin,
            wrap,
            insert,
            keypad_cursor,
            keypad,
            cursor_visible,
            mouse_standard,
            mouse_button,
            mouse_all,
            mouse_sgr,
            mouse_utf8,
            tabs,
        ] = fields.as_slice()
        else {
            return Err(Error::MalformedModes);
        };
        let columns = value(columns)?.max(1);
        let rows = value(rows)?.max(1);
        let column = |field| value(field).map(|x: u32| x.min(columns - 1));
        let row = |field| value(field).map(|y: u32| y.min(rows - 1));
        let (saved_x, saved_y) = (value(saved_x)?, value(saved_y)?);
        let alternate = flag(alternate)?;
        let alternate_saved = (alternate && saved_x != NONE && saved_y != NONE)
            .then(|| (saved_x.min(columns - 1), saved_y.min(rows - 1)));
        let (upper, lower) = (row(upper)?, row(lower)?);
        let scroll_region = if upper < lower {
            (upper, lower)
        } else {
            (0, rows - 1)
        };
        let tabs = if tabs.is_empty() {
            Vec::new()
        } else {
            tabs.split(|&byte| byte == b',')
                .map(value)
                .collect::<Result<Vec<u32>>>()?
                .into_iter()
                .filter(|&stop| stop < columns)
                .collect()
        };
        Ok(Self {
            columns,
            rows,
            history: value(history)?,
            alternate,
            alternate_saved,
            cursor: (column(cursor_x)?, row(cursor_y)?),
            scroll_region,
            origin: flag(origin)?,
            wrap: flag(wrap)?,
            insert: flag(insert)?,
            keypad_cursor: flag(keypad_cursor)?,
            keypad: flag(keypad)?,
            cursor_visible: flag(cursor_visible)?,
            mouse_standard: flag(mouse_standard)?,
            mouse_button: flag(mouse_button)?,
            mouse_all: flag(mouse_all)?,
            mouse_sgr: flag(mouse_sgr)?,
            mouse_utf8: flag(mouse_utf8)?,
            tabs,
        })
    }
}

fn value(field: &[u8]) -> Result<u32> {
    number(field).ok_or(Error::MalformedModes)
}

fn flag(field: &[u8]) -> Result<bool> {
    match field {
        b"0" => Ok(false),
        b"1" => Ok(true),
        _ => Err(Error::MalformedModes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_of_tmux_reads() {
        let modes = Modes::parse(b"80 24 4 1 0 0 6 2 4 9 0 1 0 0 0 1 0 1 0 1 0 4,10").unwrap();
        assert_eq!(
            modes,
            Modes {
                columns: 80,
                rows: 24,
                history: 4,
                alternate: true,
                alternate_saved: Some((0, 0)),
                cursor: (6, 2),
                scroll_region: (4, 9),
                origin: false,
                wrap: true,
                insert: false,
                keypad_cursor: false,
                keypad: false,
                cursor_visible: true,
                mouse_standard: false,
                mouse_button: true,
                mouse_all: false,
                mouse_sgr: true,
                mouse_utf8: false,
                tabs: vec![4, 10],
            }
        );
        assert_eq!(MODES.len(), 22);
    }

    #[test]
    fn positions_are_brought_inside_the_pane() {
        let modes = Modes::parse(
            b"80 24 0 0 4294967295 4294967295 200 99 30 40 0 1 0 0 0 1 0 0 0 0 0 8,16,80,4000",
        )
        .unwrap();
        assert_eq!(modes.alternate_saved, None);
        assert_eq!(modes.cursor, (79, 23));
        assert_eq!(modes.scroll_region, (0, 23));
        assert_eq!(modes.tabs, [8, 16]);
        let modes = Modes::parse(b"80 24 0 0 3 4 0 0 0 23 0 1 0 0 0 1 0 0 0 0 0 ").unwrap();
        assert_eq!(
            modes.alternate_saved, None,
            "kept after the alternate screen was left"
        );
        let modes = Modes::parse(b"80 24 0 1 120 30 0 0 0 23 0 1 0 0 0 1 0 0 0 0 0 ").unwrap();
        assert_eq!(modes.alternate_saved, Some((79, 23)));
        assert!(modes.tabs.is_empty());
        let modes = Modes::parse(b"0 0 0 0 0 0 5 5 0 0 0 1 0 0 0 1 0 0 0 0 0 ").unwrap();
        assert_eq!((modes.columns, modes.rows, modes.cursor), (1, 1, (0, 0)));
    }

    #[test]
    fn a_reply_that_is_not_one_is_malformed() {
        for line in [
            &b""[..],
            b"80 24",
            b"80 24 0 1 0 0 0 0 4 9 0 1 0 0 0 1 0 1 0 1 0 8 extra",
            b"80 24 0 2 0 0 0 0 4 9 0 1 0 0 0 1 0 1 0 1 0 8",
            b"80 x 0 1 0 0 0 0 4 9 0 1 0 0 0 1 0 1 0 1 0 8",
            b"80 24 0 1 0 0 0 0 4 9 0 1 0 0 0 1 0 1 0 1 0 8,,16",
            b"80  24 0 1 0 0 0 0 4 9 0 1 0 0 0 1 0 1 0 1 0",
        ] {
            assert_eq!(
                Modes::parse(line),
                Err(Error::MalformedModes),
                "{}",
                String::from_utf8_lossy(line)
            );
        }
    }
}
