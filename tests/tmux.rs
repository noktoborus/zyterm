//! The protocol of tmux control mode against a real tmux, every version on the
//! path, started through `zyt-pty` as a console starts a program, on a server
//! of its own: `tmux -L zyterm-test-<pid>-… -f /dev/null -CC`.
//!
//! The mode of the program itself — a pane shown, input, the size, the end —
//! is tested against the same tmuxes by the tests of `src/session/`, which an
//! integration test cannot reach. Here is what tmux does with the protocol
//! `zyt-tmux` speaks.

#![cfg(unix)]

use std::time::{Duration, Instant};

use zyt_pty::{PtyConfig, PtySession};
use zyt_tmux::{Client, Command, Notice, PaneId, Part, Piece, Reply, Sink, Splitter};

/// How long a case waits for what tmux is to say.
const PATIENCE: Duration = Duration::from_secs(10);

/// What came back, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Output(PaneId, Vec<u8>),
    Line(Command, String),
    End(Command, bool),
    Notice(String),
}

#[derive(Default)]
struct Record {
    events: Vec<Event>,
}

impl Sink for Record {
    fn output(&mut self, pane: PaneId, _age: Option<u32>, bytes: &[u8]) {
        self.events.push(Event::Output(pane, bytes.to_vec()));
    }

    fn reply_line(&mut self, command: Command, line: &[u8]) -> Reply {
        self.events.push(Event::Line(
            command,
            String::from_utf8_lossy(line).into_owned(),
        ));
        Reply::Go
    }

    fn reply_end(&mut self, command: Command, failed: bool) {
        self.events.push(Event::End(command, failed));
    }

    fn notice(&mut self, notice: Notice<'_>) {
        self.events.push(Event::Notice(format!("{notice:?}")));
    }
}

/// A tmux client in control mode and what it said.
struct Tmux {
    binary: &'static str,
    socket: String,
    pty: PtySession,
    splitter: Splitter,
    client: Client,
    record: Record,
    spare: Vec<u8>,
    /// The server goes with this client; a case that attaches again keeps it.
    kills_server: bool,
}

impl Tmux {
    /// `tmux -CC` with `args` after it, on the server named by `tag`.
    fn start(binary: &'static str, tag: &str, args: &[&str]) -> Self {
        let socket = format!("zyterm-test-{}-{tag}", std::process::id());
        let mut all = vec!["-L", socket.as_str(), "-f", "/dev/null", "-CC"];
        all.extend_from_slice(args);
        let config = PtyConfig {
            program: Some(binary.to_string()),
            args: all.iter().map(|arg| arg.to_string()).collect(),
            columns: 80,
            rows: 24,
            cell: (8, 16),
            environment: vec![
                ("TMUX".into(), String::new()),
                ("ENV".into(), String::new()),
            ],
        };
        let pty = PtySession::spawn(&config, None).expect("tmux starts");
        let mut tmux = Self {
            binary,
            socket,
            pty,
            splitter: Splitter::new(),
            client: Client::new(false),
            record: Record::default(),
            spare: Vec::new(),
            kills_server: true,
        };
        assert!(
            tmux.until(|events| events.contains(&Event::End(Command::Opening, false))),
            "{binary}: the mode opens"
        );
        tmux
    }

    /// Reads what tmux said so far.
    fn take(&mut self) {
        self.pty.read_into(&mut self.spare);
        let mut chunk = std::mem::take(&mut self.spare);
        let client = &mut self.client;
        let record = &mut self.record;
        self.splitter.split(&mut chunk, |piece| {
            if let Piece::Control(bytes) = piece {
                let _ = client.feed(bytes, record);
            }
        });
        self.spare = chunk;
    }

