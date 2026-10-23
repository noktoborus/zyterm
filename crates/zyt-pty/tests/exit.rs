//! The code a console ended with, which is what tells an ordinary end from a
//! failure.

#![cfg(unix)]

use std::time::{Duration, Instant};
use zyt_pty::{PtyConfig, PtySession};

/// Longest a test waits for a program that was told to leave at once.
const LIMIT: Duration = Duration::from_secs(5);

/// A shell that runs one line and leaves.
fn shell(line: &str) -> PtySession {
    let config = PtyConfig {
        program: Some("/bin/sh".to_string()),
        args: vec!["-c".to_string(), line.to_string()],
        ..PtyConfig::default()
    };
    PtySession::spawn(&config, None).expect("the console starts")
}

/// Reads until the console is over, which is what the application waits for
/// before it asks for the code.
fn until_ended(session: &PtySession) {
    let deadline = Instant::now() + LIMIT;
    let mut buffer = Vec::new();
    while session.is_running() {
        session.read_into(&mut buffer);
        assert!(Instant::now() < deadline, "the console ends");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_program_that_was_done_says_zero() {
    let session = shell("exit 0");

    until_ended(&session);

    assert_eq!(session.exit_code(), Some(0));
}

#[test]
fn a_program_that_failed_says_which_code_it_failed_with() {
    let session = shell("exit 3");

    until_ended(&session);

    assert_eq!(session.exit_code(), Some(3));
}

#[test]
fn a_program_that_is_still_running_says_nothing() {
    let session = shell("sleep 30");

    assert!(session.is_running());
    assert_eq!(session.exit_code(), None);
}

/// Whether the process is gone, or ended and waits only to be reaped.
fn is_gone(pid: &str) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
    }
}

/// A busy console that ignores the hang up is let go of at once, and its
/// program does not live on.
#[test]
fn a_busy_console_is_let_go_of_at_once() {
    let directory = std::env::temp_dir().join(format!("zyt-pty-exit-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a directory for the test");
    let file = directory.join("pid");
    let session = shell(&format!(
        "trap '' HUP TERM; echo $$ > {}; while :; do echo x; done",
        file.display()
    ));
    let deadline = Instant::now() + LIMIT;
    let pid = loop {
        match std::fs::read_to_string(&file) {
            Ok(text) if text.ends_with('\n') => break text.trim().to_string(),
            _ => assert!(Instant::now() < deadline, "the program says its pid"),
        }
        std::thread::sleep(Duration::from_millis(5));
    };

    let started = Instant::now();
    drop(session);
    let took = started.elapsed();

    assert!(took < Duration::from_millis(50), "the drop took {took:?}");
    let deadline = Instant::now() + LIMIT;
    while !is_gone(&pid) {
        assert!(Instant::now() < deadline, "the program is gone");
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = std::fs::remove_dir_all(&directory);
}
