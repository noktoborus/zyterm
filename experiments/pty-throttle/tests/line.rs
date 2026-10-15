//! The tool run as a user runs it, with a command that writes and leaves.

#![cfg(unix)]

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn throttle(args: &[&str]) -> (Output, Duration) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_pty-throttle"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("pty-throttle runs");
    (output, started.elapsed())
}

#[test]
fn bytes_take_the_time_of_the_line() {
    let (output, took) = throttle(&[
        "--quiet",
        "-b",
        "19200",
        "--",
        "sh",
        "-c",
        "printf '%384s' ''",
    ]);
    assert_eq!(output.stdout, vec![b' '; 384]);
    assert!(took >= Duration::from_millis(190), "{took:?}");
    assert!(took < Duration::from_secs(2), "{took:?}");
}

#[test]
fn a_frame_of_seven_bits_loses_the_eighth() {
    let (output, _) = throttle(&[
        "--quiet",
        "-b",
        "115200",
        "-f",
        "7E1",
        "--",
        "printf",
        "\\321\\234",
    ]);
    assert_eq!(output.stdout, [0x51, 0x1c]);
}

#[test]
fn the_terminal_of_the_command_is_told_the_line() {
    let (output, _) = throttle(&["--quiet", "-b", "57200", "--", "stty", "-a"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("speed 57600 baud"), "{text}");
    assert!(text.contains("rows 24; columns 80"), "{text}");
}

#[test]
fn the_command_has_a_controlling_terminal() {
    let (output, _) = throttle(&["--quiet", "--", "sh", "-c", "tty && ps -o stat= -p $$"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("/dev/pts/"), "{text}");
    assert!(text.contains('s'), "a session leader: {text}");
}

#[test]
fn the_code_of_the_command_is_the_code() {
    let (output, _) = throttle(&["--quiet", "--", "sh", "-c", "exit 3"]);
    assert_eq!(output.status.code(), Some(3));
    let (output, _) = throttle(&["--quiet", "--", "no-such-command-here"]);
    assert_eq!(output.status.code(), Some(127));
}
