use super::*;

#[derive(Debug, PartialEq, Eq)]
enum Owned {
    Plain(Vec<u8>),
    Entered,
    Control(Vec<u8>),
    Left,
}

/// Splits the chunks in turn, joining the pieces a chunk boundary cut.
fn split(chunks: &[&[u8]]) -> Vec<Owned> {
    let mut splitter = Splitter::new();
    let mut out: Vec<Owned> = Vec::new();
    for chunk in chunks {
        let mut chunk = chunk.to_vec();
        splitter.split(&mut chunk, |piece| match (piece, out.last_mut()) {
            (Piece::Plain(bytes), Some(Owned::Plain(last))) => last.extend_from_slice(bytes),
            (Piece::Control(bytes), Some(Owned::Control(last))) => last.extend_from_slice(bytes),
            (Piece::Plain(bytes), _) => out.push(Owned::Plain(bytes.to_vec())),
            (Piece::Control(bytes), _) => out.push(Owned::Control(bytes.to_vec())),
            (Piece::Entered, _) => out.push(Owned::Entered),
            (Piece::Left, _) => out.push(Owned::Left),
        });
    }
    out
}

/// Every cut of `stream` into two chunks gives what it gives whole.
fn assert_every_cut(stream: &[u8], expected: &[Owned]) {
    assert_eq!(split(&[stream]), expected, "whole");
    for at in 0..=stream.len() {
        let (head, tail) = stream.split_at(at);
        assert_eq!(split(&[head, tail]), expected, "cut at {at}");
    }
    let bytes: Vec<&[u8]> = stream.chunks(1).collect();
    assert_eq!(split(&bytes), expected, "byte by byte");
}

fn plain(bytes: &[u8]) -> Owned {
    Owned::Plain(bytes.to_vec())
}

fn control(bytes: &[u8]) -> Owned {
    Owned::Control(bytes.to_vec())
}

const OPENING: &[u8] = b"%begin 1 260 0\r\n%end 1 260 0\r\n";
const DETACH: &[u8] = b"%exit\r\n";

#[test]
fn plain_bytes_pass_untouched() {
    let stream = b"$ ls\r\n\x1b[1mbold\x1b[0m \x1b]0;title\x1b\\ \xd1\x9c\x7f";
    assert_every_cut(stream, &[plain(stream)]);
}

#[test]
fn the_mode_starts_at_the_start_of_a_chunk() {
    let stream = [b"\x1bP1000p".as_slice(), OPENING].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(OPENING)]);
}

#[test]
fn the_mode_starts_in_the_middle_of_a_chunk() {
    let stream = [b"$ tmux -CC\r\n\x1bP1000p".as_slice(), OPENING].concat();
    assert_every_cut(
        &stream,
        &[plain(b"$ tmux -CC\r\n"), Owned::Entered, control(OPENING)],
    );
}

#[test]
fn the_mode_starts_at_the_end_of_a_chunk() {
    assert_every_cut(
        b"$ tmux -CC\r\n\x1bP1000p",
        &[plain(b"$ tmux -CC\r\n"), Owned::Entered],
    );
}

#[test]
fn a_held_head_that_is_not_the_mode_is_given_back() {
    let mut splitter = Splitter::new();
    let mut pieces = Vec::new();
    for chunk in [b"a\x1bP10".as_slice(), b"0;0;8q", b"b"] {
        splitter.split(&mut chunk.to_vec(), |piece| match piece {
            Piece::Plain(bytes) => pieces.push(bytes.to_vec()),
            other => panic!("{other:?}"),
        });
    }
    assert_eq!(pieces, [b"a".as_slice(), b"\x1bP10", b"0;0;8q", b"b"]);
}

#[test]
fn a_held_head_followed_by_the_mode() {
    let stream = b"\x1bP1\x1bP1000p%";
    assert_eq!(
        split(&[b"\x1bP1", b"\x1bP1000p%"]),
        [plain(b"\x1bP1"), Owned::Entered, control(b"%")]
    );
    assert_every_cut(stream, &[plain(b"\x1bP1"), Owned::Entered, control(b"%")]);
}

