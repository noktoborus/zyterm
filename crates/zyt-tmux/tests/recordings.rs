//! Every recorded session of a real tmux, replayed through the splitter and
//! the client, whole and in chunks of every kind.
//!
//! The recordings are written by `cargo run -p tmux-cc -- record all`: one
//! chunk a line, `<` read from tmux and `>` written to it.

mod common;

use std::path::{Path, PathBuf};

use common::{Event, Player, unescape};
use zyt_tmux::{Client, Command, PaneId, WindowId};

/// One recorded session.
struct Recording {
    name: String,
    /// What tmux said, in the chunks the pty gave.
    said: Vec<Vec<u8>>,
    /// The commands written to tmux, in order.
    sent: Vec<Command>,
}

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn recordings() -> Vec<Recording> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(data_dir())
        .expect("tests/data")
        .map(|entry| entry.expect("an entry of tests/data").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no recordings in tests/data");
    files.iter().map(|path| read(path)).collect()
}

fn read(path: &Path) -> Recording {
    let text = std::fs::read(path).expect("a recording");
    let mut said = Vec::new();
    let mut sent = Vec::new();
    for line in text.split(|&byte| byte == b'\n') {
        match line {
            [b'<', b' ', chunk @ ..] => said.push(unescape(chunk)),
            [b'>', b' ', written @ ..] => sent.extend(commands(&unescape(written))),
            [b'<'] => said.push(Vec::new()),
            _ => {}
        }
    }
    let name = path
        .file_stem()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    Recording { name, said, sent }
}

/// The commands of a line written to tmux; every one is named by the replay.
fn commands(line: &[u8]) -> Vec<Command> {
    common::commands_of(line)
        .into_iter()
        .map(|(name, command)| {
            command.unwrap_or_else(|| panic!("a command the replay does not name: {name}"))
        })
        .collect()
}

/// A client with every command of the recording already sent.
fn client(recording: &Recording) -> Client {
    let mut client = Client::new(false);
    for &command in &recording.sent {
        common::send(&mut client, command);
    }
    assert_eq!(
        client.sent().len(),
        recording.sent.len(),
        "{}",
        recording.name
    );
    client
}

/// Replays what tmux said, cut into `chunks`.
fn replay(recording: &Recording, chunks: &[&[u8]]) -> (Vec<Event>, usize) {
    let mut player = Player::new(client(recording));
    for chunk in chunks {
        player.said(chunk);
    }
    let waiting = player.client.sent().len();
    (player.record.events, waiting)
}

/// Chunks of a length from 1 to 64, the same for a run as for the next.
fn random_chunks(stream: &[u8], seed: u64) -> Vec<&[u8]> {
    let mut state = seed;
    let mut chunks = Vec::new();
    let mut rest = stream;
    while !rest.is_empty() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let length = ((state >> 33) % 64 + 1) as usize;
        let (head, tail) = rest.split_at(length.min(rest.len()));
        chunks.push(head);
        rest = tail;
    }
    chunks
}

#[test]
fn every_recording_reads_without_an_error_and_every_reply_ends() {
    for recording in recordings() {
        let chunks: Vec<&[u8]> = recording.said.iter().map(Vec::as_slice).collect();
        let (events, waiting) = replay(&recording, &chunks);
        let name = &recording.name;
        assert_eq!(waiting, 0, "{name}: replies that never came");
        let errors: Vec<&Event> = events
            .iter()
            .filter(|event| matches!(event, Event::Error(_)))
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert_eq!(
            events.first(),
            Some(&Event::Entered),
            "{name}: the mode starts first"
        );
        let entered = events
            .iter()
            .filter(|event| **event == Event::Entered)
            .count();
        let left = events.iter().filter(|event| **event == Event::Left).count();
        assert_eq!((entered, left), (1, 1), "{name}: one mode, ended by detach");
        assert_eq!(
            events.get(1),
            Some(&Event::End(Command::Opening, false)),
            "{name}: the opening pair"
        );
        let failed: Vec<&Event> = events
            .iter()
            .filter(|event| matches!(event, Event::End(_, true)))
            .collect();
        if name.starts_with("failed-") {
            assert_eq!(
                failed,
                [&Event::End(Command::KillWindow(WindowId(0)), true)],
                "{name}"
            );
        } else {
            assert!(failed.is_empty(), "{name}: {failed:?}");
        }
    }
}

#[test]
fn every_recording_gives_the_same_events_however_it_is_cut() {
    for recording in recordings() {
        let stream = recording.said.concat();
        let name = &recording.name;
        let (whole, _) = replay(&recording, &[&stream]);
        let chunks: Vec<&[u8]> = recording.said.iter().map(Vec::as_slice).collect();
        assert_eq!(
            replay(&recording, &chunks).0,
            whole,
            "{name}: as the pty gave it"
        );
        let bytes: Vec<&[u8]> = stream.chunks(1).collect();
        assert_eq!(replay(&recording, &bytes).0, whole, "{name}: byte by byte");
        for seed in 1..=8 {
            let chunks = random_chunks(&stream, seed);
            assert_eq!(
                replay(&recording, &chunks).0,
                whole,
                "{name}: random chunks, seed {seed}"
            );
        }
    }
}

#[test]
fn a_nested_control_mode_is_seen_in_its_pane() {
    let nested: Vec<Recording> = recordings()
        .into_iter()
        .filter(|recording| recording.name.starts_with("nested-"))
        .collect();
    assert!(!nested.is_empty());
    for recording in nested {
        let chunks: Vec<&[u8]> = recording.said.iter().map(Vec::as_slice).collect();
        let (events, _) = replay(&recording, &chunks);
        let entered = events
            .iter()
            .position(|event| *event == Event::PaneEntered(PaneId(0)));
        let left = events
            .iter()
            .position(|event| *event == Event::PaneLeft(PaneId(0)));
        assert!(
            matches!((entered, left), (Some(entered), Some(left)) if entered < left),
            "{}: {entered:?} {left:?}",
            recording.name
        );
    }
}

#[test]
fn a_paused_pane_comes_as_extended_output_decoded() {
    for recording in recordings()
        .into_iter()
        .filter(|recording| recording.name.starts_with("pause-"))
    {
        let chunks: Vec<&[u8]> = recording.said.iter().map(Vec::as_slice).collect();
        let (events, _) = replay(&recording, &chunks);
        let aged = events
            .iter()
            .filter(|event| matches!(event, Event::Output(_, Some(_), _)))
            .count();
        assert!(aged > 0, "{}: no %extended-output", recording.name);
        assert!(
            events.iter().any(|event| matches!(
                event,
                Event::Output(_, Some(_), bytes) if bytes.windows(4).any(|four| four == b"0000")
            )),
            "{}",
            recording.name
        );
        assert!(
            events
                .iter()
                .any(|event| event == &Event::Notice("Paused(PaneId(0))".into()))
        );
    }
}
