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
