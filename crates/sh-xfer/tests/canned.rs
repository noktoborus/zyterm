//! The conversation against a device whose answers are written out in advance.
//!
//! What matters here is not what comes back but what goes out: which commands
//! the client sends, where in the file each chunk says it belongs — where it
//! starts when it is read, and how long the file is once it has landed when it
//! is written — and above all when it touches the settings of the line.

use sh_xfer::{Digest, Mode, Reader, Session, ShXferError};
use std::io::Cursor;
use std::time::Duration;

/// Every command the probe of a mode tries, in the order it tries them.
fn tools(mode: Mode) -> &'static [&'static str] {
    match mode {
        Mode::Base64 => &["base64", "tail", "head", "wc"],
        Mode::Raw => &["tail", "head", "wc", "stty"],
    }
}

/// A device sending these scripts that says exactly this and nothing more.
fn device(mode: Mode, answer: &str) -> Session<Vec<u8>> {
    let reader = Reader::new(Cursor::new(answer.to_string()), Duration::from_secs(2));
    Session::new(reader, Vec::new(), mode)
}

/// The answer of a device that has these commands and none of the others.
fn probe(mode: Mode, has: &[&str]) -> String {
    let mut text = String::from("### 100\n");
    for tool in tools(mode) {
        text.push(if has.contains(tool) { 'H' } else { 'E' });
        text.push_str(tool);
        text.push('\n');
    }
    text.push_str("### 200\n");
    text
}

/// The answer of a device that has everything the mode needs.
fn ready(mode: Mode) -> String {
    probe(mode, tools(mode))
}

/// The answer to the probe of one sum.
fn offers(digest: Digest, has: bool) -> String {
    let mark = if has { 'H' } else { 'E' };
    format!("### 100\n{mark}{}\n### 200\n", digest.program())
}

/// The answer to a size, as the reply line carries it.
fn size(bytes: usize) -> String {
    format!("### 100 {bytes}\n### 200\n")
}

fn base64_of(body: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(body)
}

/// What the session sent, as text.
fn sent(session: Session<Vec<u8>>) -> String {
    String::from_utf8(session.into_writer()).expect("the commands are text")
}

#[test]
fn a_line_that_carries_base64_is_never_switched_to_binary() {
    let answer = format!(
        "{}### 100\n/root\n### 200\n{}### 100\n{}\n### 200\n",
        ready(Mode::Base64),
        size(3),
        base64_of(b"abc")
    );
    let mut session = device(Mode::Base64, &answer);

    session.hello().expect("the device answers");
    assert_eq!(session.pwd().expect("a path"), "/root");
    let mut out = Vec::new();
    session
        .retrieve("/tmp/one.bin", &mut out)
        .expect("the file comes back");
    assert_eq!(out, b"abc");

    let sent = sent(session);
    assert!(
        !sent.contains("stty"),
        "base64 touched the settings of the line:\n{sent}"
    );
}

#[test]
fn a_line_that_carries_raw_is_switched_only_while_it_does() {
    let answer = format!(
        "{}### 100\n/root\n### 200\n{}### 100\nabc\n\n### 200\n",
        ready(Mode::Raw),
        size(3)
    );
    let mut session = device(Mode::Raw, &answer);

    session.hello().expect("the device answers");
    session.pwd().expect("a path");
    let mut out = Vec::new();
    session
        .retrieve("/tmp/one.bin", &mut out)
        .expect("the file comes back");
    assert_eq!(out, b"abc");

    let sent = sent(session);
    let switching: Vec<&str> = sent
        .lines()
        .filter(|line| line.contains("stty raw"))
        .collect();

    assert_eq!(
        switching.len(),
        1,
        "the line is switched once, by the command carrying the chunk:\n{sent}"
    );
    assert!(
        switching[0].contains("stty -raw echo"),
        "the command that switched the line does not put it back:\n{}",
        switching[0]
    );
    assert!(
        !sent
            .lines()
            .next()
            .expect("a first line")
            .contains("stty raw"),
        "the handshake switched the line:\n{sent}"
    );
}

