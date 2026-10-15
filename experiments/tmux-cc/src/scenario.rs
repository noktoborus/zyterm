//! A scenario: a short list of steps kept as a file, so a new case is a new
//! file and not new code.
//!
//! ```text
//! # a comment
//! needs 3.4                  skipped on an older tmux
//! send <command line>        written to tmux as it stands
//! type <pane> <text>         send-keys -H of the text
//! until <text>               read until tmux said it, or fail
//! wait <ms>                  read for that long
//! hold <ms>                  read nothing for that long, so tmux falls behind
//! ```
//!
//! Text after `type` and `until` takes `\r`, `\n`, `\e`, `\\` and `\xHH`, and
//! `{tmux}` and `{socket}` for the binary and the server of the run. Every
//! scenario ends with `detach`, added here.

use std::time::Duration;

use crate::tmux::{self, PATIENCE, Tmux};

/// One step of a scenario.
pub enum Step {
    Needs(String),
    Send(String),
    Type(String, String),
    Until(String),
    Wait(u64),
    Hold(u64),
}

/// The steps of a scenario file.
pub fn parse(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let step = match word {
            "needs" => Step::Needs(rest.trim().to_string()),
            "send" => Step::Send(rest.to_string()),
            "type" => {
                let (pane, text) = rest
                    .split_once(' ')
                    .ok_or(format!("line {}: type <pane> <text>", number + 1))?;
                Step::Type(pane.to_string(), text.to_string())
            }
            "until" => Step::Until(rest.to_string()),
            "wait" => Step::Wait(
                rest.trim()
                    .parse()
                    .map_err(|_| format!("line {}: wait <ms>", number + 1))?,
            ),
            "hold" => Step::Hold(
                rest.trim()
                    .parse()
                    .map_err(|_| format!("line {}: hold <ms>", number + 1))?,
            ),
            other => return Err(format!("line {}: no step `{other}`", number + 1)),
        };
        steps.push(step);
    }
    Ok(steps)
}

/// The oldest version a scenario asks for, if it asks.
pub fn needs(steps: &[Step]) -> Option<&str> {
    steps.iter().find_map(|step| match step {
        Step::Needs(version) => Some(version.as_str()),
        _ => None,
    })
}

/// Runs the steps on `tmux`, then leaves the mode.
pub fn play(steps: &[Step], tmux: &mut Tmux) -> Result<(), String> {
    for step in steps {
        match step {
            Step::Needs(_) => {}
            Step::Send(line) => {
                let line = fill(line, tmux);
                tmux.send(&line);
                tmux.pump(Duration::from_millis(150));
            }
            Step::Type(pane, text) => {
                let bytes = unescape(&fill(text, tmux));
                tmux.type_bytes(pane, &bytes);
                tmux.pump(Duration::from_millis(150));
            }
            Step::Until(text) => {
                let needle = unescape(&fill(text, tmux));
                if !tmux.until(&needle, PATIENCE) {
                    return Err(format!("{}: never said {text:?}", tmux.binary()));
                }
            }
            Step::Wait(ms) => tmux.pump(Duration::from_millis(*ms)),
            Step::Hold(ms) => tmux.hold(Duration::from_millis(*ms)),
        }
    }
    tmux.detach();
    Ok(())
}

fn fill(text: &str, tmux: &Tmux) -> String {
    text.replace("{tmux}", tmux.binary())
        .replace("{socket}", tmux.socket())
}

/// `\r`, `\n`, `\e`, `\\` and `\xHH` to their bytes.
pub fn unescape(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && at + 1 < bytes.len() {
            match bytes[at + 1] {
                b'r' => out.push(b'\r'),
                b'n' => out.push(b'\n'),
                b'e' => out.push(0x1b),
                b'\\' => out.push(b'\\'),
                b'x' if at + 3 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[at + 2..at + 4]).unwrap_or("");
                    if let Ok(byte) = u8::from_str_radix(hex, 16) {
                        out.push(byte);
                        at += 4;
                        continue;
                    }
                    out.extend_from_slice(b"\\x");
                }
                other => out.extend_from_slice(&[b'\\', other]),
            }
            at += 2;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    out
}

/// Whether `version` is at least `wanted`.
pub fn at_least(version: &str, wanted: &str) -> bool {
    tmux::ordered(version) >= tmux::ordered(wanted)
}
