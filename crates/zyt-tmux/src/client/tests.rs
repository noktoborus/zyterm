use super::*;
use crate::command::{Flow, Part};
use crate::id::SessionId;

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Output(PaneId, Option<u32>, Vec<u8>),
    Line(Command, Vec<u8>),
    End(Command, bool),
    Notice(String),
}

/// What comes back, written down; a reply of a command in `bounds` is broken
/// past that many lines.
#[derive(Default)]
struct Record {
    events: Vec<Event>,
    bounds: Vec<(Command, usize)>,
    lines: usize,
}

impl Sink for Record {
    fn output(&mut self, pane: PaneId, age: Option<u32>, bytes: &[u8]) {
        self.events.push(Event::Output(pane, age, bytes.to_vec()));
    }

    fn reply_line(&mut self, command: Command, line: &[u8]) -> Reply {
        self.events.push(Event::Line(command, line.to_vec()));
        self.lines += 1;
        let bound = self.bounds.iter().find(|(bounded, _)| *bounded == command);
        match bound {
            Some(&(_, bound)) if self.lines > bound => Reply::Broken,
            _ => Reply::Go,
        }
    }

    fn reply_end(&mut self, command: Command, failed: bool) {
        self.lines = 0;
        self.events.push(Event::End(command, failed));
    }

    fn notice(&mut self, notice: Notice<'_>) {
        self.events.push(Event::Notice(format!("{notice:?}")));
    }
}

/// Feeds the stream whole, and cut at every offset, and byte by byte; every
/// way gives the same events and the same first error.
fn feed(
    client: impl Fn() -> Client,
    stream: &[u8],
    record: impl Fn() -> Record,
) -> (Vec<Event>, Result<()>) {
    let whole = feed_chunks(client(), &[stream], record());
    for at in 0..=stream.len() {
        let (head, tail) = stream.split_at(at);
        let cut = feed_chunks(client(), &[head, tail], record());
        assert_eq!(cut.0, whole.0, "cut at {at}");
        assert_eq!(cut.1.is_err(), whole.1.is_err(), "cut at {at}");
    }
    let bytes: Vec<&[u8]> = stream.chunks(1).collect();
    let single = feed_chunks(client(), &bytes, record());
    assert_eq!(single.0, whole.0, "byte by byte");
    whole
}

fn feed_chunks(
    mut client: Client,
    chunks: &[&[u8]],
    mut record: Record,
) -> (Vec<Event>, Result<()>) {
    let mut first = Ok(());
    for chunk in chunks {
        let outcome = client.feed(&mut chunk.to_vec(), &mut record);
        if first.is_ok() {
            first = outcome;
        }
    }
    (record.events, first)
}

/// A client that sent `commands`, in order.
fn sent(lossy: bool, commands: &[Command]) -> Client {
    let mut client = Client::new(lossy);
    let mut out = Vec::new();
    for &command in commands {
        match command {
            Command::SendKeys(pane) => client.send_keys(pane, b"ls", &mut out),
            command => client.send(command, &mut out),
        }
    }
    client
}

fn line(command: Command, text: &str) -> Event {
    Event::Line(command, text.as_bytes().to_vec())
}

const CAPTURE: Command = Command::Capture {
    pane: PaneId(3),
    part: Part::Visible,
};

#[test]
fn the_opening_pair_answers_nothing_sent() {
    let stream = b"%begin 1 260 0\r\n%end 1 260 0\r\n%window-add @0\r\n";
    let (events, outcome) = feed(|| sent(false, &[Command::Version]), stream, Record::default);
    assert!(outcome.is_ok());
    assert_eq!(
        events,
        [
            Event::End(Command::Opening, false),
            Event::Notice("WindowAdded(WindowId(0))".into())
        ]
    );
}

#[test]
fn a_start_that_failed_says_why() {
    let stream = b"%begin 1 260 0\r\nno sessions\r\n%error 1 260 0\r\n%exit\r\n";
    let (events, _) = feed(|| Client::new(false), stream, Record::default);
    assert_eq!(
        events,
        [
            line(Command::Opening, "no sessions"),
            Event::End(Command::Opening, true),
            Event::Notice("Exit(None)".into())
        ]
    );
}

