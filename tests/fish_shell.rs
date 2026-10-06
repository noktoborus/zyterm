//! The shell transfer against a real shell, over a pipe and over a console.
//!
//! The device here is this machine: a `sh` reading its commands, which is what
//! the far end of the line always is. A pipe is what an `ssh` session is like
//! and a pseudo terminal is what a console is like, and the two differ in the
//! ways that decide this protocol — an echo of everything written, and a line
//! `stty` can switch to binary.

mod fish;

use fish::{Run, console_shell, console_shell_in, over, piped_shell, workspace};
use std::path::PathBuf;
use std::time::Duration;
use zyt_script::{Outcome, TargetKind, Value};

/// How long a round trip of a few kilobytes is given.
const PATIENCE: Duration = Duration::from_secs(60);

/// What a transfer is answered when the test does not care.
fn answers(remote: &str, mode: &str, digest: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("remote", Value::Text(remote.to_string())),
        ("mode", Value::One(mode.to_string())),
        ("digest", Value::One(digest.to_string())),
        ("verbose", Value::Flag(false)),
    ]
}

/// Sends those paths to that directory of the device and says what happened.
fn sent(
    line: std::sync::Arc<dyn zyt_script::Line>,
    paths: &[PathBuf],
    remote: &str,
    mode: &str,
    digest: &str,
) -> (Outcome, String) {
    let run = Run::shipped("shell-transfer").carrying(TargetKind::Files, paths);
    let (_, outcome, failure) = over(line, &run, &answers(remote, mode, digest), PATIENCE);
    (outcome, failure)
}

