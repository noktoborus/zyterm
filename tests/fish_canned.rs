//! The wire of the shell transfer, against replies written down beforehand.
//!
//! What the device says is a string this test wrote, and what the script sent
//! is read back off the line. Nothing runs a shell: these are the teeth of the
//! protocol — the framing, the echo of a console, the offsets of a chunk and
//! the refusals — and each of them is one reply away from being wrong.

mod fish;

use fish::{Run, against};
use zyt_script::Outcome;

/// A case of the conversation, run against those replies.
fn case(name: &str, replies: &str, told: &[(&str, &str)]) -> (String, String, Outcome) {
    let mut run = Run::fixture("fish-case").told("case", name);
    for (what, value) in told {
        run = run.told(what, value);
    }
    let (written, said, outcome) = against(replies, &run, &[]);
    (written, said.echo(), outcome)
}

#[test]
fn a_reply_glued_to_a_prompt_is_still_read() {
    let (_, echo, outcome) = case(
        "pwd",
        "/root # ### 100\r\n/root\r\n### 200\r\n",
        &[("path", "/")],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "/root");
}

#[test]
fn the_echo_of_a_console_is_skipped_as_the_noise_it_is() {
    let replies = concat!(
        "\\echo '##''# 100'; \\pwd; \\echo '##''# 200'\r\n",
        "### 100\r\n",
        "/home/board\r\n",
        "### 200\r\n"
    );
    let (_, echo, outcome) = case("pwd", replies, &[]);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "/home/board");
}

#[test]
fn nothing_the_script_sends_can_be_read_back_as_a_reply() {
    let (written, _, _) = case("pwd", "### 100\r\n/\r\n### 200\r\n", &[]);
    assert!(
        !written.contains("### "),
        "a console would echo it back as an answer: {written}"
    );
    assert!(written.contains("'##''# 100'"), "{written}");
}

#[test]
fn every_command_word_is_quoted_against_an_alias() {
    let (written, _, _) = case("pwd", "### 100\r\n/\r\n### 200\r\n", &[]);
    for word in written.split_whitespace() {
        if word.ends_with("echo") || word.ends_with("pwd") {
            assert!(word.starts_with('\\'), "{word} in {written}");
        }
    }
}

#[test]
fn a_probe_that_names_a_missing_program_ends_the_conversation() {
    let replies = "### 100\r\nHbase64\r\nEtail\r\nHhead\r\nHwc\r\n### 200\r\n";
    let (_, _, outcome) = case("probe", replies, &[]);
    assert_eq!(outcome, Outcome::Failed(None));
}

#[test]
fn raw_over_a_pipe_is_refused_by_the_name_of_the_program() {
    let replies = "### 100\r\nHtail\r\nHhead\r\nHwc\r\nEstty\r\n### 200\r\n";
    let mut run = Run::fixture("fish-case")
        .told("case", "probe")
        .told("mode", "raw");
    run = run.told("mode", "raw");
    let (_, said, outcome) = against(replies, &run, &[]);
    assert_eq!(outcome, Outcome::Failed(None));
    assert!(said.echo().is_empty(), "nothing was carried");
}

#[test]
fn the_size_rides_on_the_reply_line_and_is_asked_for_once() {
    let (written, echo, outcome) = case(
        "size",
        "### 100 4096\r\n### 200\r\n",
        &[("path", "/tmp/one")],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "4096");
    assert_eq!(written.matches("\\wc -c").count(), 1, "{written}");
}

#[test]
fn a_size_nothing_can_say_stops_the_transfer() {
    let (_, _, outcome) = case("size", "### 100\r\n### 200\r\n", &[("path", "/tmp/one")]);
    assert_eq!(outcome, Outcome::Failed(None));
}

#[test]
fn a_refusal_is_a_refusal_whatever_was_asked() {
    let (_, _, outcome) = case("size", "### 500\r\n", &[("path", "/tmp/one")]);
    assert_eq!(outcome, Outcome::Failed(None));
}

