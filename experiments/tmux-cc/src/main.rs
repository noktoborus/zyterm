//! A real tmux in control mode, and what it says.
//!
//! The protocol of `tmux -CC` is written down from what tmux does, not from
//! what its manual says it does, so this runs one — through `zyt-pty`, the way
//! the program starts a console — on a server of its own, and keeps every byte.
//!
//! - `record <scenario>` runs a scenario and writes the bytes both ways into
//!   the data of the tests of `zyt-tmux`;
//! - `probe` asks the questions the design of control mode rests on;
//! - `show <file>` prints a recording to be read;
//! - `ask` is run inside a pane by `probe`: it asks the terminal it stands in
//!   and prints the answers.
//!
//! tmux has no build for Windows, so neither has this.

#[cfg(unix)]
mod ask;
#[cfg(unix)]
mod probe;
#[cfg(unix)]
mod record;
#[cfg(unix)]
mod scenario;
#[cfg(unix)]
mod tmux;

#[cfg(unix)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = match words.as_slice() {
        ["record", rest @ ..] => record::run(rest),
        ["probe", rest @ ..] => probe::run(rest),
        ["show", file] => record::show(file),
        ["ask"] => ask::run(),
        _ => Err(USAGE.to_string()),
    };
    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("tmux has no build for this platform");
    std::process::exit(1);
}

#[cfg(unix)]
const USAGE: &str = "\
usage:
  tmux-cc record <scenario|all> [--tmux <binary>]... [--out <directory>]
  tmux-cc probe [--tmux <binary>]...
  tmux-cc show <file>";
