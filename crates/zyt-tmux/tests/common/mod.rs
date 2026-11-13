//! What the tests that play a session share: the reader of its lines, the
//! commands named in them, and the player that feeds what tmux said through
//! the splitter and the client.

#![allow(dead_code, reason = "each test of the directory takes what it needs")]

use zyt_tmux::{
    Client, Command, Notice, PaneId, Part, Piece, Reply, SessionId, Sink, Splitter, WindowId,
};

/// What a play gives, the pieces a chunk boundary cut joined again.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
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

/// The sink of the client: every event, in order.
#[derive(Default)]
pub struct Record {
    pub events: Vec<Event>,
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

/// The bytes of a chunk as the recordings and the journals of zyterm write
/// them: `\r`, `\n`, `\t`, `\e`, `\\`, `\xHH`, and every other byte as it is.
pub fn unescape(text: &[u8]) -> Vec<u8> {
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

/// The command a line written to tmux begins with, by its first word: the
/// matching of replies goes by order alone, and the name says what a reply
/// answers. Nothing for a name this does not know.
pub fn command_named(name: &str) -> Option<Command> {
    Some(match name {
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
        _ => return None,
    })
}

/// The commands of one line written to tmux, split at ` ; `, with the name of
/// each.
pub fn commands_of(line: &[u8]) -> Vec<(String, Option<Command>)> {
    String::from_utf8_lossy(line)
        .trim_end()
        .split(" ; ")
        .map(|command| {
            let name = command.split(' ').next().unwrap_or_default().to_string();
            let named = command_named(&name);
            (name, named)
        })
        .collect()
}

/// Queues `command` in `client` the way the window sends it.
pub fn send(client: &mut Client, command: Command) {
    let mut out = Vec::new();
    match command {
        Command::SendKeys(pane) => client.send_keys(pane, b"x", &mut out),
        Command::RenameWindow(window) => client.rename_window(window, "x", &mut out),
        command => client.send(command, &mut out),
    }
}

/// Feeds what tmux said through a splitter and a client, as the window does.
pub struct Player {
    pub splitter: Splitter,
    pub client: Client,
    pub record: Record,
    control: Vec<Vec<u8>>,
}

impl Player {
    pub fn new(client: Client) -> Self {
        Self {
            splitter: Splitter::new(),
            client,
            record: Record::default(),
            control: Vec::new(),
        }
    }

    /// One chunk that tmux said.
    pub fn said(&mut self, chunk: &[u8]) {
        let mut pieces: Vec<Event> = Vec::new();
        self.splitter
            .split(&mut chunk.to_vec(), |piece| match piece {
                Piece::Plain(bytes) => pieces.push(Event::Plain(bytes.to_vec())),
                Piece::Entered => pieces.push(Event::Entered),
                Piece::Control(bytes) => pieces.push(Event::Line(Command::Opening, bytes.to_vec())),
                Piece::Left => pieces.push(Event::Left),
            });
        for piece in pieces {
            match piece {
                Event::Line(Command::Opening, bytes) => self.control.push(bytes),
                event => {
                    self.feed();
                    self.record.push(event);
                }
            }
        }
        self.feed();
    }

    fn feed(&mut self) {
        for mut bytes in self.control.drain(..) {
            if let Err(error) = self.client.feed(&mut bytes, &mut self.record) {
                self.record.events.push(Event::Error(format!("{error:?}")));
            }
        }
    }
}
