//! The bash integration marks what a real bash does, and the terminal reads it.
//!
//! `assets/osc/osc133-bash.sh` is shell nothing else in this repository runs,
//! so nothing else would notice it breaking. It is sourced into a real bash on
//! a pseudo terminal here, commands are typed at it, and what comes back is
//! fed to the emulator that has to make a command history out of it.
//!
//! The configurations are the point. A shell that already has a
//! `PROMPT_COMMAND` is the ordinary case — Fedora, Debian and Ubuntu all set
//! one — and it is the case a DEBUG trap gets wrong, because the trap fires
//! for that command too and spends the mark meant for what the user typed.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// A PS0 of someone else's, in the shape systemd gives its own: a command
/// substitution that writes an OSC 3008 context for every command run.
const PS0_OF_SOMEONE_ELSE: &str = r"__ps0_of_someone_else() { printf '\033]3008;start=command\033\\'; }; PS0='$(__ps0_of_someone_else)'";

/// What the shell is told before the marks are loaded.
struct Setup {
    name: &'static str,
    before: &'static str,
}

const SETUPS: &[Setup] = &[
    Setup {
        name: "a shell with nothing of its own",
        before: "",
    },
    Setup {
        name: "a shell whose PROMPT_COMMAND is a string",
        before: "PROMPT_COMMAND='printf \"\\033]0;%s\\007\" \"$PWD\"'",
    },
    Setup {
        name: "a shell whose PROMPT_COMMAND is an array",
        before: "PROMPT_COMMAND=(true true)",
    },
    Setup {
        name: "a shell whose prompt runs a command of its own",
        before: "PS1='[$(printf sub)] \\$ '",
    },
    Setup {
        name: "a shell that already has a DEBUG trap",
        before: "trap 'true' DEBUG",
    },
    Setup {
        name: "a shell whose PS0 is already spoken for",
        before: PS0_OF_SOMEONE_ELSE,
    },
];

/// Drives a bash on a pseudo terminal and answers everything it wrote, from
/// its first prompt on.
///
/// Everything is kept, the setting up as well as the commands: the mark that
/// opens a command sits at the end of the prompt drawn before it, so a capture
/// that began after that prompt would lose the first command and blame the
/// shell for it.
fn bash_says(before: &str, lines: &[&str]) -> Vec<u8> {
    let pair = portable_pty::native_pty_system()
        .openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("a pseudo terminal opens");

    let mut command = portable_pty::CommandBuilder::new("bash");
    command.args(["--norc", "--noprofile", "-i"]);
    command.env("TERM", "xterm-256color");
    let mut child = pair
        .slave
        .spawn_command(command)
        .expect("a bash starts on it");
    let mut reader = pair.master.try_clone_reader().expect("it can be read");
    let mut writer = pair.master.take_writer().expect("it can be written");

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                return;
            }
        }
    });

    let settle = |receiver: &std::sync::mpsc::Receiver<Vec<u8>>, out: &mut Vec<u8>| {
        let deadline = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(chunk) => out.extend_from_slice(&chunk),
                Err(_) => return,
            }
        }
    };

    let mut said = Vec::new();
    settle(&receiver, &mut said);
    if !before.is_empty() {
        writeln!(writer, "{before}").expect("the shell is told");
        settle(&receiver, &mut said);
    }
    writeln!(writer, ". {}", script_path().display()).expect("the marks are loaded");
    settle(&receiver, &mut said);

    for line in lines {
        writeln!(writer, "{line}").expect("the command is typed");
        settle(&receiver, &mut said);
    }

    let _ = child.kill();
    let _ = child.wait();
    said
}

fn script_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/osc/osc133-bash.sh")
}

/// The marks in what the shell wrote, in the order they were written.
fn marks(said: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(said);
    let mut found = Vec::new();
    let mut rest = text.as_ref();
    while let Some(at) = rest.find("\u{1b}]133;") {
        rest = &rest[at + "\u{1b}]133;".len()..];
        let end = rest.find(['\u{7}', '\u{1b}']).unwrap_or(rest.len());
        found.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    found
}

/// The commands the emulator read back out of what the shell wrote.
fn commands(said: &[u8]) -> Vec<String> {
    let mut terminal = zyt_term::Terminal::new(zyt_term::TerminalConfig {
        columns: 80,
        rows: 24,
        scrollback: 1000,
        clipboard: zyt_term::ClipboardAccess::CopyPaste,
    })
    .expect("a terminal is made");
    terminal.feed(said);
    terminal
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            zyt_term::TerminalEvent::Command(line) => Some(line),
            _ => None,
        })
        .collect()
}