#[test]
fn what_a_path_is_comes_back_as_one_letter() {
    for (letter, named) in [("D", "directory"), ("F", "file"), ("O", "other")] {
        let (_, echo, outcome) = case(
            "kind",
            &format!("### 100\r\n{letter}\r\n### 200\r\n"),
            &[("path", "/tmp/one")],
        );
        assert_eq!(outcome, Outcome::Done);
        assert_eq!(echo.trim(), named);
    }
}

#[test]
fn a_listing_is_three_lines_an_entry_and_the_name_closes_it() {
    let replies = concat!(
        "### 100\r\n",
        "Pd\r\nS0\r\n:under\r\n",
        "P-\r\nS42\r\n:one.txt\r\n",
        "P-\r\nS7\r\n:two words.txt\r\n",
        "### 200\r\n"
    );
    let (_, echo, outcome) = case("list", replies, &[("path", "/tmp")]);
    assert_eq!(outcome, Outcome::Done);

    let said: Vec<&str> = echo.lines().collect();
    assert_eq!(
        said,
        [
            "directory 0 under",
            "file 42 one.txt",
            "file 7 two words.txt"
        ]
    );
}

#[test]
fn a_listing_of_nothing_is_no_error() {
    let (_, echo, outcome) = case("list", "### 100\r\n### 200\r\n", &[("path", "/tmp")]);
    assert_eq!(outcome, Outcome::Done);
    assert!(echo.trim().is_empty());
}

#[test]
fn a_sum_is_the_first_field_of_what_the_program_wrote() {
    let (written, echo, outcome) = case(
        "digest",
        "### 100\r\nD9dd4e461268c8034f5c8564e155c67a6\r\n### 200\r\n",
        &[("path", "/tmp/one"), ("program", "md5sum")],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "9dd4e461268c8034f5c8564e155c67a6");
    assert!(written.contains("\\md5sum"), "{written}");
}

#[test]
fn a_sum_the_device_will_not_say_is_refused() {
    let (_, _, outcome) = case(
        "digest",
        "### 100\r\n### 200\r\n",
        &[("path", "/tmp/one"), ("program", "md5sum")],
    );
    assert_eq!(outcome, Outcome::Failed(None));
}

#[test]
fn a_sum_the_device_has_not_got_is_no_error_in_itself() {
    let (_, echo, outcome) = case(
        "offers",
        "### 100\r\nEmd5sum\r\n### 200\r\n",
        &[("program", "md5sum")],
    );
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "false");
}

/// One whole line of base64: 57 bytes of a file, 76 characters of the wire.
const LINE: &str = "eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4eHh4";

#[test]
fn every_chunk_of_a_read_says_where_it_starts_counting_from_one() {
    let into = std::env::temp_dir().join(format!("fish-read-{}", std::process::id()));
    let chunk = format!("### 100\r\n{LINE}\r\n### 200\r\n");
    let replies = format!("### 100 171\r\n### 200\r\n{chunk}{chunk}{chunk}");

    let (written, echo, outcome) = case(
        "retrieve",
        &replies,
        &[
            ("path", "/tmp/one"),
            ("into", into.to_str().expect("a path of letters")),
            ("chunk", "77"),
        ],
    );

    assert_eq!(outcome, Outcome::Done);
    assert_eq!(echo.trim(), "171", "every chunk carried its whole line");
    assert!(written.contains("\\tail -c +1 "), "{written}");
    assert!(written.contains("\\tail -c +58 "), "{written}");
    assert!(written.contains("\\tail -c +115 "), "{written}");
    assert_eq!(
        std::fs::read(&into).expect("the file was written").len(),
        171
    );
    let _ = std::fs::remove_file(&into);
}

#[test]
fn a_chunk_that_carries_nothing_says_where_the_file_stopped() {
    let into = std::env::temp_dir().join(format!("fish-short-{}", std::process::id()));
    let replies = String::from("### 100 171\r\n### 200\r\n### 100\r\n### 200\r\n");

    let (_, _, outcome) = case(
        "retrieve",
        &replies,
        &[
            ("path", "/tmp/one"),
            ("into", into.to_str().expect("a path of letters")),
            ("chunk", "77"),
        ],
    );
    assert_eq!(outcome, Outcome::Failed(None));
    let _ = std::fs::remove_file(&into);
}