    /// Reads until `done` holds of what came, or the patience is over.
    fn until(&mut self, mut done: impl FnMut(&[Event]) -> bool) -> bool {
        let end = Instant::now() + PATIENCE;
        loop {
            self.take();
            if done(&self.record.events) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Sends commands as one line and waits for the last of them to end.
    fn ask(&mut self, commands: &[Command]) -> usize {
        let from = self.record.events.len();
        let mut out = Vec::new();
        self.client.send_line(commands, &mut out);
        self.pty.write(&out);
        let last = *commands.last().expect("a command");
        let binary = self.binary;
        assert!(
            self.until(|events| events[from..]
                .iter()
                .any(|event| matches!(event, Event::End(command, _) if *command == last))),
            "{binary}: {last:?} ends"
        );
        from
    }

    /// Writes a raw command line, for the commands `Command` does not name.
    fn raw(&mut self, line: &str) {
        self.pty.write(format!("{line}\n").as_bytes());
    }

    /// Types into a pane and waits for the `send-keys` to end.
    fn type_into(&mut self, pane: PaneId, bytes: &[u8]) {
        let from = self.record.events.len();
        let mut out = Vec::new();
        self.client.send_keys(pane, bytes, &mut out);
        let runs = self.client.sent().count();
        self.pty.write(&out);
        let binary = self.binary;
        assert!(
            self.until(|events| events[from..]
                .iter()
                .filter(|event| matches!(event, Event::End(Command::SendKeys(_), false)))
                .count()
                >= runs),
            "{binary}: the keys are taken"
        );
    }

    /// Everything a pane said since `from`, joined.
    fn output_of(&self, pane: PaneId, from: usize) -> Vec<u8> {
        self.record.events[from..]
            .iter()
            .filter_map(|event| match event {
                Event::Output(of, bytes) if *of == pane => Some(bytes.as_slice()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .concat()
    }

    /// The lines of the replies to `command` since `from`.
    fn lines_of(&self, command: Command, from: usize) -> Vec<String> {
        self.record.events[from..]
            .iter()
            .filter_map(|event| match event {
                Event::Line(of, line) if *of == command => Some(line.clone()),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        if !self.kills_server {
            return;
        }
        let _ = std::process::Command::new(self.binary)
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

/// The versions of tmux on the path: the container builds five; one not found
/// is left out.
fn installed() -> Vec<&'static str> {
    ["tmux", "tmux-3.0a", "tmux-3.2a", "tmux-3.4", "tmux-3.8"]
        .into_iter()
        .filter(|binary| {
            std::process::Command::new(binary)
                .arg("-V")
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// `echo` typed through `send-keys` comes back in `%output` of the pane.
#[test]
fn what_is_typed_comes_back_as_output() {
    for binary in installed() {
        let mut tmux = Tmux::start(
            binary,
            &format!("{binary}-echo"),
            &["new", "-s", "test", "sh"],
        );
        let from = tmux.record.events.len();
        tmux.type_into(PaneId(0), b"echo zyt-$((6*7))\r");
        assert!(
            tmux.until(|events| events[from..].iter().any(|event| matches!(
                event,
                Event::Output(PaneId(0), bytes) if contains(bytes, b"zyt-42")
            ))),
            "{binary}: the answer of the shell came back: {:?}",
            String::from_utf8_lossy(&tmux.output_of(PaneId(0), from))
        );
    }
}

/// A mouse report, `ESC CR`, `CSI 13;2u` by `-H`, and text with UTF-8, `'`,
/// `\`, `#{`, `;`, `~` and `$` by `-l`, reach a program in the pane byte for
/// byte; text that begins with `-` too, which tmux would read as a flag
/// without the `--` before it.
#[test]
fn input_reaches_the_program_byte_for_byte() {
    let mut payload = "-x it's #{x} ; ~ $HOME \\ жук ".as_bytes().to_vec();
    payload.extend_from_slice(b"\x1b[<0;10;5M\x1b\r\x1b[13;2u\x7f");
    let expected: Vec<String> = payload.iter().map(|byte| format!("{byte:02x}")).collect();
    let program = format!(
        "stty raw -echo; head -c {} | od -An -tx1 -v; sleep 10",
        payload.len()
    );
    for binary in installed() {
        let mut tmux = Tmux::start(
            binary,
            &format!("{binary}-bytes"),
            &["new", "-s", "test", &program],
        );
        std::thread::sleep(Duration::from_millis(300));
        let from = tmux.record.events.len();
        tmux.type_into(PaneId(0), &payload);
        let joined = expected.join(" ");
        let found = tmux.until(|events| {
            let text: String = events[from..]
                .iter()
                .filter_map(|event| match event {
                    Event::Output(PaneId(0), bytes) => {
                        Some(String::from_utf8_lossy(bytes).into_owned())
                    }
                    _ => None,
                })
                .collect();
            text.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains(&joined)
        });
        assert!(
            found,
            "{binary}: od read {:?}",
            String::from_utf8_lossy(&tmux.output_of(PaneId(0), from))
        );
    }
}

/// The output of a pane in a window not shown comes; that of a pane of
/// another session does not.
#[test]
fn hidden_windows_speak_and_other_sessions_do_not() {
    for binary in installed() {
        let mut tmux = Tmux::start(
            binary,
            &format!("{binary}-hidden"),
            &["new", "-s", "test", "sh"],
        );
        let from = tmux.record.events.len();
        tmux.raw("new-window -d 'echo zyt-hidden; sleep 10'");
        assert!(
            tmux.until(|events| events[from..].iter().any(|event| matches!(
                event,
                Event::Output(pane, bytes) if *pane != PaneId(0) && contains(bytes, b"zyt-hidden")
            ))),
            "{binary}: the hidden window's output came"
        );

        let from = tmux.record.events.len();
        tmux.raw("new-session -d -s other 'while :; do echo zyt-other; sleep 0.05; done'");
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(800) {
            tmux.take();
            std::thread::sleep(Duration::from_millis(20));
        }
        let other = tmux.record.events[from..]
            .iter()
            .any(|event| matches!(event, Event::Output(_, bytes) if contains(bytes, b"zyt-other")));
        assert!(
            !other,
            "{binary}: another session's pane said nothing to this client"
        );
    }
}

/// Output running while a snapshot is taken: what the snapshot holds and what
/// comes after it join with no line twice and none missing.
#[test]
fn a_snapshot_and_the_output_after_it_join() {
    let program = "sleep 0.3; i=0; while [ $i -lt 400000 ]; do i=$((i+1)); echo $i; done; sleep 10";
    for binary in installed() {
        let mut tmux = Tmux::start(
            binary,
            &format!("{binary}-edge"),
            &["new", "-s", "test", program],
        );
        assert!(
            tmux.until(|events| events
                .iter()
                .any(|event| matches!(event, Event::Output(..)))),
            "{binary}: the flood began"
        );
        let display = Command::Display(PaneId(0));
        let capture = Command::Capture {
            pane: PaneId(0),
            part: Part::Visible,
        };
        let from = tmux.ask(&[display, capture]);
        let ended = tmux
            .record
            .events
            .iter()
            .rposition(|event| *event == Event::End(capture, false))
            .expect("the snapshot ended");
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(300) {
            tmux.take();
            std::thread::sleep(Duration::from_millis(10));
        }
        let modes = tmux.lines_of(display, from);
        let modes = zyt_tmux::Modes::parse(modes[0].as_bytes()).expect("the modes");
        let screen = tmux.lines_of(capture, from);
        let (x, y) = modes.cursor;
        let mut text = String::new();
        for line in &screen[..y as usize] {
            text.push_str(line);
            text.push('\n');
        }
        let current: String = screen[y as usize].chars().take(x as usize).collect();
        text.push_str(&current);
        let after = tmux.output_of(PaneId(0), ended + 1);
        text.push_str(&String::from_utf8_lossy(&after).replace('\r', ""));

        let lines: Vec<&str> = text.split('\n').collect();
        let whole = &lines[..lines.len() - 1];
        let numbers: Vec<u64> = whole
            .iter()
            .filter_map(|line| line.trim().parse().ok())
            .collect();
        assert!(
            numbers.len() > 30,
            "{binary}: enough lines to judge: {}",
            numbers.len()
        );
        for pair in numbers.windows(2) {
            assert_eq!(
                pair[1],
                pair[0] + 1,
                "{binary}: a line twice or missing at the edge of the snapshot"
            );
        }
    }
}

/// Detached, the session stands; attached again, the snapshot holds what the
/// pane held.
#[test]
fn a_session_stands_through_detach_and_attach() {
    for binary in installed() {
        let tag = format!("{binary}-attach");
        let mut tmux = Tmux::start(binary, &tag, &["new", "-s", "test", "sh"]);
        let from = tmux.record.events.len();
        tmux.type_into(PaneId(0), b"echo zyt-$((40+2))-kept\r");
        assert!(tmux.until(|events| {
            events[from..].iter().any(|event| {
                matches!(
                    event,
                    Event::Output(PaneId(0), bytes) if contains(bytes, b"zyt-42-kept")
                )
            })
        }));
        tmux.ask(&[Command::Detach]);
        assert!(
            tmux.until(|events| events
                .iter()
                .any(|event| event.clone() == Event::Notice("Exit(None)".into()))),
            "{binary}: %exit"
        );
        let socket = tmux.socket.clone();
        tmux.kills_server = false;
        drop(tmux);

        let mut again = Tmux::start(binary, &tag, &["attach", "-t", "test"]);
        assert_eq!(again.socket, socket);
        let capture = Command::Capture {
            pane: PaneId(0),
            part: Part::Visible,
        };
        let from = again.ask(&[capture]);
        let screen = again.lines_of(capture, from).join("\n");
        assert!(screen.contains("zyt-42-kept"), "{binary}: {screen}");
    }
}
