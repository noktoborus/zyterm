//! What a terminal sends when a key is pressed, byte for byte.
//!
//! A key is not what reaches a program. What reaches it is bytes, and what the
//! terminal made of the key — `^C` is one byte and `Alt+x` is two and `F5` is
//! six — is the whole question when something that should have arrived did
//! not. So the tool reads its own standard input and writes down what came,
//! rather than asking a library what key it thinks that was: a library that
//! reads the same bytes cannot say which of them were missing.
//!
//! It is here to be run inside the terminal of this application and inside
//! another one beside it, so the two can be compared. `^C` and `^U` are the
//! two the question is usually about — a program is interrupted by the first
//! and a line is thrown away by the second, and both are a single byte the
//! terminal only sends while it is in raw mode — so they stand in a list of
//! their own with what they have been sent, counted.
//!
//! Nothing is negotiated with the terminal: no keyboard protocol is asked for
//! and no mode is set beyond the raw mode a terminal program needs, because
//! what is wanted is what this terminal sends of its own accord.

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::collections::VecDeque;
use std::io::Read as _;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// How many arrivals are kept to be read back.
const KEPT: usize = 500;

/// Bytes worth their own line, whether they have arrived or not.
///
/// Each of them is a key a terminal answers by sending one byte, and each is a
/// key somebody asks about when a program is not doing what it is told: the
/// first two are the ones this tool was written for.
const WATCHED: &[(u8, &str, &str)] = &[
    (0x03, "^C", "interrupt"),
    (0x15, "^U", "kill the line"),
    (0x04, "^D", "end of file"),
    (0x1a, "^Z", "suspend"),
    (0x1c, "^\\", "quit"),
    (0x17, "^W", "kill the word"),
    (0x12, "^R", "search the history"),
    (0x0c, "^L", "clear"),
    (0x13, "^S", "hold the output"),
    (0x11, "^Q", "let it go on, and leave this tool twice over"),
];

/// The byte that leaves, twice in a row.
const QUIT: u8 = 0x11;

/// How long the first of those two counts for.
const QUIT_WITHIN: Duration = Duration::from_secs(2);

/// How long the tool waits for a key before drawing again anyway.
///
/// What it shows changes when a key arrives and at no other time, so this is
/// not what the window is drawn by: it is what a window resized under a tool
/// nobody is typing into is put right by.
const REDRAW: Duration = Duration::from_secs(1);

/// One read from the terminal: the bytes that arrived together.
///
/// They are kept together because that is how they were sent: an escape
/// sequence is one arrival and not five, and a key held down until it repeats
/// may put several of them in one.
struct Arrival {
    /// When it came, counted from the start of the tool.
    at: Duration,
    /// What came.
    bytes: Vec<u8>,
}

/// Everything the window shows.
struct Seen {
    /// The arrivals, newest last.
    arrivals: VecDeque<Arrival>,
    /// How often each watched byte has come.
    counts: Vec<usize>,
    /// How many bytes have come at all.
    total: usize,
    /// When the tool started.
    started: Instant,
    /// When the byte that leaves last came.
    quit_asked: Option<Instant>,
    /// How many of them have come in a row since then.
    quit_run: usize,
}

impl Seen {
    /// Nothing seen yet.
    fn new() -> Self {
        Self {
            arrivals: VecDeque::new(),
            counts: vec![0; WATCHED.len()],
            total: 0,
            started: Instant::now(),
            quit_asked: None,
            quit_run: 0,
        }
    }