#[test]
fn another_dcs_stays_plain() {
    let stream = b"\x1bP0;0;8q\"1;1;2;2#0;2;0;0;0#0!2~\x1b\\\x1bP1000q\x1bP+q544e\x1b\\";
    assert_every_cut(stream, &[plain(stream)]);
}

#[test]
fn plain_bytes_after_the_end_of_the_mode() {
    let stream = [b"\x1bP1000p".as_slice(), OPENING, DETACH, b"\x1b\\$ "].concat();
    let inside = [OPENING, DETACH].concat();
    assert_every_cut(
        &stream,
        &[Owned::Entered, control(&inside), Owned::Left, plain(b"$ ")],
    );
}

#[test]
fn the_mode_again_after_its_end() {
    let stream = b"\x1bP1000p%exit\n\x1b\\x\x1bP1000p%exit\n\x1b\\";
    assert_every_cut(
        stream,
        &[
            Owned::Entered,
            control(b"%exit\n"),
            Owned::Left,
            plain(b"x"),
            Owned::Entered,
            control(b"%exit\n"),
            Owned::Left,
        ],
    );
}

#[test]
fn bytes_that_end_a_dcs_elsewhere_do_not_end_the_mode() {
    let inside = b"%output %0 \xd1\x9c\x7f\x18\x1a\x9c\r\n%exit detached\x9c\r\n";
    let stream = [b"\x1bP1000p".as_slice(), inside].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(inside)]);
}

#[test]
fn a_hyperlink_in_a_reply_does_not_end_the_mode() {
    let inside: &[u8] = b"%begin 1 270 1\r\n\x1b]8;;http://a\x1b\\link\x1b]8;;\x1b\\\r\n\
        \x1b\\ a line that lost its head\r\n\x0eshifted\x0f\r\n%end 1 270 1\r\n";
    let stream = [b"\x1bP1000p".as_slice(), inside].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(inside)]);
}

#[test]
fn the_end_comes_only_right_after_exit() {
    let inside: &[u8] = b"%exit\r\n%output %0 x\r\n\x1b\\%end\r\n";
    let stream = [b"\x1bP1000p".as_slice(), inside].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(inside)]);
}

#[test]
fn an_exit_with_a_reason_ends_the_mode() {
    let stream = b"\x1bP1000p%exit server exited\r\n\x1b\\";
    assert_every_cut(
        stream,
        &[
            Owned::Entered,
            control(b"%exit server exited\r\n"),
            Owned::Left,
        ],
    );
}

#[test]
fn an_escape_after_exit_that_is_not_the_end_stays_in_the_mode() {
    let inside: &[u8] = b"%exit\r\n\x1b[m\r\n";
    let stream = [b"\x1bP1000p".as_slice(), inside].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(inside)]);
}

#[test]
fn the_end_cut_at_every_offset() {
    let stream = [b"\x1bP1000p".as_slice(), OPENING, DETACH, b"\x1b\\"].concat();
    let inside = [OPENING, DETACH].concat();
    assert_every_cut(&stream, &[Owned::Entered, control(&inside), Owned::Left]);
}

#[test]
fn inside_follows_the_mode() {
    let mut splitter = Splitter::new();
    assert!(!splitter.inside());
    splitter.split(&mut b"\x1bP1000".to_vec(), |_| {});
    assert!(!splitter.inside());
    splitter.split(&mut b"p".to_vec(), |_| {});
    assert!(splitter.inside());
    splitter.split(&mut b"%exit\n\x1b".to_vec(), |_| {});
    assert!(splitter.inside());
    splitter.split(&mut b"\\".to_vec(), |_| {});
    assert!(!splitter.inside());
}

#[test]
fn reset_reads_the_rest_as_plain() {
    let mut splitter = Splitter::new();
    splitter.split(&mut b"\x1bP1000pnot tmux".to_vec(), |_| {});
    assert!(splitter.inside());
    splitter.reset();
    assert!(!splitter.inside());
    let mut pieces = Vec::new();
    splitter.split(&mut b"%exit\n\x1b\\x".to_vec(), |piece| {
        pieces.push(format!("{piece:?}"));
    });
    assert_eq!(pieces, [format!("{:?}", Piece::Plain(b"%exit\n\x1b\\x"))]);
}
