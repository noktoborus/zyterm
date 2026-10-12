//! The ZMODEM script against `lrzsz` on the device.
//!
//! This side speaks the protocol itself (`zyt.zmodem`); the device is a shell
//! under a pseudo terminal running `rz` and `sz`, which is what the far end of
//! a serial line most often runs. A machine without `lrzsz` has no device to
//! talk to, and the cases are skipped there.

mod fish;

use fish::{Run, Said, console_shell_in, over, workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use zyt_script::{Direction, Outcome, TargetKind, Value};

/// How long a transfer of a few kilobytes is given over a pseudo terminal.
const PATIENCE: Duration = Duration::from_secs(120);

/// True when the machine has `lrzsz`, which is the device of these cases.
fn has_lrzsz() -> bool {
    ["sz", "rz"].iter().all(|program| {
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

    /// Runs the script that way, answering its forms with `answers`.
    fn run(
        &self,
        direction: Direction,
        target: Option<(TargetKind, &Path)>,
        answers: &[(&str, Value)],
    ) -> (Arc<Said>, Outcome, String) {
        let mut run = Run::shipped("zmodem");
        if direction == Direction::Receive {
            run = run.receiving();
        }
        if let Some((kind, path)) = target {
            run = run.carrying(kind, std::slice::from_ref(&path.to_path_buf()));
        }
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

/// The answers of the receiving form, by what they are about.
fn receiving(
    into: &Path,
    listing: &str,
    two_pass: bool,
    remote: &str,
) -> Vec<(&'static str, Value)> {
    vec![
        ("into", Value::Text(into.display().to_string())),
        ("listing", Value::One(listing.to_string())),
        ("two_pass", Value::Flag(two_pass)),
        ("use_remote", Value::Flag(true)),
        ("remote", Value::Text(remote.to_string())),
    ]
}

/// The names of the forms the script asked, in order.
fn forms(said: &Said) -> Vec<String> {
    said.asked().iter().map(|form| form.id.clone()).collect()
}

#[test]
fn a_file_goes_onto_the_device_and_the_bytes_are_the_same() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-send");
    let here = case.root.join("blob.bin");
    let bytes = pattern(70_000, 251);
    std::fs::write(&here, &bytes).expect("written");

    let (said, outcome, failure) = case.run(
        Direction::Send,
        None,
        &[
            ("path", Value::Text(here.display().to_string())),
            ("remote", Value::Text("rz -y".to_string())),
        ],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("blob.bin")).expect("it landed"),
        bytes
    );
    let send = said
        .asked()
        .into_iter()
        .find(|form| form.id == "send")
        .expect("the form was asked");
    assert!(send.always, "it comes up every time");
}

#[test]
fn a_dropped_file_is_what_the_form_offers() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-dropped");
    let here = case.root.join("dropped.bin");
    let bytes = pattern(3_000, 13);
    std::fs::write(&here, &bytes).expect("written");

    let (said, outcome, failure) = case.run(Direction::Send, Some((TargetKind::File, &here)), &[]);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("dropped.bin")).expect("it landed"),
        bytes
    );
    let path = said.asked()[0]
        .field("path")
        .cloned()
        .expect("the form has the row");
    assert!(path.fresh, "the dropped path stands over a saved one");
}

#[test]
fn an_empty_file_goes_and_arrives_empty() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-empty");
    let here = case.root.join("empty.bin");
    std::fs::write(&here, b"").expect("written");

    let (_, outcome, failure) = case.run(
        Direction::Send,
        None,
        &[("path", Value::Text(here.display().to_string()))],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("empty.bin")).expect("it landed"),
        Vec::<u8>::new()
    );
}