#[test]
fn replies_are_matched_in_the_order_sent() {
    let commands = [
        Command::ListSessions,
        Command::SendKeys(PaneId(0)),
        Command::KillWindow(WindowId(9)),
        Command::Version,
    ];
    let stream = b"%begin 1 266 1\r\n$0 zyt\r\n$1 a b\r\n%end 1 266 1\r\n\
        %begin 1 267 1\r\n%end 1 267 1\r\n%output %0 l\r\n\
        %begin 1 270 1\r\ncan't find window: @9\r\n%error 1 270 1\r\n\
        %begin 1 272 1\r\n3.3a\r\n%end 1 272 1\r\n";
    let (events, outcome) = feed(|| sent(false, &commands), stream, Record::default);
    assert!(outcome.is_ok());
    assert_eq!(
        events,
        [
            line(Command::ListSessions, "$0 zyt"),
            line(Command::ListSessions, "$1 a b"),
            Event::End(Command::ListSessions, false),
            Event::End(Command::SendKeys(PaneId(0)), false),
            Event::Output(PaneId(0), None, b"l".to_vec()),
            line(Command::KillWindow(WindowId(9)), "can't find window: @9"),
            Event::End(Command::KillWindow(WindowId(9)), true),
            line(Command::Version, "3.3a"),
            Event::End(Command::Version, false),
        ]
    );
}

#[test]
fn lines_of_a_reply_that_start_with_percent_stay_lines() {
    let stream =
        b"%begin 1 5 1\r\n%output %3 x\r\n%end of a word\r\n%begin 9 9 1\r\n%end 1 5 1\r\n";
    let (events, outcome) = feed(|| sent(false, &[CAPTURE]), stream, Record::default);
    assert!(outcome.is_ok());
    assert_eq!(
        events,
        [
            line(CAPTURE, "%output %3 x"),
            line(CAPTURE, "%end of a word"),
            line(CAPTURE, "%begin 9 9 1"),
            Event::End(CAPTURE, false),
        ]
    );
}

#[test]
fn a_reply_with_nothing_sent_is_an_error_and_thrown_away() {
    let stream = b"%begin 1 5 1\r\nwhat\r\n%end 1 5 1\r\n%sessions-changed\r\n";
    let (events, outcome) = feed(|| Client::new(false), stream, Record::default);
    assert_eq!(outcome, Err(Error::UnexpectedReply));
    assert_eq!(events, [Event::Notice("SessionsChanged".into())]);
}

#[test]
fn output_is_decoded_in_place() {
    let stream = b"%output %0 ls\\015\\012\r\n%extended-output %1 15 : \\033[m\\134\r\n";
    let (events, _) = feed(|| Client::new(false), stream, Record::default);
    assert_eq!(
        events,
        [
            Event::Output(PaneId(0), None, b"ls\r\n".to_vec()),
            Event::Output(PaneId(1), Some(15), b"\x1b[m\\".to_vec()),
        ]
    );
}

#[test]
fn a_bad_line_is_skipped_and_the_next_read() {
    let stream = b"rubbish\r\n%output x\r\n%window-add @1\r\n";
    let (events, outcome) = feed(|| Client::new(false), stream, Record::default);
    assert_eq!(outcome, Err(Error::MalformedLine));
    assert_eq!(events, [Event::Notice("WindowAdded(WindowId(1))".into())]);
}

#[test]
fn lossy_a_notification_closes_a_reply_whose_end_was_lost() {
    let stream = b"%begin 1 5 1\r\nline\r\n%en\xff 1 5 1\r\n%output %3 x\r\n\
        %begin 1 6 1\r\n3.3a\r\n%end 1 6 1\r\n";
    let (events, _) = feed(
        || sent(true, &[CAPTURE, Command::Version]),
        stream,
        Record::default,
    );
    assert_eq!(
        events,
        [
            line(CAPTURE, "line"),
            line(CAPTURE, "%en\u{FFFD} 1 5 1"),
            Event::End(CAPTURE, true),
            Event::Output(PaneId(3), None, b"x".to_vec()),
            line(Command::Version, "3.3a"),
            Event::End(Command::Version, false),
        ]
        .map(|event| match event {
            Event::Line(command, text) if text.starts_with(b"%en") => {
                Event::Line(command, b"%en\xff 1 5 1".to_vec())
            }
            event => event,
        })
    );
}

