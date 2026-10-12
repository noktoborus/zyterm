//! A script that hands the line to a program of this machine.
//!
//! `modem.carry` is what a transfer by a program beside the line is: the
//! program is started, handed the line and left to it. The scripts shipped
//! speak their protocols themselves now, so what is left to hold to a promise
//! is the failure: a script that cannot start the program it needs says which
//! program that was, rather than hanging on a line.

mod fish;

use fish::{Run, console_shell_in, over, workspace};
use std::time::Duration;
use zyt_script::{Outcome, TargetKind};

/// How long the failure is given to come.
const PATIENCE: Duration = Duration::from_secs(120);

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
