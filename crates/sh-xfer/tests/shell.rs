//! The protocol against a real shell, which is the only thing it needs.
//!
//! Two kinds of far end are used. A pipe is what an `ssh` session or a program
//! driving another program looks like: there is no line to switch, so only
//! base64 can carry a body there, and the probe says so. A pseudo terminal is
//! what a console looks like, with a line discipline that echoes and that
//! `stty` can put into binary mode, which is what the raw transport needs.

use sh_xfer::{Digest, EntryKind, Mode, Reader, Session};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// A shell reading commands from a pipe, which is what a console is without
/// the console.
struct Shell {
    child: Child,
}

impl Shell {
    fn start() -> (Self, Session<std::process::ChildStdin>) {
        Self::sending(Mode::Base64)
    }

    /// A shell sending the scripts of one mode. A pipe has no line to switch,
    /// so only base64 reaches one; raw is what a console is started for.
    fn sending(mode: Mode) -> (Self, Session<std::process::ChildStdin>) {
        let mut child = Command::new("sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell starts");
        let out = child.stdout.take().expect("the shell has an output");
        let input = child.stdin.take().expect("the shell has an input");
        let reader = Reader::new(out, Duration::from_secs(10));
        (Self { child }, Session::new(reader, input, mode))
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A shell on a pseudo terminal, which is what a console really is.
struct Console {
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Console {
    /// A console sending the scripts of one mode.
    fn sending(mode: Mode) -> (Self, Session<Box<dyn Write + Send>>) {
        let pair = portable_pty::native_pty_system()
            .openpty(portable_pty::PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a pseudo terminal opens");
        let child = pair
            .slave
            .spawn_command(portable_pty::CommandBuilder::new("sh"))
            .expect("a shell starts on it");
        let out = pair.master.try_clone_reader().expect("it can be read");
        let input = pair.master.take_writer().expect("it can be written");
        let reader = Reader::new(out, Duration::from_secs(10));
        (
            Self {
                _child: child,
                _master: pair.master,
            },
            Session::new(reader, input, mode),
        )
    }
}

fn directory(case: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("sh-xfer-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("the directory is made");
    path
}

/// Counts what a session says while it works, which is what a caller of it
/// would print.
#[derive(Default)]
struct Heard {
    commands: usize,
    chunks: Vec<(u64, u64)>,
    carried: Vec<u64>,
}

impl sh_xfer::Report for Heard {
    fn command(&mut self, _script: &str) {
        self.commands += 1;
    }

    fn chunk(&mut self, start: u64, end: u64) {
        self.chunks.push((start, end));
    }

    fn carried(&mut self, bytes: u64) {
        self.carried.push(bytes);
    }
}

#[test]
fn the_shell_says_it_has_what_base64_needs() {
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");
    assert_eq!(session.mode(), Mode::Base64);
}

#[test]
fn where_the_shell_stands_comes_back() {
    let root = directory("pwd");
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    session
        .make_directory(&root.join("deep").to_string_lossy())
        .expect("the directory is made");
    let real = session
        .canonical(&root.join("deep").to_string_lossy())
        .expect("the path comes back");
    assert!(real.ends_with("deep"), "{real}");
    assert!(!session.pwd().expect("a path comes back").is_empty());
}

#[test]
fn a_listing_tells_a_file_from_a_directory() {
    let root = directory("list");
    std::fs::write(root.join("one.txt"), b"hello").expect("the file is written");
    std::fs::create_dir_all(root.join("sub")).expect("the directory is made");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut entries = session
        .list(&root.to_string_lossy())
        .expect("the listing comes back");
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "one.txt");
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].size, 5);
    assert_eq!(entries[1].name, "sub");
    assert_eq!(entries[1].kind, EntryKind::Directory);
}

#[test]
fn a_path_says_what_it_is() {
    let root = directory("kind");
    std::fs::write(root.join("one.txt"), b"hello").expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    assert_eq!(
        session
            .kind(&root.join("one.txt").to_string_lossy())
            .expect("an answer"),
        EntryKind::File
    );
    assert_eq!(
        session
            .size(&root.join("one.txt").to_string_lossy())
            .expect("a size"),
        5
    );
    assert_eq!(
        session.kind(&root.to_string_lossy()).expect("an answer"),
        EntryKind::Directory
    );
    assert_eq!(
        session
            .kind(&root.join("nothing").to_string_lossy())
            .expect("an answer"),
        EntryKind::Other
    );
}

fn round_trip<W: Write>(session: &mut Session<W>, root: &Path, body: &[u8]) {
    let source = root.join("source.bin");
    let stored = root.join("stored.bin");
    std::fs::write(&source, body).expect("the file is written");

    session.hello().expect("the shell answers");

    let mut file = std::fs::File::open(&source).expect("the file opens");
    session
        .store(&stored.to_string_lossy(), &mut file)
        .expect("the file is stored");
    assert_eq!(std::fs::read(&stored).expect("the file is there"), body);

    let mut back = Vec::new();
    session
        .retrieve(&stored.to_string_lossy(), &mut back)
        .expect("the file comes back");
    assert_eq!(back, body);
}

fn over_a_pipe(case: &str, body: &[u8]) {
    let root = directory(case);
    let (_shell, mut session) = Shell::start();
    round_trip(&mut session, &root, body);
}

fn over_a_console(case: &str, mode: Mode, body: &[u8]) {
    let root = directory(case);
    let (_console, mut session) = Console::sending(mode);
    round_trip(&mut session, &root, body);
}

#[test]
fn a_file_goes_there_and_comes_back_as_base64() {
    over_a_pipe("b64", b"hello, shell\n");
}

#[test]
fn every_byte_there_is_survives_base64() {
    let body: Vec<u8> = (0..=255_u8).cycle().take(9000).collect();
    over_a_pipe("b64-bytes", &body);
}

#[test]
fn a_file_of_no_bytes_travels_as_base64() {
    over_a_pipe("b64-empty", b"");
}

#[test]
fn a_file_goes_there_and_comes_back_raw_over_a_console() {
    over_a_console("raw", Mode::Raw, b"hello, console\n");
}

#[test]
fn every_byte_there_is_survives_raw_over_a_console() {
    let body: Vec<u8> = (0..=255_u8).cycle().take(9000).collect();
    over_a_console("raw-bytes", Mode::Raw, &body);
}

#[test]
fn a_pipe_cannot_carry_raw_and_says_so() {
    let (_shell, mut session) = Shell::sending(Mode::Raw);
    let outcome = session.hello();
    let Err(sh_xfer::ShXferError::MissingCommand { command }) = outcome else {
        panic!("a pipe has no line to switch, got {outcome:?}");
    };
    assert_eq!(command, "stty", "the probe names what a pipe has not got");
}

#[test]
fn a_file_that_is_not_there_is_refused() {
    let root = directory("missing");
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut out = Vec::new();
    let outcome = session.retrieve(&root.join("nothing.bin").to_string_lossy(), &mut out);
    assert!(matches!(outcome, Err(sh_xfer::ShXferError::Refused { .. })));
}

#[test]
fn a_tree_travels_with_its_shape() {
    let root = directory("tree");
    let from = root.join("from");
    std::fs::create_dir_all(from.join("sub")).expect("the directory is made");
    std::fs::write(from.join("top.txt"), b"1").expect("the file is written");
    std::fs::write(from.join("sub/low.txt"), b"22").expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let items = sh_xfer::local_items(std::slice::from_ref(&from)).expect("the walk works");
    let into = root.join("into");
    for item in &items {
        let target = into.join(&item.relative);
        session
            .make_directory(&target.parent().expect("a parent").to_string_lossy())
            .expect("the directory is made");
        let mut file = std::fs::File::open(&item.path).expect("the file opens");
        session
            .store(&target.to_string_lossy(), &mut file)
            .expect("the file is stored");
    }

    assert_eq!(
        std::fs::read(into.join("from/sub/low.txt")).expect("the file is there"),
        b"22"
    );

    let back = sh_xfer::remote_items(&mut session, &[into.to_string_lossy().to_string()])
        .expect("the walk of the device works");
    let mut names: Vec<&str> = back.iter().map(|item| item.relative.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["into/from/sub/low.txt", "into/from/top.txt"]);
}

#[test]
fn a_name_with_a_space_and_a_quote_stays_one_name() {
    let root = directory("quoting");
    let odd = root.join("two words it's here.bin");
    std::fs::write(&odd, b"odd").expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut back = Vec::new();
    session
        .retrieve(&odd.to_string_lossy(), &mut back)
        .expect("the file comes back");
    assert_eq!(back, b"odd");
}

#[test]
fn a_share_of_the_whole_is_reported_while_a_file_travels() {
    let root = directory("progress");
    let file = root.join("big.bin");
    let body = vec![7_u8; 20_000];
    std::fs::write(&file, &body).expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let heard = std::rc::Rc::new(std::cell::RefCell::new(Heard::default()));
    session.listen(Box::new(heard.clone()));
    let mut out = Vec::new();
    session
        .retrieve(&file.to_string_lossy(), &mut out)
        .expect("the file comes back");

    let heard = heard.borrow();
    assert!(
        heard.chunks.len() > 1,
        "a big file travels in more than one chunk"
    );
    assert_eq!(
        heard.chunks.first().expect("a chunk").0,
        0,
        "the first chunk starts at the beginning"
    );
    assert_eq!(
        heard.chunks.last().expect("a chunk").1,
        body.len() as u64,
        "the last chunk ends at the end"
    );
    assert_eq!(
        heard.carried.iter().sum::<u64>(),
        body.len() as u64,
        "what was said to have travelled is the whole file"
    );
    assert_eq!(
        heard.carried.len(),
        heard.chunks.len(),
        "every chunk that started was said to have ended"
    );
    assert!(
        heard.commands > heard.chunks.len(),
        "every command was said, the size among them"
    );
}

#[test]
fn the_echo_the_console_makes_is_not_taken_for_an_answer() {
    let root = directory("echo");
    let file = root.join("one.bin");
    std::fs::write(&file, b"x").expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut out = Vec::new();
    session
        .retrieve(&file.to_string_lossy(), &mut out)
        .expect("the file comes back");
    assert_eq!(out, b"x");

    let _ = writeln!(std::io::sink(), "{}", Path::new("/").display());
}

#[test]
fn the_shell_says_what_it_can_check_a_file_with() {
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let offered: Vec<Digest> = Digest::every()
        .into_iter()
        .filter(|digest| session.offers_digest(*digest).expect("the shell answers"))
        .collect();
    assert!(
        !offered.is_empty(),
        "a shell with coreutils on it can check a file"
    );
}

fn checked_round_trip(case: &str, digest: Digest) {
    let root = directory(case);
    let source = root.join("source.bin");
    let stored = root.join("stored.bin");
    let body: Vec<u8> = (0..=255_u8).cycle().take(4000).collect();
    std::fs::write(&source, &body).expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");
    assert!(
        session.offers_digest(digest).expect("the shell answers"),
        "a shell with coreutils on it has {digest:?}"
    );

    let mut file = std::fs::File::open(&source).expect("the file opens");
    session
        .store(&stored.to_string_lossy(), &mut file)
        .expect("the file is stored");

    let theirs = session
        .digest(digest, &stored.to_string_lossy())
        .expect("the device says a sum");
    let ours = digest.of_file(&source).expect("this machine says a sum");
    assert_eq!(ours, theirs, "what was stored is not what was sent");

    let back = root.join("back.bin");
    let mut out = std::fs::File::create(&back).expect("the file is made");
    session
        .retrieve(&stored.to_string_lossy(), &mut out)
        .expect("the file comes back");
    drop(out);

    let ours = digest.of_file(&back).expect("this machine says a sum");
    assert_eq!(ours, theirs, "what came back is not what was there");
}

#[test]
fn a_file_is_checked_with_sha1sum() {
    checked_round_trip("sum-sha1", Digest::Sha1);
}

#[test]
fn a_file_is_checked_with_md5sum() {
    checked_round_trip("sum-md5", Digest::Md5);
}

#[test]
fn a_file_is_checked_with_sha256sum() {
    checked_round_trip("sum-sha256", Digest::Sha256);
}

#[test]
fn a_file_that_changed_on_the_device_does_not_match() {
    let root = directory("sum-mismatch");
    let source = root.join("source.bin");
    let stored = root.join("stored.bin");
    std::fs::write(&source, b"the bytes that were sent").expect("the file is written");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut file = std::fs::File::open(&source).expect("the file opens");
    session
        .store(&stored.to_string_lossy(), &mut file)
        .expect("the file is stored");

    std::fs::write(&stored, b"something else entirely!").expect("the file is meddled with");

    let theirs = session
        .digest(Digest::Sha1, &stored.to_string_lossy())
        .expect("the device says a sum");
    let ours = Digest::Sha1
        .of_file(&source)
        .expect("this machine says a sum");
    assert_ne!(ours, theirs, "a file that changed must not match");
}

/// A shell told something before the conversation starts, the way a device
/// whose profile wraps its utilities would have been.
fn shell_told(setup: &str) -> (Shell, Session<std::process::ChildStdin>) {
    let mut child = Command::new("sh")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("a shell starts");
    let out = child.stdout.take().expect("the shell has an output");
    let mut input = child.stdin.take().expect("the shell has an input");

    writeln!(input, "{setup}").expect("the shell is told");
    input.flush().expect("the shell is told");

    let reader = Reader::new(out, Duration::from_secs(10));
    (Shell { child }, Session::new(reader, input, Mode::Base64))
}

fn tree(case: &str) -> PathBuf {
    let root = directory(case);
    std::fs::write(root.join("messages"), b"four").expect("the file is written");
    std::fs::write(root.join("two words.bin"), b"ab").expect("the file is written");
    std::fs::create_dir_all(root.join("logs")).expect("the directory is made");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("messages"), root.join("as-link"))
            .expect("the link is made");
        std::os::unix::fs::symlink(root.join("logs"), root.join("to-logs"))
            .expect("the link is made");
        std::os::unix::fs::symlink(root.join("gone"), root.join("dangling"))
            .expect("the link is made");
    }
    root
}