#[test]
fn lossy_a_begin_closes_the_reply_before_it() {
    let stream = b"%begin 1 5 1\r\nline\r\n%begin 1 6 1\r\n3.3a\r\n%end 1 6 1\r\n";
    let (events, outcome) = feed(
        || sent(true, &[CAPTURE, Command::Version]),
        stream,
        Record::default,
    );
    assert!(outcome.is_ok());
    assert_eq!(
        events,
        [
            line(CAPTURE, "line"),
            Event::End(CAPTURE, true),
            line(Command::Version, "3.3a"),
            Event::End(Command::Version, false),
        ]
    );
}

#[test]
fn a_reply_longer_than_it_can_be_is_closed_as_broken() {
    let stream = b"%begin 1 5 1\r\na\r\nb\r\nc\r\nd\r\n%begin 1 6 1\r\n3.3a\r\n%end 1 6 1\r\n";
    let record = || Record {
        bounds: vec![(CAPTURE, 2)],
        ..Record::default()
    };
    for lossy in [true, false] {
        let (events, _) = feed(|| sent(lossy, &[CAPTURE, Command::Version]), stream, record);
        let mut expected = vec![
            line(CAPTURE, "a"),
            line(CAPTURE, "b"),
            line(CAPTURE, "c"),
            Event::End(CAPTURE, true),
        ];
        if lossy {
            expected.extend([
                line(Command::Version, "3.3a"),
                Event::End(Command::Version, false),
            ]);
        }
        assert_eq!(events, expected, "lossy {lossy}");
    }
}

#[test]
fn not_lossy_a_broken_reply_waits_for_its_end() {
    let stream =
        b"%begin 1 5 1\r\na\r\nb\r\nc\r\n%end 1 5 1\r\n%begin 1 6 1\r\n3.3a\r\n%end 1 6 1\r\n";
    let record = || Record {
        bounds: vec![(CAPTURE, 1)],
        ..Record::default()
    };
    let (events, outcome) = feed(|| sent(false, &[CAPTURE, Command::Version]), stream, record);
    assert!(outcome.is_ok());
    assert_eq!(
        events,
        [
            line(CAPTURE, "a"),
            line(CAPTURE, "b"),
            Event::End(CAPTURE, true),
            line(Command::Version, "3.3a"),
            Event::End(Command::Version, false),
        ]
    );
}

#[test]
fn lossy_an_end_whose_begin_was_lost_takes_its_command() {
    let stream = b"%beg\xffn 1 5 1\r\n%end 1 5 1\r\n%begin 1 6 1\r\n3.3a\r\n%end 1 6 1\r\n";
    let (events, _) = feed(
        || sent(true, &[CAPTURE, Command::Version]),
        stream,
        Record::default,
    );
    assert_eq!(
        events,
        [
            Event::Notice("Unknown([37, 98, 101, 103, 255, 110])".into()),
            Event::End(CAPTURE, true),
            line(Command::Version, "3.3a"),
            Event::End(Command::Version, false),
        ]
    );
}

#[test]
fn a_line_longer_than_the_buffer_is_dropped() {
    let stream = b"%output %0 0123456789abcdef\r\n%window-add @1\r\n%output %0 short\r\n";
    let client = || {
        let mut client = Client::new(false);
        client.fit(24);
        client
    };
    let (events, outcome) =
        feed_chunks(client(), &[&stream[..20], &stream[20..]], Record::default());
    assert_eq!(outcome, Err(Error::MalformedLine));
    assert_eq!(
        events,
        [
            Event::Notice("WindowAdded(WindowId(1))".into()),
            Event::Output(PaneId(0), None, b"short".to_vec()),
        ]
    );
    let (whole, _) = feed_chunks(client(), &[stream], Record::default());
    assert_eq!(whole.len(), 3, "a line whole in one chunk is no line held");
}