#[test]
fn every_command_of_a_real_bash_is_marked_and_read_back() {
    for setup in SETUPS {
        let said = bash_says(setup.before, &["echo one", "", "false"]);

        assert_eq!(
            marks(&said),
            vec![
                "A", "B", // the prompt drawn once the marks were loaded
                "C", "D;0", "A", "B", // echo one
                "A", "B", // the empty line, which ran nothing
                "C", "D;1", "A", "B", // false
            ],
            "{}: the marks did not come in the order the protocol asks for",
            setup.name
        );

        assert_eq!(
            commands(&said),
            vec!["echo one".to_string(), "false".to_string()],
            "{}: the terminal did not read the commands back",
            setup.name
        );
    }
}

#[test]
fn an_empty_line_opens_a_prompt_and_closes_no_command() {
    let said = bash_says("", &["", "", ""]);
    let marks = marks(&said);

    assert!(
        !marks.iter().any(|mark| mark.starts_with('D')),
        "an empty line closed a command that never ran: {marks:?}"
    );
    assert!(
        !marks.iter().any(|mark| mark == "C"),
        "an empty line opened the output of a command that never ran: {marks:?}"
    );
    assert!(commands(&said).is_empty(), "an empty line is no command");
}

#[test]
fn a_key_bound_to_a_command_does_not_spend_the_mark_of_the_next_one() {
    let said = bash_says("bind -x '\"\\C-t\": true'", &["\u{14}echo one"]);

    assert_eq!(
        commands(&said),
        vec!["echo one".to_string()],
        "a command bound to a key took the mark the typed command needed"
    );
}

/// PS0 belongs to whoever was there first.
///
/// systemd writes the context of every command into it — OSC 3008, installed
/// as `/etc/profile.d/80-systemd-osc-context.sh` and on by default since
/// systemd 257. Replacing PS0 rather than adding to it leaves that session
/// with a start for every prompt and none for any command, and says nothing
/// about it.
#[test]
fn a_ps0_that_was_already_there_still_runs() {
    let said = bash_says(PS0_OF_SOMEONE_ELSE, &["echo one", "false"]);
    let text = String::from_utf8_lossy(&said);

    assert_eq!(
        text.matches("\u{1b}]3008;start=command\u{1b}\\\u{1b}]133;C\u{7}")
            .count(),
        2,
        "C did not come after the PS0 the shell already had: {text:?}"
    );
    assert_eq!(
        commands(&said),
        vec!["echo one".to_string(), "false".to_string()],
        "the marks stopped working beside a PS0 of someone else"
    );
}

#[test]
fn the_exit_code_of_a_command_is_the_one_the_shell_saw() {
    let said = bash_says("", &["(exit 7)"]);

    assert!(
        marks(&said).contains(&"D;7".to_string()),
        "the exit code did not survive the marking: {:?}",
        marks(&said)
    );
}

#[test]
fn what_the_shell_already_had_is_left_working() {
    let said = bash_says(
        "PROMPT_COMMAND='printf \"\\033]0;%s\\007\" title'",
        &["echo one"],
    );
    let text = String::from_utf8_lossy(&said);

    assert!(
        text.contains("\u{1b}]0;title\u{7}"),
        "the PROMPT_COMMAND the shell already had stopped running"
    );
}

#[test]
fn loading_the_marks_twice_marks_everything_once() {
    let said = bash_says(&format!(". {}", script_path().display()), &["echo one"]);
    let marks = marks(&said);

    assert_eq!(
        commands(&said)
            .iter()
            .filter(|line| *line == "echo one")
            .count(),
        1,
        "the file was loaded twice and the command was read back twice: {marks:?}"
    );
    assert!(
        marks.windows(2).all(|pair| pair[0] != pair[1]),
        "the file was loaded twice and wrote a mark twice over: {marks:?}"
    );
}