#[test]
fn files_picked_from_the_listing_and_only_they_come_back() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-listing");
    for (name, length) in [("a.bin", 5_000), ("b.bin", 7_000), ("c.bin", 40_000)] {
        std::fs::write(case.device.join(name), pattern(length, 239)).expect("written");
    }

    let mut answers = receiving(&case.back, "shell", false, "sz -b");
    answers.push((
        "names",
        Value::Many(vec!["a.bin".to_string(), "c.bin".to_string()]),
    ));
    let (said, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["a.bin", "c.bin"]);
    for name in ["a.bin", "c.bin"] {
        assert_eq!(
            std::fs::read(case.back.join(name)).expect("it came back"),
            std::fs::read(case.device.join(name)).expect("it is there"),
            "{name} is whole"
        );
    }
    let listed = said
        .asked()
        .into_iter()
        .find(|form| form.id == "listed")
        .expect("the listing was offered");
    let label = &listed.field("names").expect("the list").label;
    assert_eq!(
        Path::new(label).canonicalize().ok(),
        case.device.canonicalize().ok(),
        "the list is headed by the directory of the device"
    );
}

#[test]
fn two_passes_learn_what_is_offered_and_take_what_was_picked() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-two-pass");
    for (name, length) in [("a.bin", 5_000), ("b.bin", 9_000), ("c.bin", 2_000)] {
        std::fs::write(case.device.join(name), pattern(length, 233)).expect("written");
    }

    let mut answers = receiving(&case.back, "offered", true, "sz -b *.bin");
    answers.push(("names", Value::Many(vec!["b.bin".to_string()])));
    let (said, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["b.bin"]);
    assert_eq!(
        std::fs::read(case.back.join("b.bin")).expect("it came back"),
        pattern(9_000, 233)
    );
    let offered = said
        .asked()
        .into_iter()
        .find(|form| form.id == "offered")
        .expect("the offered files were asked about");
    let ids: Vec<String> = offered
        .field("names")
        .expect("the list")
        .kind
        .options()
        .iter()
        .map(|choice| choice.id.clone())
        .collect();
    assert_eq!(
        ids,
        ["a.bin", "b.bin", "c.bin"],
        "everything the device offered"
    );
}

#[test]
fn two_passes_over_one_file_take_it_without_asking() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-two-pass-one");
    std::fs::write(case.device.join("only.bin"), pattern(6_000, 229)).expect("written");

    let answers = receiving(&case.back, "offered", true, "sz -b only.bin");
    let (said, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["only.bin"]);
    assert_eq!(forms(&said), ["receive"], "nothing else was asked");
}

#[test]
fn one_pass_takes_everything_the_command_offers() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-one-pass");
    for name in ["a.bin", "b.bin"] {
        std::fs::write(case.device.join(name), pattern(4_000, 227)).expect("written");
    }

    let answers = receiving(&case.back, "offered", false, "sz -b a.bin b.bin");
    let (said, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["a.bin", "b.bin"]);
    assert_eq!(forms(&said), ["receive"]);
    assert!(
        said.notices()
            .iter()
            .any(|notice| notice.starts_with("2 of 2")),
        "{:?}",
        said.notices()
    );
}

#[test]
fn a_file_already_here_is_kept_and_the_new_one_lands_beside_it() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-existing");
    std::fs::write(case.device.join("same.bin"), pattern(3_000, 211)).expect("written");
    std::fs::write(case.back.join("same.bin"), b"what was here").expect("written");

    let answers = receiving(&case.back, "offered", false, "sz -b same.bin");
    let (_, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.back.join("same.bin")).expect("it is there"),
        b"what was here"
    );
    assert_eq!(
        std::fs::read(case.back.join("same.bin.1")).expect("it landed beside"),
        pattern(3_000, 211)
    );
}

#[test]
fn a_listing_needs_the_command_to_be_run() {
    let case = Case::new("zmodem-listing-off");
    let mut answers = receiving(&case.back, "shell", false, "sz -b");
    answers.retain(|(name, _)| *name != "use_remote");
    answers.push(("use_remote", Value::Flag(false)));

    let (said, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert!(
        said.errors().iter().any(|error| error.contains("listing")),
        "{:?}",
        said.errors()
    );
}

#[test]
fn two_mebibytes_go_both_ways_whole() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-large");
    let here = case.root.join("large.bin");
    let bytes = pattern(2 * 1024 * 1024, 255);
    std::fs::write(&here, &bytes).expect("written");

    let (_, outcome, failure) = case.run(
        Direction::Send,
        None,
        &[("path", Value::Text(here.display().to_string()))],
    );
    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("large.bin")).expect("it landed"),
        bytes,
        "onto the device"
    );

    let answers = receiving(&case.back, "offered", false, "sz -b large.bin");
    let (_, outcome, failure) = case.run(Direction::Receive, None, &answers);
    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.back.join("large.bin")).expect("it came back"),
        bytes,
        "and off it"
    );
}