#[test]
fn forget_drops_what_was_sent_and_the_line_held() {
    let mut client = sent(true, &[CAPTURE, Command::Detach]);
    let mut record = Record::default();
    client
        .feed(&mut b"%begin 1 5 1\r\nhalf a li".to_vec(), &mut record)
        .unwrap();
    client.forget();
    assert_eq!(client.sent().len(), 0);
    client
        .feed(&mut b"%window-add @1\r\n".to_vec(), &mut record)
        .unwrap();
    assert_eq!(
        record.events.last(),
        Some(&Event::Notice("WindowAdded(WindowId(1))".into()))
    );
}

#[test]
fn commands_are_written_and_queued() {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    let commands = [
        Command::Version,
        Command::ClientSize {
            columns: 120,
            rows: 40,
        },
        Command::PauseAfter(Some(5)),
        Command::PauseAfter(None),
        Command::PaneFlow {
            pane: PaneId(1),
            flow: Flow::Continue,
        },
        Command::SwitchClient(SessionId(1)),
        Command::SelectWindow(WindowId(4)),
        Command::NewWindow(SessionId(1)),
        Command::KillWindow(WindowId(4)),
        Command::KillPane(PaneId(7)),
        Command::Group(SessionId(1)),
        Command::DestroyUnattached,
        Command::Zoom(PaneId(7)),
        Command::Unzoom(WindowId(4)),
        Command::ZoomOver(PaneId(7)),
        Command::Split {
            pane: PaneId(7),
            side_by_side: true,
        },
        Command::ResizePane {
            pane: PaneId(7),
            side_by_side: false,
            cells: 10,
        },
        Command::Capture {
            pane: PaneId(7),
            part: Part::Saved,
        },
        Command::Capture {
            pane: PaneId(7),
            part: Part::Range { depth: 30 },
        },
        Command::Capture {
            pane: PaneId(7),
            part: Part::History { depth: 30 },
        },
        Command::Detach,
        Command::WindowInfo,
        Command::Opening,
        Command::SendKeys(PaneId(1)),
    ];
    for command in commands {
        client.send(command, &mut out);
    }
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "display -p '#{version}'\n\
         refresh-client -C 120x40\n\
         refresh-client -f pause-after=5\n\
         refresh-client -f '!pause-after'\n\
         refresh-client -A '%1:continue'\n\
         switch-client -t $1\n\
         select-window -t @4\n\
         new-window -t $1\n\
         kill-window -t @4\n\
         kill-pane -t %7\n\
         new-session -t '$1'\n\
         set destroy-unattached on\n\
         resize-pane -Z -t %7\n\
         resize-pane -Z -t @4\n\
         select-pane -Z -t %7\n\
         split-window -h -t %7\n\
         resize-pane -t %7 -y 10\n\
         capture-pane -p -e -J -a -q -t %7\n\
         capture-pane -p -e -J -S -30 -t %7\n\
         capture-pane -p -e -J -S -30 -E -1 -t %7\n\
         detach\n\
         display -p '#{version} #{window_width}x#{window_height} #{session_attached}'\n"
    );
    assert_eq!(client.sent().len(), commands.len() - 2);
}

