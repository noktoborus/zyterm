//! What a script may do, and what it is told.

mod common;

use common::{Said, run_to_end, start};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use zyt_script::{
    Cancel, Direction, LineChannel, Outcome, Progress, ScriptRun, StageKind, TargetKind, Targets,
    Value,
};

#[test]
fn a_script_writes_to_the_line_reads_the_answer_and_prints_its_own() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let cancel = Cancel::new();
    line.feed(b"/root\r\n");

    let mut run = ScriptRun::start(
        start("echo", line.clone(), said.clone(), cancel),
        Direction::Send,
    )
    .expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(5)), Outcome::Done);

    let mut written = Vec::new();
    line.take_output(&mut written);
    assert_eq!(written, b"pwd\r\r", "the command, then the finish key");
    assert!(said.echo().contains("said /root"), "{}", said.echo());
    assert_eq!(
        said.notices.lock().expect("free").as_slice(),
        ["the device answered"]
    );
    assert_eq!(
        said.progress.lock().expect("free").as_slice(),
        [Progress::Share(42)]
    );
}

#[test]
fn what_a_script_prints_goes_to_the_terminal_and_not_to_the_line() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("echo", line.clone(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    run_to_end(&mut run, Duration::from_secs(5));

    let mut written = Vec::new();
    line.take_output(&mut written);
    let written = String::from_utf8_lossy(&written).to_string();
    assert!(!written.contains("said"), "{written}");
    assert!(said.echo().ends_with("\r\n"), "{:?}", said.echo());
}

#[test]
fn a_script_talks_to_a_program_through_its_pipes() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("pipes", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(10)), Outcome::Done);

    let echo = said.echo();
    assert!(echo.contains("got hello"), "{echo}");
    assert!(echo.contains("code 0"), "{echo}");
    assert!(echo.contains("said bad"), "{echo}");
}

#[test]
fn a_dialog_is_described_by_the_script_and_answered_by_whoever_draws_it() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    *said.answers.lock().expect("free") = Some(BTreeMap::from([(
        "host".to_string(),
        Value::Text("other".to_string()),
    )]));

    let mut run = ScriptRun::start(
        start("asks", line.clone(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(5)), Outcome::Done);

    let asked = said.asked.lock().expect("free");
    let form = asked.first().expect("it asked once");
    let kinds: Vec<&str> = form.fields.iter().map(|field| field.kind.name()).collect();
    assert_eq!(
        kinds,
        [
            "text",
            "textarea",
            "switch",
            "one_of",
            "select",
            "many_of",
            "separator"
        ]
    );

    let mut written = Vec::new();
    line.take_output(&mut written);
    assert_eq!(
        String::from_utf8_lossy(&written),
        "other b64 true",
        "the answers come back under their names"
    );
}

#[test]
fn the_shell_of_a_script_is_read_from_files_of_its_own() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("reads", line.clone(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(5)), Outcome::Done);

    let mut written = Vec::new();
    line.take_output(&mut written);
    let written = String::from_utf8_lossy(&written).to_string();
    assert_eq!(
        written, "\\echo '##''# 100' \\wc -c < '/tmp/two words' \\echo '##''# 200'",
        "folded into one line, with the hole filled and the comment dropped"
    );
    assert!(said.echo().contains("files 1"), "{}", said.echo());
}

#[test]
fn a_script_is_told_what_it_carries_and_what_the_source_answers() {
    let line = LineChannel::new();
    let said = Arc::new(Said::default());
    let mut begin = start("carries", line.clone(), said, Cancel::new());
    begin.target = Targets {
        kind: TargetKind::File,
        paths: vec![PathBuf::from("/tmp/board.itb")],
    };
    begin.variables = BTreeMap::from([("remote_host".to_string(), "board".to_string())]);

    let mut run = ScriptRun::start(begin, Direction::Send).expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(5)), Outcome::Done);

    let mut written = Vec::new();
    line.take_output(&mut written);
    assert_eq!(
        String::from_utf8_lossy(&written),
        "file|/tmp/board.itb|board.itb|board|itb|board|4.0K"
    );
}

#[test]
fn a_script_that_does_not_offer_the_direction_is_refused_before_it_runs() {
    let error = ScriptRun::start(
        start(
            "echo",
            LineChannel::new(),
            Arc::new(Said::default()),
            Cancel::new(),
        ),
        Direction::Receive,
    )
    .expect_err("it is refused");
    assert!(
        matches!(error, zyt_script::ScriptError::NoDirection { .. }),
        "{error:?}"
    );
}

