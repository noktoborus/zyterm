//! What the emulation of this program made of a stream somebody captured.
//!
//! A terminal is read by looking at it, and a layout that comes out wrong is
//! read by nobody: the pictures are in the way of the question, which is which
//! cell holds what. So the bytes are fed to `zyt_term` — the same emulation the
//! window draws from — and the page is printed as one character per cell.
//!
//! The stream is captured with `script`, which records what a program wrote and
//! answers what it asked:
//!
//! ```sh
//! script -q -c 'lss -ly' /tmp/lss.raw
//! sixel replay /tmp/lss.raw --cell 9x17 --grid 110x36
//! ```
//!
//! The cell and the grid are the ones the window had, because a picture takes
//! the cells its pixels come to and a program laid its page out against the
//! same two numbers. `lss --diag` says what a program was told; the window says
//! it in the settings.

use std::io::Write;
use zyt_term::{Terminal, TerminalConfig};

/// What one cell of the page is printed as.
///
/// A cell of a picture is the picture, so what it carries is not what is seen:
/// the first column of a row of one is `0`, the rest of that row `#`. The other
/// cells are the character itself, with a space written as a stop so the
/// columns can be counted.
fn glyph(cell: &zyt_term::Cell) -> char {
    match cell.image {
        Some(part) if part.column == 0 => '0',
        Some(_) => '#',
        None if cell.ch == ' ' => '.',
        None => cell.ch,
    }
}

/// Feeds the stream to the emulation and prints what the page came to.
pub fn replay(bytes: &[u8], cell: (usize, usize), grid: (usize, usize)) -> Result<(), String> {
    let mut terminal = Terminal::new(TerminalConfig {
        columns: grid.0,
        rows: grid.1,
        scrollback: 10_000,
        clipboard: zyt_term::ClipboardAccess::CopyPaste,
    })
    .map_err(|error| format!("a terminal of {}x{}: {error}", grid.0, grid.1))?;

    terminal.set_sixel_placeholder(Some(std::sync::Arc::new(
        |report: &zyt_term::SixelReport| match report.declared {
            Some((width, height)) => format!("<sixel {width}x{height}, {} B>", report.bytes),
            None => format!("<sixel ?, {} B>", report.bytes),
        },
    )));
    terminal.set_cell_size(cell.0, cell.1);
    terminal.set_default_colors(zyt_term::DefaultColors {
        foreground: zyt_term::Rgb {
            r: 0xd0,
            g: 0xd0,
            b: 0xd0,
        },
        background: zyt_term::Rgb {
            r: 0x28,
            g: 0x28,
            b: 0x28,
        },
        cursor: zyt_term::Rgb {
            r: 0xd0,
            g: 0xd0,
            b: 0xd0,
        },
    });

    terminal.feed(bytes);
    let answered = terminal.take_output();
    let content = terminal.content();

    let mut out = std::io::stdout().lock();
    let say = |out: &mut dyn Write, line: String| {
        let _ = writeln!(out, "{line}");
    };

    say(
        &mut out,
        format!(
            "stream            {} bytes, into a page of {}x{} cells of {}x{} pixels",
            bytes.len(),
            grid.0,
            grid.1,
            cell.0,
            cell.1
        ),
    );
    say(
        &mut out,
        format!("answered          {:?}", String::from_utf8_lossy(&answered)),
    );
    say(
        &mut out,
        format!(
            "scrolled off      {} lines into the scrollback",
            content.history_size
        ),
    );
    say(
        &mut out,
        format!("pictures on the page   {}", content.pictures.len()),
    );
    for picture in &content.pictures {
        let kept = terminal.picture(picture.id);
        let declared = kept
            .as_ref()
            .and_then(|kept| kept.report.declared)
            .map(|(width, height)| format!("{width}x{height}"))
            .unwrap_or_else(|| "?".to_string());
        say(
            &mut out,
            format!(
                "  {:?}  declared {declared}, read as {}x{} pixels, standing in {} columns x {} rows",
                picture.id,
                picture.pixels.width,
                picture.pixels.height,
                picture.cells.0,
                picture.cells.1
            ),
        );
    }
    say(&mut out, String::new());
    say(
        &mut out,
        "0 the first cell of a row of a picture, # the rest of that row".to_string(),
    );
    say(
        &mut out,
        ". a space, everything else the character".to_string(),
    );
    say(&mut out, String::new());

    let width = content.columns.to_string().len();
    for row in 0..content.rows {
        let line: String = (0..content.columns)
            .map(|column| content.cell(column, row).map(glyph).unwrap_or(' '))
            .collect();
        say(
            &mut out,
            format!("{row:>width$} |{}|", line.trim_end(), width = width),
        );
    }
    out.flush()
        .map_err(|error| format!("standard output: {error}"))
}