    /// Writes down one arrival and answers whether the tool was asked to leave.
    ///
    /// Asking is the same byte twice, because a tool that watches keys must be
    /// able to show the key that leaves it: the first one is written down like
    /// any other and only the second one is an answer.
    fn add(&mut self, bytes: Vec<u8>) -> bool {
        let now = Instant::now();
        let mut leaving = false;

        for byte in &bytes {
            self.total += 1;
            if let Some(index) = WATCHED.iter().position(|(watched, ..)| watched == byte) {
                self.counts[index] += 1;
            }

            if *byte != QUIT {
                self.quit_asked = None;
                self.quit_run = 0;
                continue;
            }

            let in_time = self
                .quit_asked
                .is_none_or(|asked| now.duration_since(asked) < QUIT_WITHIN);
            self.quit_run = if in_time { self.quit_run + 1 } else { 1 };
            self.quit_asked = Some(now);
            leaving |= self.quit_run >= 2;
        }

        self.arrivals.push_back(Arrival {
            at: now.duration_since(self.started),
            bytes,
        });
        while self.arrivals.len() > KEPT {
            self.arrivals.pop_front();
        }

        leaving
    }
}

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste);

    let outcome = watch(&mut terminal);

    let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    outcome
}

/// Reads until the tool is asked to leave, drawing what came.
fn watch(terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    let (sender, receiver): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = channel();
    std::thread::spawn(move || read_bytes(&sender));

    let mut seen = Seen::new();
    loop {
        terminal.draw(|frame| draw(frame, &seen))?;

        match receiver.recv_timeout(REDRAW) {
            Ok(bytes) => {
                if seen.add(bytes) {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

/// Hands every read of the standard input over, as it comes.
///
/// A read of a terminal in raw mode answers as soon as there is anything to
/// answer with, so what one read holds is what was sent together — and that is
/// what is passed on, unsplit and unjoined.
fn read_bytes(sender: &Sender<Vec<u8>>) {
    let mut input = std::io::stdin().lock();
    let mut buffer = [0_u8; 1024];

    loop {
        match input.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                if sender.send(buffer[..read].to_vec()).is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// Draws one frame: what came last, what came before it, and the count of the
/// bytes that are worth counting.
fn draw(frame: &mut Frame, seen: &Seen) {
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [arrivals, watched] =
        Layout::horizontal([Constraint::Min(30), Constraint::Length(46)]).areas(body);

    frame.render_widget(latest(seen), head);
    frame.render_widget(history(seen), arrivals);
    frame.render_widget(counted(seen), watched);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ^Q ^Q ", Style::default().fg(Color::Black).bg(Color::Gray)),
            Span::raw(" leaves — every other key is only written down "),
        ])),
        foot,
    );
}

/// The arrival that came last, written out large.
fn latest(seen: &Seen) -> Paragraph<'_> {
    let block = Block::bordered().title(" last ");
    let Some(arrival) = seen.arrivals.back() else {
        return Paragraph::new("press a key").block(block);
    };

    let lines = vec![
        Line::from(Span::styled(
            shown(&arrival.bytes),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(hex(&arrival.bytes)),
        Line::from(Span::styled(
            named(&arrival.bytes),
            Style::default().fg(Color::Gray),
        )),
    ];

    Paragraph::new(lines).block(block)
}

/// Everything that came, newest first.
fn history(seen: &Seen) -> List<'_> {
    let items: Vec<ListItem> = seen
        .arrivals
        .iter()
        .rev()
        .map(|arrival| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:8.3} ", arrival.at.as_secs_f64()),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<20}", shown(&arrival.bytes)),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(hex(&arrival.bytes), Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    List::new(items).block(Block::bordered().title(" what came "))
}

/// The watched bytes and how often each of them has come.
fn counted(seen: &Seen) -> List<'_> {
    let items: Vec<ListItem> = WATCHED
        .iter()
        .zip(&seen.counts)
        .map(|((byte, name, means), count)| {
            let color = match count {
                0 => Color::DarkGray,
                _ => Color::Green,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {name:<3}"), Style::default().fg(color)),
                Span::styled(
                    format!("0x{byte:02x} "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{count:>4}  "), Style::default().fg(color)),
                Span::styled((*means).to_string(), Style::default().fg(Color::Gray)),
            ]))
        })
        .collect();

    List::new(items)
        .block(Block::bordered().title(format!(" counted — {} byte(s) in all ", seen.total)))
}

/// One arrival as a line to read: the printable characters as themselves and
/// everything else by its name.
fn shown(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| reading(*byte)).collect()
}

/// One arrival as its bytes, in hexadecimal.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<String>>()
        .join(" ")
}

