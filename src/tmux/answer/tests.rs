use std::time::{Duration, Instant};

use super::super::tests::{opened, replies, said, showing};
use super::super::{Choice, Control};

/// The mode shown, a `send-keys` sent at `start` and not answered.
fn waiting(start: Instant) -> Control {
    let mut control = showing();
    said(&mut control);
    control.tick(start, true);
    control.input(b"x");
    assert!(said(&mut control).starts_with("send-keys"));
    control
}

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn a_command_unanswered_past_the_time_holds_what_follows_until_tmux_speaks() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.tick(start + 9 * SECOND, true);
    assert_eq!(control.unanswered(), None, "not before the time");

    control.tick(start + 11 * SECOND, true);
    assert_eq!(control.unanswered(), Some(11 * SECOND));
    control.resize(100, 30);
    assert_eq!(said(&mut control), "", "the size is held");

    control.feed(&mut replies(100, &[""]));
    assert_eq!(control.unanswered(), None);
    assert!(
        said(&mut control).contains("refresh-client -C 100x30"),
        "what was held goes once tmux speaks"
    );
}

#[test]
fn anything_tmux_says_restarts_the_clock() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.tick(start + 8 * SECOND, true);
    control.feed(&mut b"%output %0 more\r\n".to_vec());
    control.tick(start + 15 * SECOND, true);
    assert_eq!(control.unanswered(), None, "heard at 8, so 7 s of silence");
    control.tick(start + 19 * SECOND, true);
    assert!(control.unanswered().is_some());
}

#[test]
fn the_clock_stands_while_the_line_is_not_read() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.tick(start + 30 * SECOND, false);
    assert_eq!(control.unanswered(), None, "a selection held the line");
    control.tick(start + 35 * SECOND, true);
    assert_eq!(control.unanswered(), None, "the clock begins again at 30");
    control.tick(start + 40 * SECOND, true);
    assert!(control.unanswered().is_some());
}

#[test]
fn the_time_is_the_one_set() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.set_unanswered_after(3 * SECOND);
    control.tick(start + 3 * SECOND, true);
    assert_eq!(control.unanswered(), Some(3 * SECOND));
}

#[test]
fn nothing_awaited_is_no_silence() {
    let start = Instant::now();
    let mut control = showing();
    said(&mut control);
    control.tick(start, true);
    control.tick(start + 60 * SECOND, true);
    assert_eq!(control.unanswered(), None);
}

#[test]
fn a_choice_made_while_tmux_is_silent_is_not_kept_for_later() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.tick(start + 11 * SECOND, true);
    control.choose(Choice::NewWindow);
    control.feed(&mut replies(100, &[""]));
    assert!(!said(&mut control).contains("new-window"));
}

#[test]
fn let_go_of_while_silent_the_line_gets_detach_alone() {
    let start = Instant::now();
    let mut control = waiting(start);
    control.tick(start + 11 * SECOND, true);
    control.input(b"typed");
    control.resize(100, 30);
    assert_eq!(control.abandon(), b"detach\n");
    assert_eq!(control.keys_held(), 0, "what was typed is thrown away");
    assert!(control.take_out().is_empty(), "and what was held");
}

#[test]
fn a_line_not_of_tmux_on_a_console_is_given_back_from_where_it_starts() {
    let mut control = showing();
    control.feed(&mut b"%output %0 x\r\nConnection to box closed.\r\n$ ".to_vec());
    assert_eq!(
        control.take_gone().as_deref(),
        Some(&b"Connection to box closed.\r\n$ "[..])
    );
}

#[test]
fn a_line_not_of_tmux_on_a_port_is_a_lost_byte() {
    let mut control = opened(true);
    control.feed(&mut b"Connection to box closed.\r\n".to_vec());
    assert_eq!(control.take_gone(), None);
}