fn names_of(entries: &[sh_xfer::Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[test]
fn a_listing_says_what_each_entry_is() {
    let root = tree("listing");
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    let mut entries = session
        .list(&root.to_string_lossy())
        .expect("the listing comes back");
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(
        names_of(&entries),
        vec!["as-link", "logs", "messages", "to-logs", "two words.bin"],
        "a dangling link leads nowhere and is left out"
    );

    let kinds: Vec<EntryKind> = entries.iter().map(|entry| entry.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EntryKind::File, // a link to a file is a file
            EntryKind::Directory,
            EntryKind::File,
            EntryKind::Directory, // a link to a directory is a directory
            EntryKind::File,
        ]
    );

    let sizes: Vec<u64> = entries.iter().map(|entry| entry.size).collect();
    assert_eq!(sizes[2], 4, "the size of a file comes back");
    assert_eq!(sizes[4], 2, "so does the size of one with a space in it");
}

#[test]
fn a_listing_survives_a_shell_whose_ls_answers_nonsense() {
    let root = tree("wrapped");

    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");
    let mut plain = session
        .list(&root.to_string_lossy())
        .expect("the listing comes back");
    plain.sort_by(|a, b| a.name.cmp(&b.name));

    let (_shell, mut session) = shell_told("ls() { echo broken; }; grep() { echo broken; }");
    session.hello().expect("the shell answers");
    let mut wrapped = session
        .list(&root.to_string_lossy())
        .expect("the listing comes back even so");
    wrapped.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(
        names_of(&wrapped),
        names_of(&plain),
        "the names come from the shell itself, so no ls can take them away"
    );
    assert_eq!(
        wrapped
            .iter()
            .map(|one| one.kind)
            .collect::<Vec<EntryKind>>(),
        plain.iter().map(|one| one.kind).collect::<Vec<EntryKind>>(),
        "and so does what each of them is"
    );
}

#[test]
fn a_listing_of_nothing_comes_back_empty_and_not_broken() {
    let root = directory("bare");
    let (_shell, mut session) = Shell::start();
    session.hello().expect("the shell answers");

    assert!(
        session
            .list(&root.to_string_lossy())
            .expect("the listing comes back")
            .is_empty()
    );
}

#[test]
fn a_file_comes_back_from_a_shell_whose_ls_answers_nonsense() {
    let root = directory("wrapped-size");
    let file = root.join("one.bin");
    std::fs::write(&file, b"twelve bytes").expect("the file is written");

    let (_shell, mut session) = shell_told("ls() { echo broken; }");
    session.hello().expect("the shell answers");

    assert_eq!(
        session.size(&file.to_string_lossy()).expect("a size"),
        12,
        "the size of a file does not depend on the layout of ls"
    );

    let mut back = Vec::new();
    session
        .retrieve(&file.to_string_lossy(), &mut back)
        .expect("the file comes back even so");
    assert_eq!(back, b"twelve bytes");
}

#[test]
fn a_file_whose_size_nothing_on_the_device_can_say_does_not_travel() {
    let root = directory("sizeless");
    let file = root.join("one.bin");
    std::fs::write(&file, b"never gets to travel").expect("the file is written");

    let (_shell, mut session) = shell_told("ls() { echo broken; }; wc() { echo broken; }");
    session.hello().expect("the shell answers");

    let mut back = Vec::new();
    let outcome = session.retrieve(&file.to_string_lossy(), &mut back);
    assert!(
        matches!(outcome, Err(sh_xfer::ShXferError::NoSize { .. })),
        "a chunk is cut from a size, got {outcome:?}"
    );
    assert!(back.is_empty(), "nothing travelled");
}

#[test]
fn a_file_travels_whole_however_small_the_chunks_are() {
    for (case, length) in [("tiny", 200), ("exact", 57), ("one-more", 58), ("none", 0)] {
        let root = directory(&format!("chunks-{case}"));
        let source = root.join("source.bin");
        let stored = root.join("stored.bin");
        let body: Vec<u8> = (0..=255_u8).cycle().take(length).collect();
        std::fs::write(&source, &body).expect("the file is written");

        let (_shell, mut session) = Shell::start();
        session.hello().expect("the shell answers");
        session.set_chunk_size(64);

        let mut file = std::fs::File::open(&source).expect("the file opens");
        session
            .store(&stored.to_string_lossy(), &mut file)
            .expect("the file is stored");
        assert_eq!(
            std::fs::read(&stored).expect("the file is there"),
            body,
            "{case} did not arrive whole"
        );

        let mut back = Vec::new();
        session
            .retrieve(&stored.to_string_lossy(), &mut back)
            .expect("the file comes back");
        assert_eq!(back, body, "{case} did not come back whole");
    }
}

/// A body big enough to cross a console many chunks over, whichever mode
/// carries it.
fn a_body_that_takes_many_chunks(case: &str, mode: Mode) {
    let root = directory(case);
    let source = root.join("source.bin");
    let stored = root.join("stored.bin");
    let body: Vec<u8> = (0..=255_u8).cycle().take(8000).collect();
    std::fs::write(&source, &body).expect("the file is written");

    let (_console, mut session) = Console::sending(mode);
    session.hello().expect("the shell answers");

    let mut file = std::fs::File::open(&source).expect("the file opens");
    session
        .store(&stored.to_string_lossy(), &mut file)
        .expect("the file is stored");
    assert_eq!(std::fs::read(&stored).expect("the file is there"), body);
}

#[test]
fn a_console_does_not_echo_the_body_of_a_raw_chunk_back() {
    a_body_that_takes_many_chunks("hushed", Mode::Raw);
}

#[test]
fn a_console_that_echoes_the_body_back_still_takes_it_whole() {
    // base64 carries its body in a here-document, which the shell reads while
    // it is still parsing the command — before any `stty` on the line could
    // run. The echo of the console therefore comes back with it, and is
    // skipped as the noise it is.
    a_body_that_takes_many_chunks("echoed", Mode::Base64);
}
