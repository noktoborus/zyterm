//! What `scp -vv` says, read back into where a transfer stands.
//!
//! `tests/data/scp/` holds what a real `scp` of OpenSSH wrote to stderr: the
//! old protocol and SFTP, each way, and one run at a single `-v`
//! (`tests/data/scp/README.md`). `lib/scp.lua` reads them line by line the way
//! the shipped script reads `scp` while it runs, so the reading is tested
//! against what the program says and not against what it was thought to say.

mod fish;

use fish::{Run, Said, finish, no_line};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use zyt_script::{Cancel, Outcome, Progress, StageKind};

/// The two files of every recording.
const FILES: &str = "one.bin=307200,two.bin=1048576";
const ONE: u64 = 307_200;
const TWO: u64 = 1_048_576;

/// A recording of `tests/data/scp/`.
fn dump(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("scp")
        .join(format!("{name}.txt"))
}

/// Reads one recording back as a run of `scp` with these arguments would.
fn read_back(name: &str, direction: &str, args: &str, files: &str) -> Arc<Said> {
    let said = Said::answering(&[]);
    let mut run = Run::fixture("scp-replay")
        .told("dump", &dump(name).display().to_string())
        .told("direction", direction)
        .told("args", args)
        .told("files", files)
        .start(no_line(), said.clone(), Cancel::new())
        .expect("the script loads");
    assert_eq!(finish(&mut run, Duration::from_secs(10)), Outcome::Done);
    said
}

/// The files the stages named, each with the place it had among the files.
fn files_said(said: &Said) -> Vec<(String, Option<u32>, Option<u32>)> {
    said.stages
        .lock()
        .expect("it is not held")
        .iter()
        .filter(|stage| matches!(stage.kind, StageKind::Sending | StageKind::Receiving))
        .map(|stage| {
            (
                stage.name.clone().unwrap_or_default(),
                stage.file,
                stage.files,
            )
        })
        .collect()
}

/// The bytes said, each with the whole they were said of.
fn bytes_said(said: &Said) -> Vec<(u64, Option<u64>)> {
    said.bytes.lock().expect("it is not held").clone()
}

/// The checks every recording at `-vv` passes: both files named in order,
/// each counted up to its length and never past it, the bytes growing within
/// a file, the transfer closed, and nothing of the debugging in the terminal.
fn read_whole(said: &Said, kind: StageKind) {
    let named: Vec<String> = files_said(said)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(named, ["one.bin", "two.bin"]);
    assert!(
        said.stages
            .lock()
            .expect("it is not held")
            .iter()
            .filter(|stage| stage.name.is_some())
            .all(|stage| stage.kind == kind),
        "every file goes the one way"
    );

    let bytes = bytes_said(said);
    for whole in [ONE, TWO] {
        let of: Vec<u64> = bytes
            .iter()
            .filter(|(_, of)| *of == Some(whole))
            .map(|(done, _)| *done)
            .collect();
        assert!(
            of.len() > 2,
            "{whole}: the bytes are said as they go: {of:?}"
        );
        assert!(of.windows(2).all(|pair| pair[0] <= pair[1]), "{of:?}");
        assert_eq!(of.last(), Some(&whole), "and up to its length: {of:?}");
    }

    let progress = said.progress.lock().expect("it is not held").clone();
    assert!(progress.contains(&Progress::Share(100)), "{progress:?}");
    let kinds = said.stage_kinds();
    assert_eq!(kinds.first(), Some(&StageKind::Negotiating));
    assert_eq!(kinds.last(), Some(&StageKind::Finishing));

    let printed = said.echo();
    assert!(
        !printed.contains("debug1:") && !printed.contains("debug2:"),
        "{printed}"
    );
    assert!(!printed.contains("adjust"), "{printed}");
}

#[test]
fn files_sent_with_the_old_protocol_are_counted_by_what_the_far_end_took() {
    let said = read_back("send-legacy", "send", "-O -vv", FILES);
    read_whole(&said, StageKind::Sending);
    assert_eq!(
        files_said(&said),
        [
            ("one.bin".to_string(), Some(1), Some(2)),
            ("two.bin".to_string(), Some(2), Some(2))
        ],
        "which of how many"
    );
}

#[test]
fn files_sent_over_sftp_are_counted_the_same_way() {
    let said = read_back("send-sftp", "send", "-vv", FILES);
    read_whole(&said, StageKind::Sending);
}

/// Taking files, every remote path is a connection of its own, each counting
/// from nothing.
#[test]
fn files_taken_with_the_old_protocol_are_counted_by_what_came() {
    let said = read_back("receive-legacy", "receive", "-O -vv -T", FILES);
    read_whole(&said, StageKind::Receiving);
}

/// SFTP never says how long a file it takes is: the length is the one the
/// listing of the device gave.
#[test]
fn files_taken_over_sftp_take_their_length_from_the_listing() {
    let said = read_back("receive-sftp", "receive", "-vv", FILES);
    read_whole(&said, StageKind::Receiving);

    let unknown = read_back("receive-sftp", "receive", "-vv", "one.bin,two.bin");
    assert!(
        bytes_said(&unknown)
            .iter()
            .all(|(_, whole)| whole.is_none()),
        "with no length, bytes and no share"
    );
}

/// At one `-v` nothing says the bytes, so the run says nothing of where it
/// stands, and every line goes to the terminal as it did.
#[test]
fn a_run_without_vv_says_no_progress_and_prints_what_scp_says() {
    let said = read_back("send-legacy-v", "send", "-O -v", "one.bin=307200");

    assert!(said.stages.lock().expect("it is not held").is_empty());
    assert!(said.progress.lock().expect("it is not held").is_empty());
    assert!(bytes_said(&said).is_empty());
    assert!(
        said.echo()
            .contains("Sending file modes: C0644 307200 one.bin"),
        "{}",
        said.echo()
    );
}

/// `-v` is counted however it is written, and a `v` that is the value of an
/// option is not one.
#[test]
fn the_verbosity_is_counted_the_way_scp_reads_it() {
    let cases = [
        ("-O -vv", "2"),
        ("-Ov -v", "2"),
        ("-v", "1"),
        ("-O", "0"),
        ("-vvv -P 2222", "3"),
        ("-P 2222 -vv", "2"),
        ("-i -v -v", "1"),
        ("-ivkey -v", "1"),
        ("-o LogLevel=DEBUG2", "0"),
        ("--vv", "0"),
    ];
    let said = Said::answering(&[]);
    let lines: Vec<&str> = cases.iter().map(|(args, _)| *args).collect();
    let mut run = Run::fixture("scp-verbosity")
        .told("cases", &lines.join("\n"))
        .start(no_line(), said.clone(), Cancel::new())
        .expect("the script loads");
    assert_eq!(finish(&mut run, Duration::from_secs(10)), Outcome::Done);

    let expected: Vec<&str> = cases.iter().map(|(_, count)| *count).collect();
    assert_eq!(said.notices(), expected, "{lines:?}");
}