#[test]
fn nothing_a_command_sets_on_the_line_outlives_it() {
    let answer = format!("{}### 000\n### 001\n### 200\n", ready(Mode::Raw));
    let mut session = device(Mode::Raw, &answer);
    session.hello().expect("the device answers");
    session.set_size_check(false);
    session
        .store("/tmp/one.bin", &mut b"abc".as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    let switching: Vec<&str> = sent
        .lines()
        .filter(|line| line.contains("stty raw"))
        .collect();
    assert!(
        switching.iter().all(|line| !line.contains("stty -g")),
        "a command kept the settings of the line in a variable of the device:\n{sent}"
    );
    assert!(
        sent.contains("stty raw -echo") && sent.contains("stty -raw echo"),
        "the line is switched and put back by the same command:\n{sent}"
    );
}

#[test]
fn a_chunk_is_cut_to_whole_lines_of_base64() {
    let mut session = device(Mode::Base64, &ready(Mode::Base64));
    session.hello().expect("the device answers");

    for chunk in [1, 20, 77, 153] {
        session.set_chunk_size(chunk);
        assert_eq!(
            session.slice() % 57,
            0,
            "a chunk of {chunk} bytes cut a line of base64 in half"
        );
    }
    session.set_chunk_size(1);
    assert_eq!(session.slice(), 57, "a chunk is never less than one line");
    session.set_chunk_size(2048);
    assert_eq!(session.slice(), 26 * 57);
}

#[test]
fn a_chunk_of_raw_is_the_chunk_itself() {
    let mut session = device(Mode::Raw, &ready(Mode::Raw));
    session.hello().expect("the device answers");
    session.set_chunk_size(2048);
    assert_eq!(session.slice(), 2048);
}

#[test]
fn every_chunk_of_a_file_being_read_says_where_it_starts() {
    let body: Vec<u8> = (0..150_u8).collect();
    let answer = format!(
        "{}{}### 100\n{}\n### 200\n### 100\n{}\n### 200\n### 100\n{}\n### 200\n",
        ready(Mode::Base64),
        size(body.len()),
        base64_of(&body[..57]),
        base64_of(&body[57..114]),
        base64_of(&body[114..])
    );
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_chunk_size(77);

    let mut out = Vec::new();
    let got = session
        .retrieve("/tmp/one.bin", &mut out)
        .expect("the file comes back");

    assert_eq!(got, 150);
    assert_eq!(out, body);
    let sent = sent(session);
    for chunk in ["+1 ", "+58 ", "+115 "] {
        assert!(
            sent.contains(&format!("tail -c {chunk}")),
            "no command read from {chunk}:\n{sent}"
        );
    }
}

#[test]
fn a_device_without_what_a_mode_needs_is_refused_by_name() {
    for (mode, has, expected) in [
        (Mode::Base64, vec!["tail", "head", "wc"], "base64"),
        (Mode::Base64, vec!["base64", "head", "wc"], "tail"),
        (Mode::Raw, vec!["tail", "head", "wc"], "stty"),
        (Mode::Raw, vec![], "tail"),
    ] {
        let mut session = device(mode, &probe(mode, &has));

        let outcome = session.hello();

        let Err(ShXferError::MissingCommand { command }) = outcome else {
            panic!("a device missing {expected} is refused by name, got {outcome:?}");
        };
        assert_eq!(command, expected, "for {mode:?} with {has:?}");
    }
}

#[test]
fn a_device_that_cannot_place_a_chunk_is_refused_before_anything_travels() {
    let mut session = device(Mode::Base64, &probe(Mode::Base64, &["base64"]));

    assert!(session.hello().is_err(), "a device without dd is refused");

    let sent = sent(session);
    assert_eq!(
        sent.lines().count(),
        1,
        "nothing was sent after the probe:\n{sent}"
    );
}

#[test]
fn nothing_the_client_sends_can_be_read_back_as_a_reply() {
    let answer = format!(
        "{}### 100\n/root\n### 200\n\
         ### 100\nP-\nS3\n:one.bin\n### 200\n\
         ### 000\n{}### 100\n{}\n### 200\n",
        ready(Mode::Base64),
        size(3),
        base64_of(b"abc")
    );
    let mut session = device(Mode::Base64, &answer);

    session.hello().expect("the device answers");
    session.pwd().expect("a path");
    session.list("/tmp").expect("a listing");
    session.make_directory("/tmp/deep").expect("a directory");
    let mut out = Vec::new();
    session
        .retrieve("/tmp/one.bin", &mut out)
        .expect("the file comes back");

    let sent = sent(session);
    assert!(
        !sent.contains("###"),
        "a script carried the marker of a reply, which its own echo would forge:\n{sent}"
    );
}

#[test]
fn no_command_can_be_caught_by_an_alias() {
    let answer = format!(
        "{}### 100\n/root\n### 200\n\
         ### 100\nPd\nS0\n:logs\n### 200\n\
         ### 000\n### 100\nDabc\n### 200\n{}### 100\n{}\n### 200\n",
        ready(Mode::Base64),
        size(3),
        base64_of(b"abc")
    );
    let mut session = device(Mode::Base64, &answer);

    session.hello().expect("the device answers");
    session.pwd().expect("a path");
    session.list("/tmp").expect("a listing");
    session.make_directory("/tmp/deep").expect("a directory");
    session.digest(Digest::Sha1, "/tmp/one.bin").expect("a sum");
    let mut out = Vec::new();
    session
        .retrieve("/tmp/one.bin", &mut out)
        .expect("the file comes back");

    let sent = sent(session);
    for word in [
        "ls", "wc", "dd", "base64", "head", "stty", "sha1sum", "md5sum", "echo", "pwd", "cd",
        "read", "mkdir",
    ] {
        for line in sent.lines() {
            let mut rest = line;
            while let Some(at) = rest.find(word) {
                let before = &rest[..at];
                assert!(
                    before.ends_with('\\')
                        || before
                            .chars()
                            .last()
                            .is_some_and(|last| last.is_alphanumeric() || last == '-'),
                    "{word} stands without a backslash and an alias could catch it:\n{line}"
                );
                rest = &rest[at + word.len()..];
            }
        }
    }
}

#[test]
fn a_shell_prompt_does_not_hide_the_answer() {
    let answer = "/root # ### 100\nHbase64\nHdd\n/root # ### 200\n";
    let mut session = device(Mode::Base64, answer);

    session.hello().expect("the device answers");
}

#[test]
fn a_shell_prompt_does_not_hide_a_size() {
    let answer = format!("{}/root # ### 100 4096\n### 200\n", ready(Mode::Base64));
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");

    assert_eq!(session.size("/tmp/one.bin").expect("a size"), 4096);
}

#[test]
fn a_tool_is_tried_and_not_asked_after() {
    let mut session = device(Mode::Base64, &ready(Mode::Base64));
    session.hello().expect("the device answers");

    let sent = sent(session);
    assert!(
        sent.contains(r"\echo x | \base64"),
        "base64 is tried by running it:\n{sent}"
    );
    assert!(
        !sent.contains("command -v"),
        "a tool is not asked after, it is tried:\n{sent}"
    );
}

#[test]
fn every_sum_has_a_probe_of_its_own() {
    for digest in Digest::every() {
        for has in [true, false] {
            let answer = format!("{}{}", ready(Mode::Base64), offers(digest, has));
            let mut session = device(Mode::Base64, &answer);
            session.hello().expect("the device answers");

            assert_eq!(
                session.offers_digest(digest).expect("the device answers"),
                has,
                "for {digest:?}"
            );

            let sent = sent(session);
            assert!(
                sent.contains(&format!("\\{}", digest.program())),
                "the probe of {digest:?} does not try the program:\n{sent}"
            );
        }
    }
}

#[test]
fn a_sum_the_device_has_not_got_is_no_error_in_itself() {
    let answer = format!(
        "{}{}{}",
        ready(Mode::Base64),
        offers(Digest::Sha256, false),
        offers(Digest::Sha1, true)
    );
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");

    assert!(!session.offers_digest(Digest::Sha256).expect("an answer"));
    assert!(session.offers_digest(Digest::Sha1).expect("an answer"));
}

#[test]
fn a_sum_is_the_first_field_of_what_the_program_said() {
    let answer = format!(
        "{}### 100\nDa9993e364706816aba3e25717850c26c9cd0d89d\n### 200\n",
        ready(Mode::Base64)
    );
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");

    assert_eq!(
        session.digest(Digest::Sha1, "/tmp/one.bin").expect("a sum"),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );

    let sent = sent(session);
    assert!(
        sent.contains("sha1sum '/tmp/one.bin'"),
        "the device is told to run the program on the file:\n{sent}"
    );
}

#[test]
fn a_device_that_says_nothing_about_a_file_is_reported() {
    let answer = format!("{}### 100\n### 200\n", ready(Mode::Base64));
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");

    let Err(ShXferError::NoDigest { program, path }) = session.digest(Digest::Sha1, "/tmp/one.bin")
    else {
        panic!("a device with nothing to say is reported");
    };
    assert_eq!(program, "sha1sum");
    assert_eq!(path, "/tmp/one.bin");
}

#[test]
fn a_size_nothing_could_say_stops_the_transfer_before_it_starts() {
    for mode in [Mode::Base64, Mode::Raw] {
        let answer = format!("{}### 100\n### 200\n", ready(mode));
        let mut session = device(mode, &answer);
        session.hello().expect("the device answers");

        let mut out = Vec::new();
        let outcome = session.retrieve("/tmp/one.bin", &mut out);
        assert!(
            matches!(outcome, Err(ShXferError::NoSize { .. })),
            "a chunk is cut from a size, got {outcome:?} for {mode:?}"
        );
        assert!(out.is_empty(), "nothing travelled");
    }
}

#[test]
fn a_chunk_of_base64_travels_in_a_here_document_the_shell_reads_itself() {
    let answer = format!("{}### 000\n### 200\n", ready(Mode::Base64));
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_size_check(false);

    session
        .store("/tmp/one.bin", &mut b"abc".as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    let opening: Vec<&str> = sent.lines().filter(|line| line.contains("<<")).collect();
    assert_eq!(
        opening.len(),
        1,
        "one chunk opens one here-document:\n{sent}"
    );
    assert!(
        opening[0].contains(">> '/tmp/one.bin'"),
        "the chunk is not appended to the file it belongs to:\n{}",
        opening[0]
    );
    assert!(
        !sent.contains("wc -c < '/tmp/one.bin'"),
        "a chunk measured the file, which is what the end of a file is for:\n{sent}"
    );
    assert!(
        !sent.contains("stty"),
        "base64 touched the settings of the line, which it never needs:\n{sent}"
    );

    let lines: Vec<&str> = sent.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("<<"))
        .expect("the here-document opens");
    assert_eq!(lines[at + 1], "YWJj", "the body follows the command");
    assert_eq!(lines[at + 2], "SHXFER_EOF", "the client closes it itself");
}

#[test]
fn a_chunk_of_raw_waits_to_be_told_the_line_is_ready() {
    let answer = format!("{}### 000\n### 001\n### 200\n", ready(Mode::Raw));
    let mut session = device(Mode::Raw, &answer);
    session.hello().expect("the device answers");
    session.set_size_check(false);

    session
        .store("/tmp/one.bin", &mut b"abc".as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    let switching: Vec<&str> = sent
        .lines()
        .filter(|line| line.contains("stty raw"))
        .collect();
    assert_eq!(
        switching.len(),
        1,
        "the line is switched once, by the command carrying the chunk:\n{sent}"
    );
    assert!(
        switching[0].contains("head -c 3") && switching[0].contains(">> '/tmp/one.bin'"),
        "the chunk does not count its body out onto the file:\n{}",
        switching[0]
    );
    assert!(
        !switching[0].contains("wc -c"),
        "a chunk measured the file, which is what the end of a file is for:\n{}",
        switching[0]
    );
    assert!(
        switching[0].contains("stty -raw echo"),
        "the command that switched the line does not put it back:\n{}",
        switching[0]
    );
}

#[test]
fn the_body_of_a_file_is_written_in_lines_a_terminal_will_take() {
    // A terminal in its usual mode holds one line and no more: what does not
    // fit is dropped where it stands, with no error and no gap, so a body
    // written in longer lines arrives wrong and says nothing about it. POSIX
    // promises 255 characters; `base64` itself writes 76, and so do we.
    const PROMISED: usize = 255;
    const WRITTEN: usize = 76;

    let answer = format!("{}### 000\n{}", ready(Mode::Base64), "### 200\n".repeat(64));
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_size_check(false);

    let body: Vec<u8> = (0..=255_u8).cycle().take(40_000).collect();
    session
        .store("/tmp/one.bin", &mut body.as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    let encoded: Vec<&str> = sent
        .lines()
        .filter(|line| {
            !line.is_empty()
                && line
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        })
        .collect();

    assert!(encoded.len() > 500, "the body is written in lines");
    for line in &encoded {
        assert!(
            line.len() <= WRITTEN && line.len() <= PROMISED,
            "a line of {} characters is more than a terminal promises to keep",
            line.len()
        );
    }
}

#[test]
fn a_file_that_was_written_is_measured_when_the_last_chunk_has_landed() {
    let answer = format!(
        "{}### 000\n{}### 100 150\n### 200\n",
        ready(Mode::Base64),
        "### 200\n".repeat(3)
    );
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_chunk_size(77);
    assert_eq!(session.slice(), 57);

    let body: Vec<u8> = (0..=255_u8).cycle().take(150).collect();
    session
        .store("/tmp/one.bin", &mut body.as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    let measuring: Vec<&str> = sent
        .lines()
        .filter(|line| line.contains("ls -ln"))
        .collect();
    assert_eq!(
        measuring.len(),
        1,
        "the file is measured once, when the last chunk has landed:\n{sent}"
    );
    let at = sent
        .lines()
        .position(|line| line.contains("ls -ln"))
        .expect("the file is measured");
    assert_eq!(
        at,
        sent.lines().count() - 1,
        "the measuring is the last thing said:\n{sent}"
    );
}

#[test]
fn a_file_the_device_holds_at_another_length_is_refused() {
    let answer = format!(
        "{}### 000\n{}### 100 93\n### 200\n",
        ready(Mode::Base64),
        "### 200\n".repeat(3)
    );
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_chunk_size(77);

    let body: Vec<u8> = (0..=255_u8).cycle().take(150).collect();
    let refused = session.store("/tmp/one.bin", &mut body.as_slice());

    assert!(
        matches!(
            refused,
            Err(ShXferError::WrongSize {
                expected: 150,
                got: 93,
                ..
            })
        ),
        "a file of another length is refused: {refused:?}"
    );
}

#[test]
fn a_file_is_believed_when_the_measuring_is_switched_off() {
    let answer = format!("{}### 000\n{}", ready(Mode::Base64), "### 200\n".repeat(8));
    let mut session = device(Mode::Base64, &answer);
    session.hello().expect("the device answers");
    session.set_chunk_size(77);
    session.set_size_check(false);

    let body: Vec<u8> = (0..=255_u8).cycle().take(150).collect();
    session
        .store("/tmp/one.bin", &mut body.as_slice())
        .expect("the file is stored");

    let sent = sent(session);
    assert!(
        !sent.contains("ls -ln"),
        "nothing measured the file:\n{sent}"
    );
}
