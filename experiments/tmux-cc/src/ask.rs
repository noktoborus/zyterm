//! `ask`: run inside a pane, it asks the terminal it stands in — which is tmux —
//! and prints what came back.
//!
//! It is how `probe` learns what tmux answers a program: the queries go out on
//! standard output, the answers come in on standard input in raw mode, and the
//! result is printed as one line a question, `ASK <query> = <answer>`.

use std::io::{Read as _, Write as _};
use std::time::{Duration, Instant};

const QUERIES: [(&str, &[u8]); 5] = [
    ("DA1", b"\x1b[c"),
    ("DA2", b"\x1b[>c"),
    ("CSI14t", b"\x1b[14t"),
    ("CSI16t", b"\x1b[16t"),
    ("CSI18t", b"\x1b[18t"),
];

pub fn run() -> Result<(), String> {
    let size = crossterm::terminal::window_size().map_err(|error| error.to_string())?;
    crossterm::terminal::enable_raw_mode().map_err(|error| error.to_string())?;
    let (sender, bytes) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        while std::io::stdin()
            .read(&mut byte)
            .map(|count| count == 1)
            .unwrap_or(false)
        {
            if sender.send(byte[0]).is_err() {
                break;
            }
        }
    });
    let answers: Vec<(&str, Vec<u8>)> = QUERIES
        .iter()
        .map(|(name, query)| (*name, ask(query, &bytes)))
        .collect();
    crossterm::terminal::disable_raw_mode().map_err(|error| error.to_string())?;
    for (name, answer) in answers {
        println!("ASK {name} = {}", crate::record::escape(&answer));
    }
    println!(
        "ASK winsize = {}x{} cells, {}x{} pixels",
        size.columns, size.rows, size.width, size.height
    );
    Ok(())
}

/// Writes one query and reads what comes back within half a second.
///
/// One thread reads standard input for all the queries: a reader per query
/// would still be waiting on its read when the next answer came, and take it.
fn ask(query: &[u8], receiver: &std::sync::mpsc::Receiver<u8>) -> Vec<u8> {
    let mut out = std::io::stdout();
    let _ = out.write_all(query);
    let _ = out.flush();
    let end = Instant::now() + Duration::from_millis(500);
    let mut answer = Vec::new();
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(left) {
            Ok(byte) => answer.push(byte),
            Err(_) => break,
        }
    }
    answer
}