#[test]
fn a_file_crosses_a_pipe_as_base64_and_arrives_whole() {
    let root = workspace("pipe-base64");
    let here = root.join("one.txt");
    std::fs::write(&here, b"one two three\n").expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        piped_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "base64",
        "auto",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("one.txt")).expect("it landed"),
        b"one two three\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_value_of_a_byte_crosses_a_console_as_base64() {
    let root = workspace("console-bytes");
    let here = root.join("bytes.bin");
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(&here, &bytes).expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        console_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "base64",
        "sha256sum",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("bytes.bin")).expect("it landed"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_value_of_a_byte_crosses_a_console_raw() {
    let root = workspace("console-raw");
    let here = root.join("bytes.bin");
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(&here, &bytes).expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        console_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "raw",
        "md5sum",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("bytes.bin")).expect("it landed"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pipe_cannot_carry_raw_and_says_so() {
    let root = workspace("pipe-raw");
    let here = root.join("one.txt");
    std::fs::write(&here, b"one\n").expect("written");

    let (outcome, failure) = sent(
        piped_shell(),
        std::slice::from_ref(&here),
        root.join("device").to_str().expect("a path of letters"),
        "raw",
        "none",
    );

    assert_eq!(outcome, Outcome::Failed(None));
    assert!(failure.contains("stty"), "{failure}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_empty_file_crosses_and_stays_empty() {
    let root = workspace("empty");
    let here = root.join("nothing.bin");
    std::fs::write(&here, b"").expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        piped_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "base64",
        "none",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("nothing.bin")).expect("it landed"),
        Vec::<u8>::new()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_name_with_a_space_and_a_quote_in_it_stays_one_word() {
    let root = workspace("quoting");
    let here = root.join("it's two words.txt");
    std::fs::write(&here, b"quoted\n").expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        piped_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "base64",
        "none",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("it's two words.txt")).expect("it landed"),
        b"quoted\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_shape_of_a_tree_is_kept_on_the_way_over() {
    let root = workspace("tree");
    let here = root.join("tree");
    std::fs::create_dir_all(here.join("under")).expect("made");
    std::fs::write(here.join("one.txt"), b"one\n").expect("written");
    std::fs::write(here.join("under/two.txt"), b"two\n").expect("written");
    let there = root.join("device");

    let (outcome, failure) = sent(
        piped_shell(),
        std::slice::from_ref(&here),
        there.to_str().expect("a path of letters"),
        "base64",
        "none",
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("tree/under/two.txt")).expect("it landed"),
        b"two\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_chunk_of_one_line_still_carries_the_whole_file() {
    let root = workspace("tiny-chunk");
    let here = root.join("many.bin");
    let bytes: Vec<u8> = (0..400).map(|at| (at % 251) as u8).collect();
    std::fs::write(&here, &bytes).expect("written");
    let there = root.join("device");

    let run =
        Run::shipped("shell-transfer").carrying(TargetKind::Files, std::slice::from_ref(&here));
    let mut answered = answers(there.to_str().expect("a path of letters"), "base64", "none");
    answered.push(("chunk", Value::Text("77".to_string())));
    let (_, outcome, failure) = over(piped_shell(), &run, &answered, PATIENCE);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(there.join("many.bin")).expect("it landed"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_comes_back_off_a_console_with_its_tree() {
    let root = workspace("back");
    let there = root.join("device");
    std::fs::create_dir_all(there.join("under")).expect("made");
    let bytes: Vec<u8> = (0..1000).map(|at| (at % 253) as u8).collect();
    std::fs::write(there.join("blob.bin"), &bytes).expect("written");
    std::fs::write(there.join("under/leaf.txt"), b"leaf\n").expect("written");
    let here = root.join("back");
    std::fs::create_dir_all(&here).expect("made");

    let run = Run::shipped("shell-transfer")
        .receiving()
        .carrying(TargetKind::Directory, std::slice::from_ref(&here));
    let (_, outcome, failure) = over(
        console_shell(),
        &run,
        &answers(there.to_str().expect("a path of letters"), "base64", "auto"),
        PATIENCE,
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(here.join("device/blob.bin")).expect("it came back"),
        bytes
    );
    assert_eq!(
        std::fs::read(here.join("device/under/leaf.txt")).expect("it came back"),
        b"leaf\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_the_device_has_not_got_is_refused_rather_than_guessed_at() {
    let root = workspace("missing");
    let here = root.join("back");
    std::fs::create_dir_all(&here).expect("made");

    let run = Run::shipped("shell-transfer")
        .receiving()
        .carrying(TargetKind::Directory, std::slice::from_ref(&here));
    let (said, outcome, failure) = over(
        piped_shell(),
        &run,
        &answers(
            root.join("nothing-there").to_str().expect("a path"),
            "base64",
            "none",
        ),
        PATIENCE,
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert!(
        said.errors().iter().any(|said| said.contains("nothing")),
        "{:?}",
        said.errors()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A console is not a pipe: it holds one line at a time, drops what does not
/// fit, and its line discipline is free to add a newline of its own at the
/// end. What travels over it plainly is therefore text of ordinary lines, and
/// it travels believed rather than checked -- which is what `cat-file` says of
/// itself, and why the protocol of `shell-transfer` encodes instead.
#[test]
fn a_program_handed_to_the_line_carries_the_file_itself() {
    let root = workspace("attached");
    let here = root.join("one.txt");
    let text: String = (0..40)
        .map(|at| format!("line {at} of a file a console can carry\n"))
        .collect();
    let bytes = text.into_bytes();
    std::fs::write(&here, &bytes).expect("written");
    let there = root.join("device");
    std::fs::create_dir_all(&there).expect("made");

    let run = Run::shipped("cat-file").carrying(TargetKind::File, std::slice::from_ref(&here));
    let (_, outcome, failure) = over(console_shell_in(Some(&there)), &run, &[], PATIENCE);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let landed = there.join("one.txt");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::fs::metadata(&landed)
        .map(|data| data.len())
        .unwrap_or(0)
        < bytes.len() as u64
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the device is still writing what the line carried"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let there = std::fs::read(&landed).expect("it landed");
    assert_eq!(
        String::from_utf8_lossy(&there).trim_end_matches('\n'),
        String::from_utf8_lossy(&bytes).trim_end_matches('\n'),
        "every line of the file crossed the console"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_listing_of_the_device_says_what_is_there() {
    let root = workspace("listing");
    std::fs::create_dir_all(root.join("under")).expect("made");
    std::fs::write(root.join("one.txt"), b"one\n").expect("written");

    let run = Run::shipped("shell-list");
    let (said, outcome, failure) = over(
        piped_shell(),
        &run,
        &[("remote", Value::Text(root.to_string_lossy().to_string()))],
        PATIENCE,
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let echo = said.echo();
    assert!(echo.contains("one.txt"), "{echo}");
    assert!(echo.contains("under"), "{echo}");
    let _ = std::fs::remove_dir_all(&root);
}
