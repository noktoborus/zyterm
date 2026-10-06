//! The runner, driven as somebody writing a script drives it.

use std::path::PathBuf;
use std::process::Command;

/// The runner, pointed at the fixture scripts and nowhere else.
fn runner() -> Command {
    let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("scripts");
    let mut command = Command::new(env!("CARGO_BIN_EXE_zyt-script"));
    command.arg("--path").arg(scripts).arg("--no-default-paths");
    command
}

#[test]
fn the_listing_says_what_was_found_and_what_was_left_out() {
    let said = runner().arg("list").output().expect("it runs");
    let out = String::from_utf8_lossy(&said.stdout).to_string();

    assert!(said.status.success());
    assert!(out.contains("echo"), "{out}");
    assert!(out.contains("on the line"), "{out}");
    assert!(
        out.contains("broken") && out.contains("left out"),
        "a directory that is not a script is named and the rest still listed: {out}"
    );
}

#[test]
fn one_script_says_what_it_needs_before_it_runs() {
    let said = runner()
        .args(["show", "carries"])
        .output()
        .expect("it runs");
    let out = String::from_utf8_lossy(&said.stdout).to_string();

    assert!(said.status.success());
    assert!(out.contains("asks for   remote_host"), "{out}");
    assert!(out.contains("send       takes file"), "{out}");
}

#[test]
fn a_name_nothing_carries_fails_with_a_word_about_it() {
    let said = runner()
        .args(["show", "nothing-of-the-sort"])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(!said.status.success());
    assert!(err.contains("no script named nothing-of-the-sort"), "{err}");
}

#[test]
fn checking_every_script_fails_on_the_one_that_is_broken() {
    let said = runner().args(["check", "--all"]).output().expect("it runs");
    let out = String::from_utf8_lossy(&said.stdout).to_string();
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(out.contains("echo"), "{out}");
    assert!(!said.status.success(), "the broken one is reported");
    assert!(err.contains("broken"), "{err}");
}

#[test]
fn a_script_runs_against_a_program_with_no_window_anywhere() {
    let said = runner()
        .args(["run", "pipes", "--line", "null"])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(said.status.success(), "{err}");
    assert!(err.contains("got hello"), "{err}");
    assert!(err.contains("code 0"), "{err}");
}

#[test]
fn a_console_echoes_what_the_script_typed_and_the_script_reads_it() {
    let said = runner()
        .args(["run", "echo", "--line", "pty", "--command", "sh"])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(said.status.success(), "{err}");
    assert!(err.contains("said pwd"), "{err}");
}

#[test]
fn a_dialog_is_answered_from_the_command_line() {
    let said = runner()
        .args([
            "run",
            "asks",
            "--line",
            "null",
            "--answer",
            "host=board",
            "--answer",
            "mode=raw",
            "--answer",
            "verify=false",
        ])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(said.status.success(), "{err}");
    assert!(err.contains("asked: Ask"), "{err}");
}

#[test]
fn a_run_asked_to_stop_ends_as_cancelled() {
    let said = runner()
        .args(["run", "spin", "--line", "null", "--cancel-after", "200"])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(!said.status.success());
    assert!(err.contains("cancelled"), "{err}");
}

#[test]
fn a_run_that_asks_for_a_value_the_source_has_not_got_says_which() {
    let said = runner()
        .args(["run", "carries", "--line", "null", "--target", "/tmp/x.itb"])
        .output()
        .expect("it runs");
    let err = String::from_utf8_lossy(&said.stderr).to_string();

    assert!(!said.status.success());
    assert!(err.contains("remote_host"), "{err}");
}
