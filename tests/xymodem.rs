//! The XModem and YModem scripts against `lrzsz` on the device.
//!
//! This side speaks both protocols itself (`scripts/lib/xymodem.lua`); the
//! device is a shell under a pseudo terminal running `rx`, `sx`, `rb` and `sb`,
//! which is what the far end of a serial line most often runs. A machine
//! without `lrzsz` has no device to talk to, and the cases are skipped there.

mod fish;

use fish::{Run, Said, console_shell_in, over, workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use zyt_script::{Outcome, TargetKind, Value};

/// How long a transfer is given over a pseudo terminal.
const PATIENCE: Duration = Duration::from_secs(120);

/// True when the machine has the programs of `lrzsz`, which are the device of
/// these cases.
fn has_lrzsz() -> bool {
    ["sx", "rx", "sb", "rb"].iter().all(|program| {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {program}"))
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    })
}

/// Bytes of a file that are not one byte repeated, so a shifted or truncated
/// copy does not compare equal.
fn pattern(length: usize, modulus: usize) -> Vec<u8> {
    (0..length).map(|at| (at % modulus) as u8).collect()
}

/// A case: a directory of its own, the device standing in `device`, files
/// coming back into `back`.
struct Case {
    root: PathBuf,
    device: PathBuf,
    back: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let root = workspace(name);
        let device = root.join("device");
        let back = root.join("back");
        std::fs::create_dir_all(&device).expect("made");
        std::fs::create_dir_all(&back).expect("made");
        Self { root, device, back }
    }

    /// Runs `script` carrying `target`, answering its form with `answers`.
    fn run(
        &self,
        run: Run,
        target: (TargetKind, &Path),
        answers: &[(&str, Value)],
    ) -> (Arc<Said>, Outcome, String) {
        let run = run.picking(std::slice::from_ref(&target.1.to_path_buf()));
        over(
            console_shell_in(Some(&self.device)),
            &run,
            answers,
            PATIENCE,
        )
    }

    /// What came back, by name, sorted.
    fn back_names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.back)
            .expect("it is there")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The command the device is told to run.
fn remote(command: &str) -> (&'static str, Value) {
    ("remote", Value::Text(command.to_string()))
}

/// What XModem leaves of a file: the file, padded to a whole block.
fn padded(there: &[u8], bytes: &[u8]) {
    assert!(
        there.len() >= bytes.len() && there.len().is_multiple_of(128),
        "{} bytes arrived for {}",
        there.len(),
        bytes.len()
    );
    assert!(
        there.len() - bytes.len() < 1024,
        "padded by less than a block"
    );
    assert_eq!(&there[..bytes.len()], bytes, "the file itself is whole");
}

