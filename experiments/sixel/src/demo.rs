//! Ways a program puts a picture on a page, each written out as the bytes a
//! terminal gets.
//!
//! A payload says what a picture is; where it goes on the page is the program's
//! doing, and every program does it differently. These are the ways worth
//! knowing, each of them a few bytes around the same payload: the picture
//! alone, text beside it, a row of thumbnails, an icon inside a sentence, a
//! page that scrolls under a picture, text written over one. What they are for
//! is reading the terminal against them — a window narrowed over any of these
//! says more about the terminal than a payload on its own does.
//!
//! The cell is the one number a program cannot work out for itself, so the
//! layouts here take it as it is given (`--cell WxH`) and a terminal is asked
//! for it with `CSI 16 t`.

use crate::pattern::test_pattern;
use std::io::{Read, Write};

/// One way of putting a picture on a page.
pub struct Layout {
    /// What it is called on the command line.
    pub name: &'static str,
    /// One line about what it shows.
    pub about: &'static str,
    /// The bytes of it, for a cell of that many pixels.
    pub write: fn(cell: (usize, usize)) -> String,
}

/// Every layout, in the order they are worth reading: the plainest first.
pub const LAYOUTS: &[Layout] = &[
    Layout {
        name: "plain",
        about: "a picture of its own, with a line before it and a line after",
        write: plain,
    },
    Layout {
        name: "beside",
        about: "text to the right of a picture, the way a file listing writes it",
        write: beside,
    },
    Layout {
        name: "row",
        about: "thumbnails side by side on one band of rows, each with a caption",
        write: row,
    },
    Layout {
        name: "inline",
        about: "a picture the height of one line, inside a sentence",
        write: inline,
    },
    Layout {
        name: "sizes",
        about: "the same pattern at five widths, each saying what it came to",
        write: sizes,
    },
    Layout {
        name: "over",
        about: "text written where a picture stands, with no cursor move of its own",
        write: over,
    },
    Layout {
        name: "scroll",
        about: "pictures printed one after another until the page scrolls under them",
        write: scroll,
    },
    Layout {
        name: "noscroll",
        about: "the display mode on: the picture at the top left, the cursor left where it was",
        write: noscroll,
    },
    Layout {
        name: "cut",
        about: "the display mode on and a picture taller than the page, cut at the bottom",
        write: cut,
    },
    Layout {
        name: "corner",
        about: "what noscroll comes to, written with cursor moves and no mode",
        write: corner,
    },
];

/// Where the display mode layouts leave the cursor before the picture: well
/// inside the page, so the corner and the cursor are two places.
const CURSOR: (usize, usize) = (12, 30);

/// What is written at the cursor after the picture, to show where it stood.
const AFTER: &str = "<- the cursor after the picture";

/// The layout of that name.
pub fn layout(name: &str) -> Option<&'static Layout> {
    LAYOUTS.iter().find(|layout| layout.name == name)
}

/// Shows every layout, one at a time, stepping on a press of the space bar.
///
/// It is the way the layouts are meant to be read: the same window, the same
/// font, one layout after another, so what differs between them is the layout
/// and nothing else. A window resized while one of them stands is the question
/// they are all written for.
///
/// The terminal is put into raw mode, because what is read is one press and not
/// a line: nothing else about the terminal is changed, and the mode is given
/// back whatever happens. The layouts carry their own carriage returns, so they
/// read the same either way.
pub fn walk(cell: (usize, usize)) -> std::io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let outcome = step(cell);
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = std::io::stdout().write_all(b"\r\n");
    outcome
}