#[test]
fn a_directory_whose_manifest_says_nothing_usable_is_left_out_with_a_word() {
    let roots = vec![common::scripts()];
    let library = zyt_script::Library::load(&roots);

    assert!(
        library.find("broken").is_err(),
        "a directory nothing can be started from is not a script"
    );
    assert!(
        library
            .problems()
            .iter()
            .any(|problem| problem.path.ends_with("broken") && problem.said.contains("name")),
        "it says what is wrong with it: {:?}",
        library.problems()
    );
}

/// A stage is data the window words, bytes say the share when the whole is
/// known and nothing more than that they move when it is not.
#[test]
fn a_script_says_where_it_stands_and_how_many_bytes_went() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("stages", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    assert_eq!(run_to_end(&mut run, Duration::from_secs(5)), Outcome::Done);

    let stages = said.stages.lock().expect("free").clone();
    assert_eq!(stages.len(), 3);
    assert_eq!(stages[0].kind, StageKind::Sending);
    assert_eq!(stages[0].name.as_deref(), Some("a.bin"));
    assert_eq!((stages[0].file, stages[0].files), (Some(2), Some(3)));
    assert_eq!(stages[1].kind, StageKind::Other, "a text alone");
    assert_eq!(stages[1].text.as_deref(), Some("warming up"));
    assert_eq!(
        (stages[2].kind, stages[2].count),
        (StageKind::Retrying, Some(4))
    );

    assert_eq!(
        said.bytes.lock().expect("free").as_slice(),
        [(512, Some(2048)), (100, None)]
    );
    assert_eq!(
        said.progress.lock().expect("free").as_slice(),
        [
            Progress::Share(25),
            Progress::Indeterminate,
            Progress::Removed
        ]
    );
}

/// A stage under a name none has fails the run, saying the name.
#[test]
fn a_stage_no_kind_is_called_fails_the_run() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("misnamed", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    assert!(matches!(
        run_to_end(&mut run, Duration::from_secs(5)),
        Outcome::Failed(_)
    ));
    let failure = run.take_failure().map(|error| error.to_string());
    assert!(
        failure
            .as_deref()
            .is_some_and(|text| text.contains("sendnig")),
        "{failure:?}"
    );
}

/// True once that port can be listened on again, which the kernel allows a
/// moment after both ends of a connection through it have let go.
fn port_freed(address: std::net::SocketAddr) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::net::TcpListener::bind(address).is_err() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// Where the script `listens` said it listens.
fn listening_at(said: &Said) -> std::net::SocketAddr {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let echo = said.echo();
        if let Some(at) = echo.split("at ").nth(1) {
            let address = at.split_whitespace().next().unwrap_or_default();
            return address.parse().expect("an address");
        }
        assert!(std::time::Instant::now() < deadline, "it never said where");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_script_listens_answers_one_client_and_refuses_a_second() {
    use std::io::{Read, Write};

    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("listens", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    let address = listening_at(&said);

    let mut client = std::net::TcpStream::connect(address).expect("connect");
    let mut second = std::net::TcpStream::connect(address).expect("connect a second");
    let mut refused = String::new();
    second.read_to_string(&mut refused).expect("read");
    assert_eq!(refused, "busy\r\n");

    client.write_all(b"hello").expect("write");
    let mut answer = [0u8; 9];
    client.read_exact(&mut answer).expect("read");
    assert_eq!(&answer, b"got hello");
    drop(client);

    assert_eq!(run_to_end(&mut run, Duration::from_secs(10)), Outcome::Done);
    assert!(said.echo().contains("then nil closed"), "{}", said.echo());
}

#[test]
fn a_script_that_fails_leaves_no_port_taken_and_no_client_connected() {
    use std::io::{Read, Write};

    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("listens", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    let address = listening_at(&said);

    let mut client = std::net::TcpStream::connect(address).expect("connect");
    client.write_all(b"fail").expect("write");
    let mut rest = Vec::new();
    client
        .read_to_end(&mut rest)
        .expect("the end of the connection");
    assert_eq!(rest, b"got fail", "what it said, and then the end");

    assert!(matches!(
        run_to_end(&mut run, Duration::from_secs(10)),
        Outcome::Failed(_)
    ));
    assert!(port_freed(address), "the port is free again");
}

#[test]
fn a_cancelled_script_closes_its_port() {
    let said = Arc::new(Said::default());
    let mut run = ScriptRun::start(
        start("listens", LineChannel::new(), said.clone(), Cancel::new()),
        Direction::Send,
    )
    .expect("the script loads");
    let address = listening_at(&said);

    run.cancel();
    assert_eq!(
        run_to_end(&mut run, Duration::from_secs(10)),
        Outcome::Cancelled
    );
    assert!(port_freed(address), "the port is free again");
}
