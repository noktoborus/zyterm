//! The picture, drawn with half blocks so a terminal can show it without
//! knowing anything about sixel.
//!
//! One character carries two pixels — the upper half is the foreground and the
//! lower half the background — so a picture drawn this way is as wide as it is
//! in pixels and half as tall. It is not what the terminal will do with a
//! payload; it is how this tool is read.

use crate::pixels;
use sixel_image::SixelImage;
use std::io::Write;

/// The upper half of a cell, `UPPER HALF BLOCK`.
const HALF: char = '\u{2580}';

/// Draws the picture, two rows of pixels per row of text.
pub fn draw(image: &SixelImage, out: &mut impl Write) -> std::io::Result<()> {
    let rows = image.pixels.len();

    for pair in (0..rows).step_by(2) {
        let upper = &image.pixels[pair];
        let lower = image.pixels.get(pair + 1);
        let width = upper.len().max(lower.map(|row| row.len()).unwrap_or(0));

        for column in 0..width {
            let above = upper
                .get(column)
                .filter(|pixel| pixel.on)
                .and_then(|pixel| pixels::color_of(image, pixel.color));
            let below = lower
                .and_then(|row| row.get(column))
                .filter(|pixel| pixel.on)
                .and_then(|pixel| pixels::color_of(image, pixel.color));

            match (above, below) {
                (None, None) => write!(out, "\x1b[0m ")?,
                (Some(above), None) => write!(
                    out,
                    "\x1b[0m\x1b[38;2;{};{};{}m{HALF}",
                    above[0], above[1], above[2]
                )?,
                (None, Some(below)) => write!(
                    out,
                    "\x1b[0m\x1b[48;2;{};{};{}m ",
                    below[0], below[1], below[2]
                )?,
                (Some(above), Some(below)) => write!(
                    out,
                    "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m{HALF}",
                    above[0], above[1], above[2], below[0], below[1], below[2]
                )?,
            }
        }
        writeln!(out, "\x1b[0m")?;
    }
    Ok(())
}
