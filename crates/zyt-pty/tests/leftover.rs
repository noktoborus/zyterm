//! A program that leaves something behind holding its pty, which is what a
//! server of tmux does with the terminal of its client.

#![cfg(target_os = "linux")]

use std::time::{Duration, Instant};
use zyt_pty::{PtyConfig, PtySession};

/// Longest a test waits for what it is looking for.
const LIMIT: Duration = Duration::from_secs(5);

fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + LIMIT;
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The threads of this process that read a pty.
fn readers() -> usize {
    std::fs::read_dir("/proc/self/task")
        .expect("the threads of this process")
        .filter_map(|task| task.ok())
        .filter_map(|task| std::fs::read_to_string(task.path().join("comm")).ok())
        .filter(|name| name.trim() == "zyt-pty-read")
        .count()
}

/// The shell leaves a sleeper behind that holds its terminal and ignores the
/// hang-up. The output of the pty never ends while it sleeps, and the session
/// that is dropped stops reading it all the same.
#[test]
fn a_session_that_ends_stops_reading_a_pty_something_else_still_holds() {
    let config = PtyConfig {
        program: Some("/bin/sh".to_string()),
        args: vec![
            "-c".to_string(),
            "(trap '' HUP; echo started; sleep 10) & sleep 0.2; exit 0".to_string(),
        ],
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("the console starts");

    let mut seen = Vec::new();
    let mut buffer = Vec::new();
    assert!(
        wait_for(|| {
            session.read_into(&mut buffer);
            seen.extend_from_slice(&buffer);
            String::from_utf8_lossy(&seen).contains("started")
        }),
        "the sleeper starts"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        session.is_running(),
        "the pty is held, so its output goes on"
    );
    assert_eq!(readers(), 1);

    drop(session);

    assert!(
        wait_for(|| readers() == 0),
        "the reading of a dropped session ends while the pty is still held"
    );
}