/// Sends `bytes` with XModem to a device running `command`.
fn xmodem_onto_the_device(name: &str, bytes: &[u8], command: &str, large: bool) {
    let case = Case::new(name);
    let here = case.root.join("blob.bin");
    std::fs::write(&here, bytes).expect("written");

    let (_, outcome, failure) = case.run(
        Run::shipped("xmodem"),
        (TargetKind::File, &here),
        &[remote(command), ("large", Value::Flag(large))],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    padded(
        &std::fs::read(case.device.join("blob.bin")).expect("it landed"),
        bytes,
    );
}

#[test]
fn xmodem_goes_with_a_sum_to_a_receiver_asking_for_one() {
    if !has_lrzsz() {
        return;
    }
    xmodem_onto_the_device("xy-x-sum", &pattern(5_000, 239), "rx blob.bin", false);
}

#[test]
fn xmodem_goes_with_a_crc_and_past_block_255() {
    if !has_lrzsz() {
        return;
    }
    xmodem_onto_the_device("xy-x-crc", &pattern(100_000, 239), "rx -c blob.bin", false);
}

#[test]
fn xmodem_goes_in_blocks_of_1k() {
    if !has_lrzsz() {
        return;
    }
    xmodem_onto_the_device("xy-x-1k", &pattern(100_000, 241), "rx -c blob.bin", true);
}

#[test]
fn xmodem_with_1k_blocks_falls_back_to_128_for_a_receiver_asking_for_a_sum() {
    if !has_lrzsz() {
        return;
    }
    xmodem_onto_the_device("xy-x-1k-sum", &pattern(3_000, 241), "rx blob.bin", true);
}

#[test]
fn xmodem_takes_a_file_off_the_device() {
    if !has_lrzsz() {
        return;
    }
    for (name, command) in [
        ("xy-x-back", "sx blob.bin"),
        ("xy-x-back-1k", "sx -k blob.bin"),
    ] {
        let case = Case::new(name);
        let bytes = pattern(70_000, 251);
        std::fs::write(case.device.join("blob.bin"), &bytes).expect("written");
        let here = case.back.join("came.bin");

        let (said, outcome, failure) = case.run(
            Run::shipped("xmodem").receiving(),
            (TargetKind::File, &here),
            &[remote(command)],
        );

        assert_eq!(outcome, Outcome::Done, "{command}: {failure}");
        padded(&std::fs::read(&here).expect("it came"), &bytes);
        assert!(
            said.notices().iter().any(|notice| notice.contains("took")),
            "{:?}",
            said.notices()
        );
    }
}

#[test]
fn ymodem_carries_the_name_and_the_time_of_the_file() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-y-send");
    let here = case.root.join("named.bin");
    let bytes = pattern(100_000, 241);
    std::fs::write(&here, &bytes).expect("written");
    let then = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    std::fs::File::options()
        .write(true)
        .open(&here)
        .and_then(|file| file.set_modified(then))
        .expect("dated");

    let (_, outcome, failure) = case.run(
        Run::shipped("ymodem"),
        (TargetKind::Files, &here),
        &[remote("rb")],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let there = case.device.join("named.bin");
    assert_eq!(
        std::fs::read(&there).expect("it landed under its own name"),
        bytes,
        "and exactly as long as it was"
    );
    assert_eq!(
        std::fs::metadata(&there)
            .and_then(|data| data.modified())
            .expect("dated"),
        then
    );
}

#[test]
fn ymodem_sends_several_files_in_one_batch() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-y-several");
    let first = case.root.join("first.bin");
    let second = case.root.join("second.bin");
    std::fs::write(&first, pattern(3_000, 13)).expect("written");
    std::fs::write(&second, pattern(5_000, 17)).expect("written");

    let run = Run::shipped("ymodem").picking(&[first.clone(), second.clone()]);
    let (said, outcome, failure) = over(
        console_shell_in(Some(&case.device)),
        &run,
        &[remote("rb")],
        PATIENCE,
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    for here in [&first, &second] {
        let name = here.file_name().expect("a name");
        assert_eq!(
            std::fs::read(case.device.join(name)).expect("it landed"),
            std::fs::read(here).expect("it is here"),
        );
    }
    assert_eq!(said.staged_names(), ["first.bin", "second.bin"]);
}

#[test]
fn ymodem_sends_an_empty_file() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-y-empty");
    let here = case.root.join("empty.bin");
    std::fs::write(&here, b"").expect("written");

    let (_, outcome, failure) = case.run(
        Run::shipped("ymodem"),
        (TargetKind::Files, &here),
        &[remote("rb")],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("empty.bin")).expect("it landed"),
        b""
    );
}

