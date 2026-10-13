//! What a sixel payload is, what it would cost to draw, and what it looks like.
//!
//! The questions this answers are the ones the terminal has to answer before it
//! can show a picture: how many pixels a payload really carries, how many cells
//! that is, how many coloured rectangles the picture comes to once equal
//! neighbours are merged — which is what deciding between a texture and plain
//! geometry turns on — and whether the crate reads a payload the way the
//! standard says it does.

mod blocks;
mod demo;
mod pattern;
mod pixels;
mod replay;

use sixel_image::SixelDeserializer;
use sixel_tokenizer::{Parser, SixelEvent};
use std::io::{Read, Write};
use std::time::Instant;

/// Cell the number of columns and rows is counted against when none is given.
const CELL: (usize, usize) = (8, 17);

/// Page a stream is replayed into when none is given.
const GRID: (usize, usize) = (80, 24);

fn main() {
    let mut arguments = std::env::args().skip(1);
    // Nothing asked for is the viewer: the layouts are what this tool has to
    // show, and reading a payload is what it is told to do.
    let command = arguments.next().unwrap_or_else(|| "demo".to_string());
    let rest: Vec<String> = arguments.collect();

    let outcome = match command.as_str() {
        "make" => make(&rest),
        "demo" => write_demo(&rest),
        "replay" => read_replay(&rest),
        "show" => show(&rest, false),
        "blocks" => show(&rest, true),
        "-h" | "--help" | "help" => {
            usage();
            Ok(())
        }
        other => Err(format!("no command called {other}; try help")),
    };

    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn usage() {
    println!("sixel                 every way of putting pictures on a page,");
    println!("                      one press of the space bar apart");
    println!("sixel demo [name]     one of them; demo list names them");
    println!("sixel replay [file]   what this emulation made of a captured stream");
    println!("sixel show [file]     what the payload is and what it would cost");
    println!("sixel blocks [file]   the same, and the picture in half blocks");
    println!("sixel make [w] [h]    a test pattern, to standard output");
    println!();
    println!("No file, or -, reads standard input.");
    println!("--cell WxH counts the cells against that size instead of 8x17.");
    println!("--grid CxR replays into a page of that many cells instead of 80x24.");
}

/// Writes a test pattern, for feeding a terminal something known.
fn make(arguments: &[String]) -> Result<(), String> {
    let number = |index: usize, fallback: usize| {
        arguments
            .get(index)
            .map(|text| {
                text.parse::<usize>()
                    .map_err(|_| format!("not a size: {text}"))
            })
            .unwrap_or(Ok(fallback))
    };
    let width = number(0, 128)?;
    let height = number(1, 96)?;

    let payload = pattern::test_pattern(width, height);
    std::io::stdout()
        .write_all(payload.as_bytes())
        .map_err(|error| format!("standard output: {error}"))
}

/// Shows every layout, or writes one of them.
///
/// With no name it is the viewer: the layouts in the window being read, one
/// press of the space bar apart. With a name the bytes of that one go to
/// standard output as they stand, which is how a layout is captured or piped.
/// The cell is the one number a program cannot work out for itself, so a
/// terminal whose cell is not the default is told with `--cell WxH`.
fn write_demo(arguments: &[String]) -> Result<(), String> {
    let (name, cell) = read_arguments(arguments)?;
    let name = match name.as_deref() {
        None => {
            return demo::walk(cell).map_err(|error| {
                format!(
                    "{error}; the viewer wants a terminal, or name one layout: {}",
                    names()
                )
            });
        }
        Some("list") => {
            println!("sixel demo            every layout, one press of the space bar apart");
            println!("sixel demo <name>     one of them, to standard output:");
            for layout in demo::LAYOUTS {
                println!("  {:7}  {}", layout.name, layout.about);
            }
            println!();
            println!("--cell WxH lays the pictures out against that cell instead of");
            println!(
                "{}x{}; a terminal answers CSI 16 t with its own.",
                CELL.0, CELL.1
            );
            return Ok(());
        }
        Some(name) => name.to_string(),
    };

    let layout =
        demo::layout(&name).ok_or_else(|| format!("no layout called {name}; try demo list"))?;
    let bytes = (layout.write)(cell);
    let mut out = std::io::stdout();
    out.write_all(bytes.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|error| format!("standard output: {error}"))
}

/// Feeds a captured stream to the emulation and prints the page.
///
/// What is captured is what a program wrote and what the terminal answered it,
/// which `script -q -c '<program>' <file>` records; replayed against the cell
/// and the grid the window had, it says which cell of the page holds what. It
/// is how a layout that comes out wrong is read: a picture in the way of the
/// question is a picture nobody can look past.
fn read_replay(arguments: &[String]) -> Result<(), String> {
    let (path, cell, grid) = read_replay_arguments(arguments)?;
    let bytes = read_payload(path.as_deref())?;
    replay::replay(&bytes, cell, grid)
}

/// Every layout by name, for an error that has to say what there is.
fn names() -> String {
    demo::LAYOUTS
        .iter()
        .map(|layout| layout.name)
        .collect::<Vec<&str>>()
        .join(", ")
}

/// Reads a payload and says what it is.
fn show(arguments: &[String], picture: bool) -> Result<(), String> {
    let (path, cell) = read_arguments(arguments)?;
    let bytes = read_payload(path.as_deref())?;

    let started = Instant::now();
    let read = parse(&bytes)?;
    let decoding = started.elapsed();

    let (height, width) = read.image.pixel_size();
    let counted = pixels::count(&read.image);

    println!("payload           {} bytes", bytes.len());
    match read.raster {
        Some((pan, pad, ph, pv)) => println!(
            "raster attribute  {pan}:{pad} aspect, {} x {} declared",
            size_or_nothing(ph),
            size_or_nothing(pv)
        ),
        None => println!("raster attribute  none, so the size is the extent of the data"),
    }
    println!("pixels            {width} x {height}");
    println!(
        "cells at {}x{}      {} columns x {} rows",
        cell.0,
        cell.1,
        width.div_ceil(cell.0),
        height.div_ceil(cell.1)
    );
    println!(
        "colour registers  {} ({} of them painted by the payload)",
        read.image.color_registers.len(),
        read.colors
    );
    println!(
        "pixels drawn      {} of {} ({} left transparent)",
        counted.on,
        counted.total,
        counted.total - counted.on
    );
    println!(
        "runs              {} horizontal runs of one colour, {:.1} pixels each",
        counted.runs,
        counted.on as f32 / counted.runs.max(1) as f32
    );
    println!("as a texture      {} bytes of RGBA", counted.total * 4);
    println!("decoded in        {decoding:.1?}");

    if picture {
        println!();
        blocks::draw(&read.image, &mut std::io::stdout().lock())
            .map_err(|error| format!("standard output: {error}"))?;
    }
    Ok(())
}

/// The file to read and the cell the columns and rows are counted against.
fn read_arguments(arguments: &[String]) -> Result<(Option<String>, (usize, usize)), String> {
    let mut path = None;
    let mut cell = CELL;
    let mut rest = arguments.iter();

    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--cell" => {
                let value = rest.next().ok_or("--cell wants a size like 8x17")?;
                cell = parse_cell(value)?;
            }
            "-" => path = None,
            other if other.starts_with("--") => return Err(format!("no option called {other}")),
            other => path = Some(other.to_string()),
        }
    }
    Ok((path, cell))
}

