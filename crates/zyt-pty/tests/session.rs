//! Local console behaviour.

use std::time::{Duration, Instant};
use zyt_pty::{PtyConfig, PtySession};

fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn shell_output_reaches_the_caller() {
    let config = PtyConfig {
        program: Some("sh".to_string()),
        args: vec!["-c".to_string(), "echo marker".to_string()],
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("shell starts");

    let mut text = String::new();
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        session.read_into(&mut chunk);
        text.push_str(&String::from_utf8_lossy(&chunk));
        text.contains("marker")
    }));
}

#[test]
fn input_reaches_the_shell_and_resize_is_accepted() {
    let config = PtyConfig {
        program: Some("sh".to_string()),
        columns: 40,
        rows: 10,
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("shell starts");
    session.resize(100, 30).expect("pty accepts the size");
    session.write(b"echo from-input\n");

    let mut text = String::new();
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        session.read_into(&mut chunk);
        text.push_str(&String::from_utf8_lossy(&chunk));
        text.contains("from-input")
    }));

    session.shutdown().expect("session ends");
}

#[test]
fn a_console_starts_where_this_process_stands() {
    let expected = std::env::current_dir().expect("the process has a directory");
    let config = PtyConfig {
        program: Some("sh".to_string()),
        args: vec!["-c".to_string(), "pwd".to_string()],
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("shell starts");

    let mut text = String::new();
    assert!(
        wait_for(|| {
            let mut chunk = Vec::new();
            session.read_into(&mut chunk);
            text.push_str(&String::from_utf8_lossy(&chunk));
            text.contains(&expected.to_string_lossy().to_string())
        }),
        "the shell printed {text:?}, expected {}",
        expected.display()
    );
}

/// A held console is not read, and what it said while it was held is still
/// there once it is let go of.
///
/// Nothing is thrown away by a hold: the bytes stand in the pipe of the pseudo
/// terminal, the program waits at its next write, and the first reading after the
/// hold is what the program said all along.
///
/// The read already waiting on the pty when the hold arrives is answered once
/// more — the flag is looked at between two reads and not inside one — so the
/// first thing asked for after the hold may still come through, and it is what
/// parks the reading. Whether it does is the shell's own timing: the reading may
/// have parked on something the shell said before it. What is asked for after that
/// one never comes through, and that is what this checks.
#[test]
fn a_held_console_is_read_again_once_it_is_let_go_of() {
    let config = PtyConfig {
        program: Some("sh".to_string()),
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("shell starts");
    session.set_read_hold(true);
    assert!(session.read_hold());

    // One write to answer whatever read was in flight when the hold arrived, and
    // to park the reading behind it. Whether it comes through is the shell's own
    // timing — the reading may have parked before it — so nothing is asked of it.
    session.write(b"echo in-flight\n");
    std::thread::sleep(Duration::from_millis(300));
    let mut chunk = Vec::new();
    session.read_into(&mut chunk);

    session.write(b"echo held-marker\n");
    std::thread::sleep(Duration::from_millis(300));
    session.read_into(&mut chunk);
    assert!(
        !String::from_utf8_lossy(&chunk).contains("held-marker"),
        "and nothing is read after it while the reading is held"
    );

    session.set_read_hold(false);
    assert!(!session.read_hold());

    let mut text = String::new();
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        session.read_into(&mut chunk);
        text.push_str(&String::from_utf8_lossy(&chunk));
        text.contains("held-marker")
    }));

    session.shutdown().expect("session ends");
}

/// A console held by a hold nobody let go of is still ended.
///
/// The hold is a wait, and a wait the end of a session had to ask permission of
/// would be a window that cannot close a console somebody stopped reading.
#[test]
fn a_held_console_still_ends() {
    let config = PtyConfig {
        program: Some("sh".to_string()),
        args: vec!["-c".to_string(), "sleep 30".to_string()],
        ..PtyConfig::default()
    };
    let session = PtySession::spawn(&config, None).expect("shell starts");
    session.set_read_hold(true);

    session.shutdown().expect("session ends");
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        session.read_into(&mut chunk);
        !session.is_running()
    }));
}