/// What the bytes of one arrival are called, when they have names worth
/// saying, and the text they are when they are text.
fn named(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes)
        && text.chars().all(|character| !character.is_control())
    {
        return format!("text: {text:?}");
    }

    let names: Vec<&str> = bytes.iter().filter_map(|byte| name_of(*byte)).collect();
    match names.is_empty() {
        true => String::new(),
        false => names.join(" "),
    }
}

/// How one byte is written where a line of them is read.
fn reading(byte: u8) -> String {
    match byte {
        0x1b => "ESC ".to_string(),
        0x00..=0x1f => format!("^{} ", (byte + b'@') as char),
        0x7f => "^? ".to_string(),
        0x20 => "SPC ".to_string(),
        0x21..=0x7e => format!("{} ", byte as char),
        _ => format!("{byte:02x} "),
    }
}

/// What one byte is called, for the bytes that are called anything.
fn name_of(byte: u8) -> Option<&'static str> {
    Some(match byte {
        0x00 => "NUL",
        0x03 => "ETX, interrupt",
        0x04 => "EOT, end of file",
        0x08 => "BS, backspace",
        0x09 => "HT, tab",
        0x0a => "LF, line feed",
        0x0d => "CR, return",
        0x0c => "FF, clear",
        0x11 => "DC1, XON",
        0x13 => "DC3, XOFF",
        0x15 => "NAK, kill the line",
        0x17 => "ETB, kill the word",
        0x1a => "SUB, suspend",
        0x1b => "ESC",
        0x1c => "FS, quit",
        0x7f => "DEL, backspace",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every watched byte is one byte, named once: a list that counted one of
    /// them twice would say a key arrived that never did.
    #[test]
    fn every_watched_byte_stands_once() {
        let mut bytes: Vec<u8> = WATCHED.iter().map(|(byte, ..)| *byte).collect();
        let count = bytes.len();
        bytes.sort_unstable();
        bytes.dedup();

        assert_eq!(bytes.len(), count);
    }

    /// A byte is written the way somebody reading it would write it, and the
    /// ones a terminal sends for a key are told from the ones it sends as text.
    #[test]
    fn a_byte_is_written_the_way_it_is_read() {
        assert_eq!(reading(0x03), "^C ");
        assert_eq!(reading(0x15), "^U ");
        assert_eq!(reading(0x1b), "ESC ");
        assert_eq!(reading(0x7f), "^? ");
        assert_eq!(reading(b'a'), "a ");
        assert_eq!(reading(b' '), "SPC ");

        assert_eq!(hex(&[0x1b, b'[', b'A']), "1b 5b 41");
        assert_eq!(shown(&[0x1b, b'[', b'A']), "ESC [ A ");
    }

    /// What arrives is counted where it is watched and nowhere else, and the
    /// byte that leaves the tool leaves it only the second time.
    #[test]
    fn what_arrives_is_counted_and_leaving_is_asked_for_twice() {
        let mut seen = Seen::new();

        assert!(!seen.add(vec![0x03]));
        assert!(!seen.add(b"hello".to_vec()));
        assert_eq!(seen.total, 6);
        assert_eq!(seen.counts[0], 1, "the interrupt was counted");

        assert!(!seen.add(vec![QUIT]), "one is written down like any other");
        assert!(seen.add(vec![QUIT]), "two in a row is the answer");

        let mut together = Seen::new();
        assert!(
            together.add(vec![QUIT, QUIT]),
            "and two of them in one read are two of them"
        );
    }

    /// A key pressed between the two is a key that was meant, so the tool stays.
    #[test]
    fn a_key_between_the_two_keeps_the_tool() {
        let mut seen = Seen::new();

        assert!(!seen.add(vec![QUIT]));
        assert!(!seen.add(vec![b'x']));
        assert!(!seen.add(vec![QUIT]));
    }
}
