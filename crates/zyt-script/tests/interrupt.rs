//! Stopping a script, in each of the ways one can refuse to stop.

mod common;

use common::{Said, scripts, start};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_script::{Cancel, Direction, Line, LineChannel, Outcome, ScriptError, ScriptRun};

/// Walks the ladder of stopping until the run is over, or says it never was.
fn stop_and_wait(run: &mut ScriptRun, how_long: Duration) -> Outcome {
    let deadline = Instant::now() + how_long;
    run.cancel();
    while !run.tick() {
        if Instant::now() >= deadline {
            panic!("the script was never stopped");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    run.outcome().expect("it says how it ended")
}

/// True while that process is still running, as `/proc` says.
#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(after_name) = stat.rsplit_once(')') else {
        return false;
    };
    !matches!(after_name.1.split_whitespace().next(), Some("Z") | None)
}

#[test]
fn a_loop_that_asks_the_host_nothing_is_stopped_by_the_hook() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("spin", LineChannel::new(), said, Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    std::thread::sleep(Duration::from_millis(50));

    let started = Instant::now();
    assert_eq!(
        stop_and_wait(&mut run, Duration::from_secs(10)),
        Outcome::Cancelled
    );
    assert!(
        started.elapsed() < zyt_script::ABANDON_AFTER,
        "the hook caught it before it had to be abandoned"
    );
}

#[test]
fn a_script_waiting_on_a_silent_device_is_let_go_at_once() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("silent", line, said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    std::thread::sleep(Duration::from_millis(50));

    let started = Instant::now();
    assert_eq!(
        stop_and_wait(&mut run, Duration::from_secs(10)),
        Outcome::Cancelled
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "it was waiting for a device, not for the hook"
    );
    assert!(
        !said.echo().contains("never"),
        "the script did not carry on"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn stopping_a_script_stops_the_program_it_started() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("child", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");

    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Some(said) = said.echo().strip_prefix("pid ")
            && let Ok(pid) = said.trim().parse::<u32>()
        {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "the program never said its number"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(alive(pid), "the program of the script is running");

    assert_eq!(
        stop_and_wait(&mut run, Duration::from_secs(10)),
        Outcome::Cancelled
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        assert!(
            Instant::now() < deadline,
            "the program the script started is still running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_script_that_was_dropped_takes_its_program_with_it() {
    let said = Arc::new(Said::default());
    let run = ScriptRun::start(
        start("child", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");

    let deadline = Instant::now() + Duration::from_secs(5);
    while !said.echo().starts_with("pid ") {
        assert!(Instant::now() < deadline, "the program never started");
        std::thread::sleep(Duration::from_millis(10));
    }

    #[cfg(target_os = "linux")]
    let pid: u32 = said
        .echo()
        .strip_prefix("pid ")
        .expect("it said a number")
        .trim()
        .parse()
        .expect("the number is a number");

    drop(run);

    #[cfg(target_os = "linux")]
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(pid) {
            assert!(Instant::now() < deadline, "the program outlived the run");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn a_script_cannot_reach_a_process_this_crate_never_heard_of() {
    let directory = scripts().join("sneaks");
    std::fs::create_dir_all(&directory).expect("the directory is made");
    std::fs::write(
        directory.join("Manifest.yaml"),
        "name: Reaches for what it may not\nsend:\n  target: none\n",
    )
    .expect("the manifest is written");
    std::fs::write(
        directory.join("init.lua"),
        "return {\n    send = function()\n        zyt.term.print(tostring(os.execute) .. \" \" .. tostring(io.popen) .. \" \" .. tostring(os.exit) .. \" \" .. tostring(debug))\n    end,\n}\n",
    )
    .expect("the script is written");

    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("sneaks", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    let outcome = common::run_to_end(&mut run, Duration::from_secs(5));
    std::fs::remove_dir_all(&directory).expect("it is taken away");

    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        said.echo().trim(),
        "nil nil nil nil",
        "nothing that starts a process or ends the program is reachable"
    );
}

#[test]
fn a_run_given_up_on_reports_that_it_was_abandoned() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let cancel = Cancel::new();
    let mut run = ScriptRun::start(start("silent", line.clone(), said, cancel), Direction::Send)
        .expect("the script loads");

    run.abandon();

    assert!(run.is_finished());
    assert_eq!(run.outcome(), Some(Outcome::Cancelled));
    assert!(matches!(
        run.take_failure(),
        Some(ScriptError::Abandoned { .. })
    ));
    assert!(!line.is_open(), "the line it was writing to is shut");
}