/// Draws one layout and waits for what to do next.
///
/// The space bar steps on and comes back to the first after the last; `q`, the
/// escape and `^C` leave. Everything else is drawn again, so a key nobody meant
/// to press costs a frame and nothing more.
fn step(cell: (usize, usize)) -> std::io::Result<()> {
    let mut input = std::io::stdin().lock();
    let mut buffer = [0_u8; 32];
    let mut at = 0;

    loop {
        show(&LAYOUTS[at], at, cell)?;

        let read = input.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        for byte in &buffer[..read] {
            match byte {
                b' ' => at = (at + 1) % LAYOUTS.len(),
                b'q' | 0x1b | 0x03 => return Ok(()),
                _ => {}
            }
        }
    }
}

/// Clears the page, says which layout this is, and writes it.
fn show(layout: &Layout, at: usize, cell: (usize, usize)) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(b"\x1b[2J\x1b[H")?;
    out.write_all(
        format!(
            "{}/{} {} \u{2014} {}\r\n{}\r\n\r\n",
            at + 1,
            LAYOUTS.len(),
            layout.name,
            layout.about,
            "space: the next one, q: leave"
        )
        .as_bytes(),
    )?;
    out.write_all((layout.write)(cell).as_bytes())?;
    out.flush()
}

/// A picture of its own.
///
/// The cursor comes to rest where the picture began, on its last row, so the
/// line after it is written by a program that moves on itself: the newline is
/// the program's and not the picture's.
fn plain(cell: (usize, usize)) -> String {
    let mut out = String::from("a line of text above the picture\r\n");
    out.push_str(&test_pattern(cell.0 * 8, cell.1 * 3));
    out.push_str("\r\n");
    out.push_str("a line of text below it\r\n");
    out
}

/// Text to the right of a picture.
///
/// Room is made with line feeds, the cursor goes back up, the picture is drawn
/// between a save and a restore, and every line of the caption steps past the
/// picture with `CSI n C`. Nothing here depends on where the terminal leaves
/// the cursor after a picture, which is why a file listing is written this way.
fn beside(cell: (usize, usize)) -> String {
    let rows = 4;
    let wide = 9;
    let mut out = String::new();
    for file in 0..3 {
        out.push_str(&"\n".repeat(rows));
        out.push_str(&format!("\u{1b}[{rows}A"));
        out.push_str("\u{1b}7");
        out.push_str(&test_pattern(cell.0 * wide, cell.1 * rows - 2));
        out.push_str("\u{1b}8");
        for line in 0..rows {
            out.push_str(&format!("\u{1b}[{}C", wide + 1));
            out.push_str(&format!(
                "file {file}, line {line} of what is known about it"
            ));
            out.push_str("\r\n");
        }
    }
    out
}

/// Thumbnails side by side, each with a caption under its own.
///
/// The band is as tall as the pictures. Every thumbnail is drawn where the
/// cursor stands, the cursor is put back, and the next one begins one picture
/// to the right: a grid of pictures is a row of these with nothing between
/// them.
fn row(cell: (usize, usize)) -> String {
    let rows = 4;
    let wide = 8;
    let mut out = String::from("three thumbnails on one band of rows\r\n");
    out.push_str(&"\n".repeat(rows + 1));
    out.push_str(&format!("\u{1b}[{}A", rows + 1));

    for _ in 0..3 {
        out.push_str("\u{1b}7");
        out.push_str(&test_pattern(cell.0 * wide, cell.1 * rows - 3));
        out.push_str("\u{1b}8");
        out.push_str(&format!("\u{1b}[{}C", wide + 2));
    }
    out.push_str("\r\n");
    out.push_str(&format!("\u{1b}[{rows}B"));
    for column in 0..3 {
        out.push_str(&format!("name {column}"));
        out.push_str(&format!("\u{1b}[{}C", wide + 2 - 6));
    }
    out.push_str("\r\n");
    out
}

/// A picture the height of one line, inside a sentence.
///
/// It is the hardest of the layouts on a terminal and the one an icon of a
/// prompt is: the picture is one row tall, the text before it leaves the cursor
/// in the middle of the line, and what comes after it has to step past it.
fn inline(cell: (usize, usize)) -> String {
    let wide = 2;
    let mut out = String::from("an icon ");
    out.push_str(&test_pattern(cell.0 * wide, cell.1 - 1));
    out.push_str(&format!("\u{1b}[{wide}C"));
    out.push_str(" in the middle of a sentence, and the sentence goes on\r\n");
    out
}

