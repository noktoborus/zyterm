//! A journal of the line of zyterm (`docs/journal.md` of zyterm), played
//! through the splitter and the client as the window plays the line.
//!
//! Every recorded session of `tests/data` is written as a journal — a shell
//! before it, `tmux -CC` typed into it — and must play as cleanly as the
//! recording does. A journal of a session that went wrong is played by:
//!
//! ```text
//! ZYTERM_JOURNAL=20261113-142530-4321-line.log \
//!     cargo test -p zyt-tmux --test journal -- --ignored --nocapture
//! ```

mod common;

use std::path::Path;

use common::{Event, Player, commands_of, send, unescape};
use zyt_tmux::{Client, Command};

/// One chunk of the line, as the journal has it.
enum Step {
    /// What the source said.
    Said(Vec<u8>),
    /// What was written to the source.
    Wrote(Vec<u8>),
}

/// The chunks of the screen in a journal, in order; the lines of `#` and the
/// chunks a script wrote while it held the line are not of the window.
fn read(text: &[u8]) -> Vec<Step> {
    text.split(|&byte| byte == b'\n')
        .filter_map(|line| {
            let mut fields = line.splitn(4, |&byte| byte == b' ');
            let mark = fields.next()?;
            let _second = fields.next()?;
            let owner = fields.next()?;
            let bytes = unescape(fields.next().unwrap_or_default());
            match (mark, owner) {
                (b"<", b"screen") => Some(Step::Said(bytes)),
                (b">", b"screen") => Some(Step::Wrote(bytes)),
                _ => None,
            }
        })
        .collect()
}

/// What a play gives: the events, the replies that never came, and the names
/// of the commands it sent without knowing what they answer.
struct Played {
    events: Vec<Event>,
    waiting: usize,
    unnamed: Vec<String>,
}

/// Plays the steps: what the source said goes through the splitter, and what
/// was written while the mode stood is queued in the client as sent, in the
/// order it went.
fn play(steps: &[Step]) -> Played {
    let mut player = Player::new(Client::new(false));
    let mut unnamed = Vec::new();
    for step in steps {
        match step {
            Step::Said(bytes) => player.said(bytes),
            Step::Wrote(bytes) if player.splitter.inside() => {
                for line in bytes.split_inclusive(|&byte| byte == b'\n') {
                    for (name, command) in commands_of(line) {
                        let command = command.unwrap_or_else(|| {
                            unnamed.push(name);
                            Command::Version
                        });
                        send(&mut player.client, command);
                    }
                }
            }
            Step::Wrote(_) => {}
        }
    }
    Played {
        waiting: player.client.sent().len(),
        events: player.record.events,
        unnamed,
    }
}

/// A recording of `tests/data` written as zyterm writes a journal.
fn as_journal(recording: &[u8]) -> Vec<u8> {
    let mut journal = b"# zyterm journal of the line, made from a recording\n".to_vec();
    journal.extend_from_slice(b"# source: console sh\n");
    journal.extend_from_slice(b"< 0.100000 screen $ \n");
    journal.extend_from_slice(b"> 0.200000 screen tmux -CC\\r\n");
    journal.extend_from_slice(b"< 0.210000 screen tmux -CC\\r\\n\n");
    let mut second = 1.0;
    for line in recording.split(|&byte| byte == b'\n') {
        let (mark, rest) = match line {
            [b'<', b' ', rest @ ..] => ("<", rest),
            [b'<'] => ("<", &[][..]),
            [b'>', b' ', rest @ ..] => (">", rest),
            _ => continue,
        };
        second += 0.001;
        journal.extend_from_slice(format!("{mark} {second:.6} screen ").as_bytes());
        journal.extend_from_slice(rest);
        journal.push(b'\n');
    }
    journal.extend_from_slice(b"> 9.000000 run a script after the mode\\r\n");
    journal
}

fn recordings() -> Vec<(String, Vec<u8>)> {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut files: Vec<_> = std::fs::read_dir(data)
        .expect("tests/data")
        .map(|entry| entry.expect("an entry of tests/data").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read(&path).expect("a recording"))
        })
        .collect()
}

#[test]
fn every_recording_written_as_a_journal_plays_cleanly() {
    for (name, recording) in recordings() {
        let played = play(&read(&as_journal(&recording)));
        let errors: Vec<&Event> = played
            .events
            .iter()
            .filter(|event| matches!(event, Event::Error(_)))
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        assert_eq!(played.waiting, 0, "{name}: replies that never came");
        assert!(played.unnamed.is_empty(), "{name}: {:?}", played.unnamed);
        assert!(
            matches!(played.events.first(), Some(Event::Plain(shell)) if shell.starts_with(b"$ ")),
            "{name}: the shell before the mode"
        );
        let entered = played
            .events
            .iter()
            .filter(|event| **event == Event::Entered)
            .count();
        let left = played
            .events
            .iter()
            .filter(|event| **event == Event::Left)
            .count();
        assert_eq!((entered, left), (1, 1), "{name}: one mode");
    }
}

#[test]
#[ignore = "plays the journal ZYTERM_JOURNAL names"]
fn the_journal_named_plays() {
    let Ok(path) = std::env::var("ZYTERM_JOURNAL") else {
        panic!("ZYTERM_JOURNAL names no journal");
    };
    let text = std::fs::read(&path).expect("the journal reads");
    let played = play(&read(&text));
    for event in &played.events {
        match event {
            Event::Output(pane, _, bytes) => {
                println!(
                    "output {pane:?} {}",
                    String::from_utf8_lossy(bytes).escape_debug()
                )
            }
            event => println!("{event:?}"),
        }
    }
    println!(
        "replies never come for {} command(s); sent without a name: {:?}",
        played.waiting, played.unnamed
    );
    let errors: Vec<&Event> = played
        .events
        .iter()
        .filter(|event| matches!(event, Event::Error(_)))
        .collect();
    assert!(errors.is_empty(), "{path}: {errors:?}");
}
