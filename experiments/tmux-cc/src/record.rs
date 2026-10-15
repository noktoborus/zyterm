//! Recordings: what went to tmux and what came back, kept as text a test reads
//! and a person can.
//!
//! ```text
//! # tmux-cc record attach, tmux 3.3a
//! < \eP1000p%begin 1792046023 244 0\r\n%end 1792046023 244 0\r\n
//! > list-sessions -F '#{session_id} #{session_name}'\n
//! ```
//!
//! A line is one chunk as it went: `<` read from tmux, `>` written to it. A
//! byte below `0x20`, `0x7F` and `\` are written as `\r`, `\n`, `\t`, `\e`, `\\`
//! or `\xHH`; every other byte, UTF-8 included, as it is. The chunks are as the
//! pty gave them, so a replay meets the boundaries a real one meets.

use std::path::{Path, PathBuf};

use crate::scenario;
use crate::tmux::{self, Tmux, Way};

/// The scenarios kept beside the program.
fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

/// Where the recordings go unless told otherwise: the data of the tests of
/// `zyt-tmux`, which replays them.
fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/zyt-tmux/tests/data")
}

/// `record <scenario|all> [--tmux <binary>]... [--out <directory>]`.
pub fn run(args: &[&str]) -> Result<(), String> {
    let mut names = Vec::new();
    let mut binaries = Vec::new();
    let mut out = data_dir();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match *word {
            "--tmux" => binaries.push(words.next().ok_or("--tmux <binary>")?.to_string()),
            "--out" => out = PathBuf::from(words.next().ok_or("--out <directory>")?),
            "all" => names.extend(all_scenarios()?),
            name => names.push(name.to_string()),
        }
    }
    if names.is_empty() {
        return Err("record <scenario|all>".into());
    }
    if binaries.is_empty() {
        binaries = tmux::installed();
    }
    std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;

    for binary in &binaries {
        let version = tmux::version(binary).ok_or(format!("{binary}: no such tmux"))?;
        for name in &names {
            let path = scenarios_dir().join(format!("{name}.txt"));
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let steps = scenario::parse(&text).map_err(|error| format!("{name}: {error}"))?;
            if let Some(wanted) = scenario::needs(&steps)
                && !scenario::at_least(&version, wanted)
            {
                println!("{name:<10} {version:<5} skipped: needs {wanted}");
                continue;
            }
            let mut tmux = Tmux::start(binary, &tmux::socket(name))?;
            let outcome = scenario::play(&steps, &mut tmux);
            let file = out.join(format!("{name}-{version}.txt"));
            std::fs::write(&file, write(name, &version, &tmux))
                .map_err(|error| format!("{}: {error}", file.display()))?;
            match outcome {
                Ok(()) => println!("{name:<10} {version:<5} {}", file.display()),
                Err(error) => println!("{name:<10} {version:<5} FAILED: {error}"),
            }
        }
    }
    Ok(())
}

fn all_scenarios() -> Result<Vec<String>, String> {
    let dir = scenarios_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    Ok(names)
}

/// The text of a recording.
fn write(name: &str, version: &str, tmux: &Tmux) -> String {
    let mut text = format!("# tmux-cc record {name}, tmux {version}\n");
    for chunk in &tmux.log {
        text.push_str(match chunk.way {
            Way::Sent => "> ",
            Way::Said => "< ",
        });
        text.push_str(&escape(&chunk.bytes));
        text.push('\n');
    }
    text
}

/// The bytes of a chunk as one line of a recording.
pub fn escape(bytes: &[u8]) -> String {
    let mut out = Vec::new();
    for &byte in bytes {
        match byte {
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x1b => out.extend_from_slice(b"\\e"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            byte if byte < 0x20 || byte == 0x7f => {
                out.extend_from_slice(format!("\\x{byte:02x}").as_bytes())
            }
            byte => out.push(byte),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `show <file>`: a recording to be read, the lines of what tmux said
/// numbered, its control bytes named, the start and end of the mode marked,
/// what was sent between them.
pub fn show(file: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|error| format!("{file}: {error}"))?;
    let mut said = Vec::new();
    let mut number = 0;
    let flush = |said: &mut Vec<u8>, all: bool, number: &mut usize| {
        while let Some(end) =
            said.iter()
                .position(|&byte| byte == b'\n')
                .or(if all && !said.is_empty() {
                    Some(said.len() - 1)
                } else {
                    None
                })
        {
            let line: Vec<u8> = said.drain(..=end).collect();
            *number += 1;
            println!("{number:5}  {}", named(&line));
        }
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("< ") {
            said.extend(scenario::unescape(rest));
            flush(&mut said, false, &mut number);
        } else if let Some(rest) = line.strip_prefix("> ") {
            flush(&mut said, true, &mut number);
            println!("    →  {}", rest.trim_end_matches("\\n"));
        } else {
            println!("{line}");
        }
    }
    flush(&mut said, true, &mut number);
    Ok(())
}

/// A line of what tmux said, its control bytes named.
fn named(line: &[u8]) -> String {
    let mut text = String::new();
    let mut rest = line;
    while !rest.is_empty() {
        if rest.starts_with(b"\x1bP1000p") {
            text.push_str("〔mode starts〕");
            rest = &rest[7..];
            continue;
        }
        if rest.starts_with(b"\x1b\\") {
            text.push_str("〔ESC \\〕");
            rest = &rest[2..];
            continue;
        }
        let (character, length) = match std::str::from_utf8(&rest[..rest.len().min(4)]) {
            Ok(text) => (
                text.chars().next(),
                text.chars().next().map_or(1, char::len_utf8),
            ),
            Err(error) if error.valid_up_to() > 0 => {
                let text = std::str::from_utf8(&rest[..error.valid_up_to()]).unwrap_or("");
                (
                    text.chars().next(),
                    text.chars().next().map_or(1, char::len_utf8),
                )
            }
            Err(_) => (None, 1),
        };
        match character {
            Some('\r') => {}
            Some('\n') => {}
            Some('\x1b') => text.push_str("‹ESC›"),
            Some(c) if (c as u32) < 0x20 || c == '\x7f' => {
                text.push_str(&format!("‹{:02x}›", c as u32))
            }
            Some(c) => text.push(c),
            None => text.push_str(&format!("‹{:02x}›", rest[0])),
        }
        rest = &rest[length..];
    }
    text
}