/// What a replay was asked for: the stream, the cell and the page.
type Replay = (Option<String>, (usize, usize), (usize, usize));

/// The same arguments as the rest, and the size of the page to replay into.
fn read_replay_arguments(arguments: &[String]) -> Result<Replay, String> {
    let mut path = None;
    let mut cell = CELL;
    let mut grid = GRID;
    let mut rest = arguments.iter();

    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--cell" => {
                let value = rest.next().ok_or("--cell wants a size like 8x17")?;
                cell = parse_cell(value)?;
            }
            "--grid" => {
                let value = rest.next().ok_or("--grid wants a size like 110x36")?;
                grid = parse_cell(value)?;
            }
            "-" => path = None,
            other if other.starts_with("--") => return Err(format!("no option called {other}")),
            other => path = Some(other.to_string()),
        }
    }
    Ok((path, cell, grid))
}

fn parse_cell(text: &str) -> Result<(usize, usize), String> {
    let (width, height) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("not a cell size: {text}"))?;
    let number = |value: &str| {
        value
            .parse::<usize>()
            .ok()
            .filter(|size| *size > 0)
            .ok_or_else(|| format!("not a cell size: {text}"))
    };
    Ok((number(width)?, number(height)?))
}

fn read_payload(path: Option<&str>) -> Result<Vec<u8>, String> {
    match path {
        Some(path) => std::fs::read(path).map_err(|error| format!("{path}: {error}")),
        None => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|error| format!("standard input: {error}"))?;
            Ok(bytes)
        }
    }
}

