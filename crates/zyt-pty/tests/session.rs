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
