//! The modem scripts against the programs they drive.
//!
//! `sz`, `rz` and the rest are a package (`lrzsz`), so a machine may not have
//! them. When it has not, the assertion is the other half of the same promise:
//! a script that cannot start the program it needs says which program that
//! was, rather than hanging on a line.
//!
//! The device here is a shell under a pseudo terminal, which is what the far
//! end of a serial line is, and the file is carried by the programs
//! themselves — nothing of it passes through the script.

mod fish;

use fish::{Run, console_shell_in, over, workspace};
use std::path::Path;
use std::time::Duration;
use zyt_script::{Outcome, TargetKind, Value};

/// How long a modem is given to carry a few kilobytes over a pseudo terminal.
const PATIENCE: Duration = Duration::from_secs(120);

/// True when the machine has that program.
fn has(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|at| at.join(program).is_file()))
        .unwrap_or(false)
}

/// Runs one modem script and says how it ended.
fn carried(
    script: &str,
    direction: Direction,
    target: (TargetKind, &Path),
    device: &Path,
    answers: &[(&str, Value)],
) -> (Outcome, String) {
    let run = match direction {
        Direction::Send => Run::shipped(script),
        Direction::Receive => Run::shipped(script).receiving(),
    }
    .carrying(target.0, std::slice::from_ref(&target.1.to_path_buf()));

    let (_, outcome, failure) = over(console_shell_in(Some(device)), &run, answers, PATIENCE);
    (outcome, failure)
}

/// Which way a case goes.
#[derive(Debug, Clone, Copy)]
enum Direction {
    /// The file goes to the device.
    Send,
    /// It comes back.
    Receive,
}

#[test]
fn zmodem_carries_a_file_onto_the_device_and_the_bytes_are_the_same() {
    let root = workspace("zmodem-send");
    let here = root.join("blob.bin");
    let bytes: Vec<u8> = (0..20_000).map(|at| (at % 251) as u8).collect();
    std::fs::write(&here, &bytes).expect("written");
    let device = root.join("device");
    std::fs::create_dir_all(&device).expect("made");

    let (outcome, failure) = carried(
        "zmodem",
        Direction::Send,
        (TargetKind::File, &here),
        &device,
        &[],
    );

    if !has("sz") || !has("rz") {
        assert_eq!(outcome, Outcome::Failed(None));
        assert!(
            failure.contains("sz") || failure.contains("rz"),
            "it says which program is missing: {failure}"
        );
        let _ = std::fs::remove_dir_all(&root);
        return;
    }

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(device.join("blob.bin")).expect("it landed"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn zmodem_takes_a_file_off_the_device_once_it_is_told_which() {
    if !has("sz") || !has("rz") {
        return;
    }

    let root = workspace("zmodem-receive");
    let device = root.join("device");
    std::fs::create_dir_all(&device).expect("made");
    let bytes: Vec<u8> = (0..20_000).map(|at| (at % 247) as u8).collect();
    std::fs::write(device.join("come.bin"), &bytes).expect("written");
    let back = root.join("back");
    std::fs::create_dir_all(&back).expect("made");

    let (outcome, failure) = carried(
        "zmodem",
        Direction::Receive,
        (TargetKind::Directory, &back),
        &device,
        &[("remote", Value::Text("sz -b come.bin".to_string()))],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(back.join("come.bin")).expect("it came back"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ymodem_carries_the_name_of_the_file_with_it() {
    if !has("sb") || !has("rb") {
        return;
    }

    let root = workspace("ymodem");
    let here = root.join("named.bin");
    let bytes: Vec<u8> = (0..9_000).map(|at| (at % 241) as u8).collect();
    std::fs::write(&here, &bytes).expect("written");
    let device = root.join("device");
    std::fs::create_dir_all(&device).expect("made");

    let (outcome, failure) = carried(
        "ymodem",
        Direction::Send,
        (TargetKind::File, &here),
        &device,
        &[],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(device.join("named.bin")).expect("it landed under its own name"),
        bytes
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn xmodem_carries_no_length_so_what_arrives_is_padded_to_a_sector() {
    if !has("sx") || !has("rx") {
        return;
    }

    let root = workspace("xmodem");
    let here = root.join("blob.bin");
    let bytes: Vec<u8> = (0..5_000).map(|at| (at % 239) as u8).collect();
    std::fs::write(&here, &bytes).expect("written");
    let device = root.join("device");
    std::fs::create_dir_all(&device).expect("made");

    let (outcome, failure) = carried(
        "xmodem",
        Direction::Send,
        (TargetKind::File, &here),
        &device,
        &[],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let there = std::fs::read(device.join("blob.bin")).expect("it landed");
    assert!(
        there.len() >= bytes.len() && there.len().is_multiple_of(128),
        "{} bytes arrived for {} sent",
        there.len(),
        bytes.len()
    );
    assert_eq!(
        &there[..bytes.len()],
        &bytes[..],
        "the file itself is whole"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The one path that has to work whether the package is installed or not.
#[test]
fn a_script_that_cannot_start_its_program_says_which_one() {
    let root = workspace("missing-program");
    let here = root.join("one.bin");
    std::fs::write(&here, b"one").expect("written");
    let device = root.join("device");
    std::fs::create_dir_all(&device).expect("made");

    let run = Run::fixture("absent").carrying(TargetKind::File, &[here]);
    let (_, outcome, failure) = over(console_shell_in(Some(&device)), &run, &[], PATIENCE);

    assert_eq!(outcome, Outcome::Failed(None));
    assert!(
        failure.contains("nothing-of-the-sort"),
        "it names the program: {failure}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
