//! Every recorded session of a real tmux, replayed through the splitter and
//! the client, whole and in chunks of every kind.
//!
//! The recordings are written by `cargo run -p tmux-cc -- record all`: one
//! chunk a line, `<` read from tmux and `>` written to it.

use std::path::{Path, PathBuf};

use zyt_tmux::{
    Client, Command, Notice, PaneId, Part, Piece, Reply, SessionId, Sink, Splitter, WindowId,
};

/// One recorded session.
struct Recording {
    name: String,
    /// What tmux said, in the chunks the pty gave.
    said: Vec<Vec<u8>>,
    /// The commands written to tmux, in order.
    sent: Vec<Command>,
}

/// What a replay gives, the pieces a chunk boundary cut joined again.
#[derive(Debug, PartialEq, Eq)]
enum Event {
    Plain(Vec<u8>),
    Entered,
    Left,
    Output(PaneId, Option<u32>, Vec<u8>),
    Line(Command, Vec<u8>),
    End(Command, bool),
    Notice(String),
    /// A control mode started inside a pane: the nested case.
    PaneEntered(PaneId),
    PaneLeft(PaneId),
    Error(String),
}

#[derive(Default)]
struct Record {
    events: Vec<Event>,
    panes: Vec<(PaneId, Splitter)>,
}

impl Record {
    fn push(&mut self, event: Event) {
        match (self.events.last_mut(), event) {
            (Some(Event::Plain(last)), Event::Plain(more)) => last.extend(more),
            (_, event) => self.events.push(event),
        }
    }
}

impl Sink for Record {
    fn output(&mut self, pane: PaneId, age: Option<u32>, bytes: &[u8]) {
        self.events.push(Event::Output(pane, age, bytes.to_vec()));
        let at = match self.panes.iter().position(|(known, _)| *known == pane) {
            Some(at) => at,
            None => {
                self.panes.push((pane, Splitter::new()));
                self.panes.len() - 1
            }
        };
        let mut pieces = Vec::new();
        self.panes[at]
            .1
            .split(&mut bytes.to_vec(), |piece| match piece {
                Piece::Entered => pieces.push(Event::PaneEntered(pane)),
                Piece::Left => pieces.push(Event::PaneLeft(pane)),
                Piece::Plain(_) | Piece::Control(_) => {}
            });
        self.events.extend(pieces);
    }

    fn reply_line(&mut self, command: Command, line: &[u8]) -> Reply {
        self.events.push(Event::Line(command, line.to_vec()));
        Reply::Go
    }

    fn reply_end(&mut self, command: Command, failed: bool) {
        self.events.push(Event::End(command, failed));
    }

    fn notice(&mut self, notice: Notice<'_>) {
        self.events.push(Event::Notice(format!("{notice:?}")));
    }
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

/// The bytes of a chunk as `tmux-cc` writes them: `\r`, `\n`, `\t`, `\e`,
/// `\\`, `\xHH`, and every other byte as it is.
fn unescape(text: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at] != b'\\' {
            bytes.push(text[at]);
            at += 1;
            continue;
        }
        let (byte, length) = match text.get(at + 1) {
            Some(b'r') => (b'\r', 2),
            Some(b'n') => (b'\n', 2),
            Some(b't') => (b'\t', 2),
            Some(b'e') => (0x1b, 2),
            Some(b'\\') => (b'\\', 2),
            Some(b'x') => {
                let hex = std::str::from_utf8(&text[at + 2..at + 4]).expect("hex digits");
                (u8::from_str_radix(hex, 16).expect("a byte in hex"), 4)
            }
            other => panic!("an escape of a recording: {other:?}"),
        };
        bytes.push(byte);
        at += length;
    }
    bytes
}

/// The commands of a line written to tmux, each named by its first word: the
/// matching goes by order alone, and the name says what a reply answers.
fn commands(line: &[u8]) -> Vec<Command> {
    let line = String::from_utf8_lossy(line);
    line.trim_end()
        .split(" ; ")
        .map(
            |command| match command.split(' ').next().unwrap_or_default() {
                "capture-pane" => Command::Capture {
                    pane: PaneId(0),
                    part: Part::Visible,
                },
                "display" => Command::Display(PaneId(0)),
                "detach" => Command::Detach,
                "kill-window" => Command::KillWindow(WindowId(0)),
                "list-panes" => Command::ListPanes,
                "list-sessions" => Command::ListSessions,
                "list-windows" => Command::ListWindows,
                "new-session" | "new-window" => Command::NewWindow(SessionId(0)),
                "refresh-client" => Command::ClientSize {
                    columns: 0,
                    rows: 0,
                },
                "rename-window" => Command::RenameWindow(WindowId(0)),
                "select-window" => Command::SelectWindow(WindowId(0)),
                "send-keys" => Command::SendKeys(PaneId(0)),
                "switch-client" => Command::SwitchClient(SessionId(0)),
                other => panic!("a command the replay does not name: {other}"),
            },
        )
        .collect()
}

/// A client with every command of the recording already sent.
fn client(recording: &Recording) -> Client {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    for &command in &recording.sent {
        match command {
            Command::SendKeys(pane) => client.send_keys(pane, b"x", &mut out),
            Command::RenameWindow(window) => client.rename_window(window, "x", &mut out),
            command => client.send(command, &mut out),
        }
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
    let mut splitter = Splitter::new();
    let mut client = client(recording);
    let mut record = Record::default();
    let mut control = Vec::new();
    for chunk in chunks {
        let mut pieces: Vec<Event> = Vec::new();
        splitter.split(&mut chunk.to_vec(), |piece| match piece {
            Piece::Plain(bytes) => pieces.push(Event::Plain(bytes.to_vec())),
            Piece::Entered => pieces.push(Event::Entered),
            Piece::Control(bytes) => pieces.push(Event::Line(Command::Opening, bytes.to_vec())),
            Piece::Left => pieces.push(Event::Left),
        });
        for piece in pieces {
            match piece {
                Event::Line(Command::Opening, bytes) => control.push(bytes),
                event => {
                    feed(&mut client, &mut control, &mut record);
                    record.push(event);
                }
            }
        }
        feed(&mut client, &mut control, &mut record);
    }
    (record.events, client.sent().len())
}

fn feed(client: &mut Client, control: &mut Vec<Vec<u8>>, record: &mut Record) {
    for mut bytes in control.drain(..) {
        if let Err(error) = client.feed(&mut bytes, record) {
            record.events.push(Event::Error(format!("{error:?}")));
        }
    }
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
