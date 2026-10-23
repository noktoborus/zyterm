//! `zyt.wait`: one wait for the line, a socket and a timer.

mod common;

use common::{Said, start};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_script::{Cancel, Direction, LineChannel, Outcome, ScriptRun};

/// Waits until the script printed `mark`, and says how long that took.
fn until_said(said: &Said, mark: &str) -> Duration {
    let started = Instant::now();
    while !said.echo().contains(mark) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the script never said {mark}: {}",
            said.echo()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    started.elapsed()
}

#[test]
fn one_wait_takes_a_timer_the_line_a_client_and_room_to_write() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("waits", line.clone(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");

    until_said(&said, "[timer]");
    let address: std::net::SocketAddr = said
        .echo()
        .split("at ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("it said where")
        .parse()
        .expect("an address");

    std::thread::sleep(Duration::from_millis(100));
    line.feed(b"x");
    assert!(
        until_said(&said, "[line]") < Duration::from_secs(1),
        "the line woke the wait, long before its time"
    );

    let _client = std::net::TcpStream::connect(address).expect("connect");
    assert!(until_said(&said, "[client]") < Duration::from_secs(1));
    until_said(&said, "[writable]");
    until_said(&said, "[long]");

    let started = Instant::now();
    run.cancel();
    while !run.tick() {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the Cancel let it go"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(run.outcome(), Some(Outcome::Cancelled));
    assert!(!said.echo().contains("[no "), "{}", said.echo());
}