/// What one pass over a payload read out of it.
struct Payload {
    image: sixel_image::SixelImage,
    /// The raster attribute the payload declared, when it declared one.
    raster: Option<(usize, usize, Option<usize>, Option<usize>)>,
    /// How many colour registers the payload itself painted.
    colors: usize,
}

/// Reads a payload the way the terminal will have to read it: the tokenizer
/// byte by byte, the deserializer told about every event.
///
/// The raster attribute is taken off the events rather than out of the image,
/// because the crate keeps it to itself and it is what a refused payload is
/// described by: a placeholder says the size the payload declared, and reading
/// that attribute builds no image.
fn parse(bytes: &[u8]) -> Result<Payload, String> {
    let mut parser = Parser::new();
    let mut deserializer = SixelDeserializer::new();
    let mut raster = None;
    let mut colors = 0;
    let mut failed = None;

    for byte in bytes {
        parser.advance(byte, |event| {
            match event {
                SixelEvent::RasterAttribute { pan, pad, ph, pv } => {
                    raster = Some((pan, pad, ph, pv))
                }
                SixelEvent::ColorIntroducer {
                    color_coordinate_system: Some(_),
                    ..
                } => colors += 1,
                _ => {}
            }
            if let Err(error) = deserializer.handle_event(event) {
                failed = failed.or(Some(error));
            }
        });
    }

    if let Some(error) = failed {
        return Err(format!("the payload is not sixel: {error}"));
    }
    let image = deserializer
        .create_image()
        .map_err(|error| format!("the payload is not sixel: {error}"))?;

    Ok(Payload {
        image,
        raster,
        colors,
    })
}

fn size_or_nothing(size: Option<usize>) -> String {
    match size {
        Some(size) => size.to_string(),
        None => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pattern this tool writes is read back as the size it declared, so
    /// what the crate makes of a payload is held to something written without
    /// it.
    #[test]
    fn a_pattern_is_read_back_at_the_size_it_declared() {
        for (width, height) in [(64, 36), (13, 7), (8, 6), (128, 97)] {
            let payload = pattern::test_pattern(width, height);
            let read = parse(payload.as_bytes()).expect("the pattern is sixel");

            assert_eq!(
                read.raster,
                Some((1, 1, Some(width), Some(height))),
                "{width}x{height}: the raster attribute of the payload"
            );
            let (read_height, read_width) = read.image.pixel_size();
            assert_eq!(
                (read_width, read_height),
                (width, height.div_ceil(pattern::BAND) * pattern::BAND),
                "{width}x{height}: the pixels the payload drew, rounded up to whole bands"
            );
        }
    }

    /// A payload of something else is an error and not a picture of nothing.
    #[test]
    fn what_is_not_sixel_is_refused() {
        assert!(parse(b"not a payload at all").is_err());
    }
}