/// The same pattern at five widths.
///
/// Each of them says what it was asked for, so what the terminal made of it is
/// read against the line beside it: a picture is as many cells as its pixels
/// come to, rounded up, and the last row and column of it are part empty.
fn sizes(cell: (usize, usize)) -> String {
    let mut out = String::new();
    for cells in [1, 2, 4, 8, 16] {
        let (width, height) = (cell.0 * cells, cell.1 * cells / 2 + 1);
        out.push_str(&format!("{cells:2} cells wide, {width}x{height} pixels: "));
        out.push_str(&test_pattern(width, height));
        out.push_str("\r\n\r\n");
    }
    out
}

/// Text written where a picture stands.
///
/// The cursor is where the picture began, so the text lands on the picture and
/// takes those cells from it: a cell written into is a cell that is text again.
/// It is what a program that forgets to move the cursor does, and what a
/// terminal has to survive.
fn over(cell: (usize, usize)) -> String {
    let mut out = String::from("the next picture has text written into it\r\n");
    out.push_str(&test_pattern(cell.0 * 10, cell.1 * 3));
    out.push_str("TEXT OVER THE PICTURE");
    out.push_str("\r\n\r\n");
    out
}

/// Pictures printed one after another until the page scrolls.
///
/// A picture is output the way text is: it scrolls off the top, and what the
/// scrollback keeps of it is what is scrolled back to. Ten of them is more than
/// a page of any ordinary window.
fn scroll(cell: (usize, usize)) -> String {
    let mut out = String::new();
    for number in 0..10 {
        out.push_str(&format!("picture {number} of 10\r\n"));
        out.push_str(&test_pattern(cell.0 * 6, cell.1 * 2));
        out.push_str("\r\n");
    }
    out
}

/// The display mode on, `CSI ? 80 h`.
///
/// With it on, a picture is drawn from the top left corner of the page,
/// nothing scrolls, and the cursor stays where it was. So the line written
/// after it stands where the cursor was put, and the picture covers the
/// heading. The mode is put off again at the end, so the page after this one
/// is drawn as before.
fn noscroll(cell: (usize, usize)) -> String {
    let mut out = format!("\u{1b}[{};{}H\u{1b}[?80h", CURSOR.0, CURSOR.1);
    out.push_str(&test_pattern(cell.0 * 10, cell.1 * 3));
    out.push_str(AFTER);
    out.push_str("\u{1b}[?80l");
    out
}

/// The display mode on and a picture taller than any ordinary page.
///
/// With the mode on the page does not scroll: the picture is cut at the
/// bottom row, and the line written after it stands where the cursor was put.
/// With the mode off the page scrolls under the picture.
fn cut(cell: (usize, usize)) -> String {
    let mut out = format!("\u{1b}[{};{}H\u{1b}[?80h", CURSOR.0, CURSOR.1);
    out.push_str(&test_pattern(cell.0 * 10, cell.1 * 100));
    out.push_str(AFTER);
    out.push_str("\u{1b}[?80l");
    out
}

/// What `noscroll` comes to, written with no mode at all.
///
/// The cursor is saved, put in the top left corner for the picture, and put
/// back. A page that draws `noscroll` the same as this one keeps the mode; a
/// page that draws it elsewhere does not.
fn corner(cell: (usize, usize)) -> String {
    let mut out = format!("\u{1b}[{};{}H\u{1b}7\u{1b}[H", CURSOR.0, CURSOR.1);
    out.push_str(&test_pattern(cell.0 * 10, cell.1 * 3));
    out.push_str("\u{1b}8");
    out.push_str(AFTER);
    out
}