#[test]
fn a_name_with_a_space_is_picked_and_sent_whole() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-space");
    std::fs::write(case.device.join("two words.bin"), pattern(1_500, 7)).expect("written");

    let mut answers = receiving(&case.back, "shell", false, "sz -b");
    answers.push(("names", Value::Many(vec!["two words.bin".to_string()])));
    let (_, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["two words.bin"]);
}

#[test]
fn a_file_of_a_subdirectory_lands_under_its_own_name() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-subdirectory");
    std::fs::create_dir_all(case.device.join("deep/er")).expect("made");
    std::fs::write(case.device.join("deep/er/inner.bin"), pattern(2_500, 17)).expect("written");

    let answers = receiving(&case.back, "offered", false, "sz -b deep/er/inner.bin");
    let (_, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["inner.bin"]);
}

#[test]
fn a_sender_escaping_every_control_byte_is_read_the_same() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-escaped");
    let bytes = pattern(20_000, 256);
    std::fs::write(case.device.join("raw.bin"), &bytes).expect("written");

    let answers = receiving(&case.back, "offered", false, "sz -b -e raw.bin");
    let (_, outcome, failure) = case.run(Direction::Receive, None, &answers);

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.back.join("raw.bin")).expect("it came back"),
        bytes
    );
}

#[test]
fn a_transfer_stopped_midway_leaves_the_device_at_its_shell() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-cancel");
    let here = case.root.join("long.bin");
    std::fs::write(&here, pattern(8 * 1024 * 1024, 253)).expect("written");

    let line = console_shell_in(Some(&case.device));
    let said = Said::answering(&[("path", Value::Text(here.display().to_string()))]);
    let cancel = zyt_script::Cancel::new();
    let mut running = Run::shipped("zmodem")
        .start(line.clone(), said.clone(), cancel.clone())
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
    let outcome = fish::finish(&mut running, Duration::from_secs(30));
    assert_eq!(outcome, Outcome::Cancelled);

    std::thread::sleep(Duration::from_secs(1));
    let mut heard = Vec::new();
    let mut spare = Vec::new();
    line.read(&mut spare, Duration::from_millis(200));
    line.write(b"echo al''ive\r");
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
fn a_device_told_nothing_starts_its_receiver_from_the_line() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-announce");
    let here = case.root.join("told.bin");
    let bytes = pattern(4_000, 19);
    std::fs::write(&here, &bytes).expect("written");

    let (_, outcome, failure) = case.run(
        Direction::Send,
        None,
        &[
            ("path", Value::Text(here.display().to_string())),
            ("use_remote", Value::Flag(false)),
        ],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(
        std::fs::read(case.device.join("told.bin")).expect("it landed"),
        bytes,
        "the shell read rz off the line and started it"
    );
}

#[test]
fn a_receive_stopped_midway_leaves_the_device_at_its_shell() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-cancel-receive");
    std::fs::write(case.device.join("long.bin"), pattern(8 * 1024 * 1024, 251)).expect("written");

    let line = console_shell_in(Some(&case.device));
    let said = Said::answering(&receiving(&case.back, "offered", false, "sz -b long.bin"));
    let mut running = Run::shipped("zmodem")
        .receiving()
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
    let mut spare = Vec::new();
    line.read(&mut spare, Duration::from_millis(200));
    line.write(b"echo al''ive\r");
    let mut heard = Vec::new();
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

/// The answers of the receiving form for one file the device is told to send,
/// a file already here taken for its start.
fn resuming(into: &Path, name: &str) -> Vec<(&'static str, Value)> {
    let mut answers = receiving(into, "offered", false, &format!("sz -b {name}"));
    answers.push(("existing", Value::One("resume".to_string())));
    answers
}

/// Where the bytes of `there` stop being zero.
fn zeros_in(there: &[u8]) -> usize {
    there
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(there.len())
}