#[test]
fn ymodem_takes_every_file_the_device_sends_whole() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-y-back");
    let one = pattern(70_000, 251);
    let two = pattern(1_000, 7);
    std::fs::write(case.device.join("one.bin"), &one).expect("written");
    std::fs::write(case.device.join("two.bin"), &two).expect("written");
    std::fs::write(case.device.join("none.bin"), b"").expect("written");

    let (said, outcome, failure) = case.run(
        Run::shipped("ymodem").receiving(),
        (TargetKind::Directory, &case.back.clone()),
        &[remote("sb one.bin two.bin none.bin")],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        said.staged_names(),
        ["one.bin", "two.bin", "none.bin"],
        "each file is said as it comes"
    );
    let kinds = said.stage_kinds();
    assert_eq!(kinds.first(), Some(&zyt_script::StageKind::Telling));
    assert_eq!(kinds.last(), Some(&zyt_script::StageKind::Finishing));
    assert_eq!(case.back_names(), ["none.bin", "one.bin", "two.bin"]);
    assert_eq!(std::fs::read(case.back.join("one.bin")).expect("read"), one);
    assert_eq!(std::fs::read(case.back.join("two.bin")).expect("read"), two);
    assert_eq!(
        std::fs::read(case.back.join("none.bin")).expect("read"),
        b""
    );
}

#[test]
fn ymodem_lands_a_file_beside_one_already_here() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-y-beside");
    let bytes = pattern(3_000, 13);
    std::fs::write(case.device.join("a.bin"), &bytes).expect("written");
    std::fs::write(case.back.join("a.bin"), b"old").expect("written");

    let (_, outcome, failure) = case.run(
        Run::shipped("ymodem").receiving(),
        (TargetKind::Directory, &case.back.clone()),
        &[remote("sb a.bin")],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["a.bin", "a.bin.1"]);
    assert_eq!(
        std::fs::read(case.back.join("a.bin")).expect("read"),
        b"old"
    );
    assert_eq!(
        std::fs::read(case.back.join("a.bin.1")).expect("read"),
        bytes
    );
}

/// Starts a transfer, stops it once it has moved, and makes sure the shell of
/// the device answers again — which it does only when the program there was
/// told to stop rather than left waiting for blocks.
fn stopped_midway(case: &Case, run: Run, answers: &[(&str, Value)]) {
    let line = console_shell_in(Some(&case.device));
    let said = Said::answering(answers);
    let mut running = run
        .start(line.clone(), said.clone(), zyt_script::Cancel::new())
        .expect("the script loads");

    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !said
        .progress
        .lock()
        .expect("it is not held")
        .iter()
        .any(|progress| matches!(progress, zyt_script::Progress::Share(share) if *share > 0))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the transfer never moved"
        );
        assert!(!running.tick(), "it ended before it was stopped");
        std::thread::sleep(Duration::from_millis(5));
    }
    running.cancel();
    assert_eq!(
        fish::finish(&mut running, Duration::from_secs(30)),
        Outcome::Cancelled
    );

    std::thread::sleep(Duration::from_secs(1));
    let mut heard = Vec::new();
    let mut spare = Vec::new();
    line.read(&mut spare, Duration::from_millis(200));
    line.write(b"\recho al''ive\r");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !String::from_utf8_lossy(&heard).contains("alive") {
        assert!(
            std::time::Instant::now() < deadline,
            "the shell did not come back: {:?}",
            String::from_utf8_lossy(&heard)
        );
        line.read(&mut spare, Duration::from_millis(100));
        heard.extend_from_slice(&spare);
    }
}

#[test]
fn a_send_stopped_midway_leaves_the_device_at_its_shell() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-stop-send");
    let here = case.root.join("long.bin");
    std::fs::write(&here, pattern(8 * 1024 * 1024, 253)).expect("written");

    stopped_midway(
        &case,
        Run::shipped("ymodem").picking(std::slice::from_ref(&here)),
        &[remote("rb")],
    );
}

#[test]
fn a_receive_stopped_midway_leaves_the_device_at_its_shell() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("xy-stop-receive");
    std::fs::write(case.device.join("long.bin"), pattern(8 * 1024 * 1024, 251)).expect("written");

    stopped_midway(
        &case,
        Run::shipped("ymodem")
            .receiving()
            .picking(std::slice::from_ref(&case.back)),
        &[remote("sb long.bin")],
    );
}
