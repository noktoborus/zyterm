//! What a script started, and that stopping it stops all of it.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::{Duration, Instant};
use zyt_script::{ProcessRegistry, shell_command};

/// True once the check answers true, or false when it never does.
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

/// True while that process is still running.
///
/// A process killed while its parent is gone too is reparented, and whoever
/// it lands on may not be reaping anything, so its entry stays behind as a
/// zombie. The state of it is therefore what is asked, not whether the entry
/// is there.
fn alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(after_name) = stat.rsplit_once(')') else {
        return false;
    };
    !matches!(after_name.1.split_whitespace().next(), Some("Z") | None)
}

/// A shell that starts a long program of its own and says its number.
fn shell_with_a_child() -> (std::process::Child, u32) {
    let mut command = shell_command("sleep 60 & echo $!; wait");
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().expect("the shell starts");

    let out = child.stdout.take().expect("the shell has a pipe");
    let mut line = String::new();
    BufReader::new(out)
        .read_line(&mut line)
        .expect("the shell says the number");
    let pid: u32 = line.trim().parse().expect("the number is a number");
    (child, pid)
}

#[test]
fn stopping_a_script_stops_what_its_shell_started() {
    let mut registry = ProcessRegistry::new().expect("the group is made");
    let (child, grandchild) = shell_with_a_child();
    let (_, handle) = registry.adopt(child).expect("the child is taken in");
    assert_eq!(registry.len(), 1);
    assert!(alive(grandchild), "the program of the shell is running");

    registry.kill_all();

    assert!(registry.is_empty(), "nothing is left on the list");
    assert!(
        wait_for(|| !alive(grandchild)),
        "the program the shell started is gone with it"
    );
    assert!(
        handle.lock().is_ok(),
        "the handle is still the caller's to hold"
    );
}

#[test]
fn a_process_the_list_lost_is_still_stopped_when_the_list_is_dropped() {
    let (child, grandchild) = shell_with_a_child();
    {
        let mut registry = ProcessRegistry::new().expect("the group is made");
        registry.adopt(child).expect("the child is taken in");
    }
    assert!(
        wait_for(|| !alive(grandchild)),
        "dropping the list stops everything on it"
    );
}

#[test]
fn one_of_several_is_stopped_alone() {
    let mut registry = ProcessRegistry::new().expect("the group is made");
    let (first, first_child) = shell_with_a_child();
    let (second, second_child) = shell_with_a_child();
    let (one, _) = registry.adopt(first).expect("the first is taken in");
    registry.adopt(second).expect("the second is taken in");

    registry.kill(one).expect("the first stops");

    assert!(wait_for(|| !alive(first_child)), "the first one is gone");
    assert_eq!(registry.len(), 1, "the other entry stands");
    assert!(alive(second_child), "the other program runs on");

    registry.kill_all();
    assert!(wait_for(|| !alive(second_child)));
}

#[test]
fn a_program_that_ended_by_itself_is_named_once_and_taken_off() {
    let mut registry = ProcessRegistry::new().expect("the group is made");
    let child = shell_command("true").spawn().expect("it starts");
    let (id, _) = registry.adopt(child).expect("it is taken in");

    assert!(
        wait_for(|| registry.reap().contains(&id)),
        "the one that ended is named"
    );
    assert!(registry.is_empty(), "and taken off the list");
    assert!(registry.reap().is_empty(), "and named only once");
}