#[test]
fn a_shorter_file_here_is_the_start_and_only_the_rest_comes() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-resume-here");
    let bytes = pattern(70_000, 251);
    std::fs::write(case.device.join("blob.bin"), &bytes).expect("written");
    std::fs::write(case.back.join("blob.bin"), vec![0u8; 30_000]).expect("the start");

    let (_, outcome, failure) =
        case.run(Direction::Receive, None, &resuming(&case.back, "blob.bin"));

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["blob.bin"]);
    let there = std::fs::read(case.back.join("blob.bin")).expect("it is there");
    assert_eq!(there.len(), bytes.len());
    assert_eq!(
        zeros_in(&there),
        30_000,
        "what was here is kept, which is how the start is told from a fresh copy"
    );
    assert_eq!(there[30_000..], bytes[30_000..]);
}

#[test]
fn a_file_here_as_long_as_the_one_offered_is_not_fetched_again() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-resume-whole");
    std::fs::write(case.device.join("blob.bin"), pattern(5_000, 13)).expect("written");
    std::fs::write(case.back.join("blob.bin"), vec![0u8; 5_000]).expect("here");

    let (_, outcome, failure) =
        case.run(Direction::Receive, None, &resuming(&case.back, "blob.bin"));

    assert_eq!(outcome, Outcome::Done, "{failure}");
    assert_eq!(case.back_names(), ["blob.bin"]);
    assert_eq!(
        std::fs::read(case.back.join("blob.bin")).expect("it is there"),
        vec![0u8; 5_000]
    );
}

#[test]
fn a_receive_stopped_midway_is_finished_by_the_next_one() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-resume-stopped");
    let bytes = pattern(8 * 1024 * 1024, 251);
    std::fs::write(case.device.join("long.bin"), &bytes).expect("written");

    let line = console_shell_in(Some(&case.device));
    let said = Said::answering(&resuming(&case.back, "long.bin"));
    let mut running = Run::shipped("zmodem")
        .receiving()
        .start(line, said.clone(), zyt_script::Cancel::new())
        .expect("the script loads");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::fs::metadata(case.back.join("long.bin")).map_or(0, |here| here.len()) < 100_000 {
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

    let start = std::fs::read(case.back.join("long.bin")).expect("the start is kept");
    assert!(start.len() < bytes.len(), "it was stopped before the end");
    assert_eq!(start[..], bytes[..start.len()], "what is kept is what came");
    // A mark in what is kept: a fresh copy would write over it, a resumed one
    // does not touch it.
    let mut marked = start;
    marked[0] ^= 0xff;
    std::fs::write(case.back.join("long.bin"), &marked).expect("marked");

    let (_, outcome, failure) =
        case.run(Direction::Receive, None, &resuming(&case.back, "long.bin"));

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let there = std::fs::read(case.back.join("long.bin")).expect("it is there");
    assert_eq!(there.len(), bytes.len());
    assert_eq!(there[0], bytes[0] ^ 0xff, "the start was not fetched again");
    assert_eq!(there[1..], bytes[1..]);
}

#[test]
fn a_device_resuming_is_sent_only_the_rest() {
    if !has_lrzsz() {
        return;
    }
    let case = Case::new("zmodem-resume-send");
    let here = case.root.join("blob.bin");
    let bytes = pattern(70_000, 251);
    std::fs::write(&here, &bytes).expect("written");
    std::fs::write(case.device.join("blob.bin"), vec![0u8; 30_000]).expect("the start");

    let (_, outcome, failure) = case.run(
        Direction::Send,
        None,
        &[
            ("path", Value::Text(here.display().to_string())),
            ("remote", Value::Text("rz -r".to_string())),
        ],
    );

    assert_eq!(outcome, Outcome::Done, "{failure}");
    let there = std::fs::read(case.device.join("blob.bin")).expect("it landed");
    assert_eq!(there.len(), bytes.len());
    // `rz` asks from the kilobyte below what it has.
    let kept = zeros_in(&there);
    assert_eq!(kept, 30_000 / 1024 * 1024, "the start there was kept");
    assert_eq!(there[kept..], bytes[kept..]);
}