#[test]
fn keys_go_as_runs_of_text_and_hex() {
    let cases: &[(&[u8], &str, usize)] = &[
        (
            b"ls -la\r",
            "send-keys -t %3 -l -- 'ls -la' ; send-keys -t %3 -H 0d",
            2,
        ),
        (b"\x1b[A", "send-keys -t %3 -H 1b 5b 41", 1),
        (b"l", "send-keys -t %3 -H 6c", 1),
        (
            b"\x1b[<0;10;5Mab",
            "send-keys -t %3 -H 1b 5b 3c 30 3b 31 30 3b 35 4d ; send-keys -t %3 -l -- 'ab'",
            2,
        ),
        (b"\x1bOPx", "send-keys -t %3 -H 1b 4f 50 78", 1),
        (
            b"\x1b]52;c;?\x07ok",
            "send-keys -t %3 -H 1b 5d 35 32 3b 63 3b 3f 07 ; send-keys -t %3 -l -- 'ok'",
            2,
        ),
        (
            "жук\x7f".as_bytes(),
            "send-keys -t %3 -l -- 'жук' ; send-keys -t %3 -H 7f",
            2,
        ),
        (
            b"ab\xff\xfecd",
            "send-keys -t %3 -l -- 'ab' ; send-keys -t %3 -H ff fe ; send-keys -t %3 -l -- 'cd'",
            3,
        ),
        (b"\x1b", "send-keys -t %3 -H 1b", 1),
        (
            b"it's #{x} ; ~ $HOME \\",
            "send-keys -t %3 -l -- 'it'\\''s #{x} ; ~ $HOME \\'",
            1,
        ),
        (b"a\rb", "send-keys -t %3 -H 61 0d 62", 1),
        (b"-x 200", "send-keys -t %3 -l -- '-x 200'", 1),
    ];
    for &(bytes, expected, count) in cases {
        let mut client = Client::new(false);
        let mut out = Vec::new();
        client.send_keys(PaneId(3), bytes, &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!("{expected}\n"),
            "{bytes:?}"
        );
        assert_eq!(client.sent().len(), count, "{bytes:?}");
        assert!(
            client
                .sent()
                .all(|command| command == Command::SendKeys(PaneId(3)))
        );
    }
    let mut client = Client::new(false);
    let mut out = Vec::new();
    client.send_keys(PaneId(3), b"", &mut out);
    assert!(out.is_empty() && client.sent().len() == 0);
}

#[test]
fn a_key_by_its_name_goes_in_its_place_among_the_bytes() {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    client.send_input(
        PaneId(3),
        &[
            Keys::Bytes(b"ab"),
            Keys::Named("Home"),
            Keys::Named("C-S-End"),
            Keys::Bytes(b"\x1b"),
        ],
        &mut out,
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "send-keys -t %3 -l -- 'ab' ; send-keys -t %3 -- 'Home' ; \
         send-keys -t %3 -- 'C-S-End' ; send-keys -t %3 -H 1b\n"
    );
    assert_eq!(client.sent().len(), 4, "each its reply");
}

#[test]
fn a_name_that_would_break_the_line_is_not_sent() {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    client.send_input(
        PaneId(3),
        &[Keys::Named(""), Keys::Named("Home\nkill-server")],
        &mut out,
    );
    assert!(out.is_empty() && client.sent().len() == 0);
}

#[test]
fn every_byte_value_goes_by_hex_alone() {
    for byte in (0x00..=0x1f).chain([0x7f]).chain(0x80..=0xff) {
        let mut client = Client::new(false);
        let mut out = Vec::new();
        client.send_keys(PaneId(0), &[byte], &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!("send-keys -t %0 -H {byte:02x}\n")
        );
    }
}

#[test]
fn a_name_is_quoted_for_a_command_line() {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    client.rename_window(WindowId(2), "it's a; #{name}\n~", &mut out);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "rename-window -t @2 -- 'it'\\''s a; ##{name}~'\n"
    );
    assert_eq!(
        client.sent().collect::<Vec<_>>(),
        [Command::RenameWindow(WindowId(2))]
    );
}

#[test]
fn commands_of_one_line_are_joined_and_queued_each() {
    let mut client = Client::new(false);
    let mut out = Vec::new();
    let snapshot = [
        Command::Display(PaneId(7)),
        Command::Opening,
        Command::Capture {
            pane: PaneId(7),
            part: Part::Saved,
        },
        Command::Capture {
            pane: PaneId(7),
            part: Part::Visible,
        },
    ];
    client.send_line(&snapshot, &mut out);
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("display -p -t %7 '"), "{text}");
    assert!(
        text.ends_with("' ; capture-pane -p -e -J -a -q -t %7 ; capture-pane -p -e -J -t %7\n"),
        "{text}"
    );
    assert_eq!(text.matches('\n').count(), 1);
    assert_eq!(client.sent().count(), 3);
    let mut empty = Vec::new();
    client.send_line(&[Command::Opening], &mut empty);
    assert!(empty.is_empty());
}
