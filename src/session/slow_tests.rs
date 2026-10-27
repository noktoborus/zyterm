//! What goes over a line, behind `experiments/pty-throttle` at the slow end of
//! a real port (`RULE test.slow_line`): a pseudo terminal answers at once and
//! hides what a port shows — a reply cut across chunks, keys typed while a
//! command is on its way, a screen drawn before its snapshot came.

use super::transfer_tests::terminal_text;
use super::*;

/// The speed of the line: the slow end of what a port of a board runs at.
const BAUD: u32 = 9600;

/// Bytes a second at `BAUD`, ten bits a byte for `8N1`.
const BYTES_A_SECOND: u64 = BAUD as u64 / 10;

/// The most a step of a test waits: what `bytes` cost at the speed of the
/// line, twice over, and a second for tmux itself.
fn bound(bytes: u64) -> Duration {
    Duration::from_millis(bytes * 2 * 1000 / BYTES_A_SECOND + 1000)
}

/// The binary of `pty-throttle`, as cargo builds it and names it.
///
/// The test asks the cargo that runs it (`CARGO`) to build the package and
/// takes the path from what it says, so no directory of a build is assumed. A
/// test without it fails: one skipped is one nobody reads.
fn throttle() -> std::path::PathBuf {
    static BUILT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    BUILT.get_or_init(build_throttle).clone()
}

/// Builds `pty-throttle` and answers with the path of its executable.
fn build_throttle() -> std::path::PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args([
            "build",
            "--package",
            "pty-throttle",
            "--message-format=json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "pty-throttle does not build:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(r#""reason":"compiler-artifact""#))
        .find_map(executable)
        .expect("cargo names the executable of pty-throttle")
}

/// The executable a message of cargo names, when it is the one of
/// `pty-throttle`.
fn executable(message: &str) -> Option<std::path::PathBuf> {
    const KEY: &str = r#""executable":""#;
    let start = message.find(KEY)? + KEY.len();
    let path = &message[start..start + message[start..].find('"')?];
    let name = std::path::Path::new(path).file_stem()?;
    (name == "pty-throttle").then(|| std::path::PathBuf::from(path))
}

/// A server of tmux of its own, killed when the value goes, failed or not.
struct Server {
    binary: &'static str,
    socket: String,
}

impl Server {
    fn tmux(&self, args: &[&str]) {
        let status = std::process::Command::new(self.binary)
            .args(["-L", &self.socket])
            .args(args)
            .status()
            .expect("tmux runs");
        assert!(status.success(), "tmux {args:?}");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::process::Command::new(self.binary)
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

/// A console attaching `tmux -CC` behind a line of `BAUD` to a session made
/// beforehand, once `prepare` has run in it.
fn slow_attach(tag: &str, prepare: impl FnOnce(&Server)) -> (Session, Server) {
    let server = Server {
        binary: "tmux",
        socket: format!("zyterm-slow-{}-{tag}", std::process::id()),
    };
    server.tmux(&[
        "-f",
        "/dev/null",
        "new",
        "-d",
        "-s",
        "test",
        "-x",
        "80",
        "-y",
        "24",
        "sh",
    ]);
    prepare(&server);
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session.resize(80, 24);
    session
        .connect_console(&crate::consoles::Console {
            name: "slow tmux".to_string(),
            program: format!(
                "{} --quiet -b {BAUD} -- env ENV= TMUX= tmux -L {} -f /dev/null -CC attach -t test",
                throttle().display(),
                server.socket
            ),
            ..crate::consoles::default_console()
        })
        .expect("the console starts");
    (session, server)
}

/// A console running `tmux -CC` behind a line of `BAUD`, its pane a `sh`.
fn slow_tmux(tag: &str) -> (Session, Server) {
    let binary = "tmux";
    let server = Server {
        binary,
        socket: format!("zyterm-slow-{}-{tag}", std::process::id()),
    };
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session.resize(80, 24);
    session
        .connect_console(
            &crate::consoles::Console {
                name: "slow tmux".to_string(),
                program: format!(
                    "{} --quiet -b {BAUD} -- env ENV= TMUX= {binary} -L {} -f /dev/null -CC new -s test sh",
                    throttle().display(),
                    server.socket
                ),
                ..crate::consoles::default_console()
            })
        .expect("the console starts");
    (session, server)
}

/// Pumps until `done`, for as long as `bytes` take on the line.
fn pump_within(
    session: &mut Session,
    bytes: u64,
    mut done: impl FnMut(&mut Session) -> bool,
) -> bool {
    let deadline = Instant::now() + bound(bytes);
    while Instant::now() < deadline {
        session.pump();
        if done(session) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The snapshot of a pane of 80x24: its modes, a screen of text and the
/// sequences between the lines.
const SNAPSHOT: u64 = 80 * 24 + 1000;

/// At 9600 a window split by another client is two panes in the menu, the
/// second is chosen and shown with nothing sent for it, and what is typed
/// reaches it.
#[cfg(unix)]
#[test]
fn a_pane_is_chosen_and_typed_into_on_a_slow_line() {
    use crate::tmux::Choice;
    let (mut session, server) = slow_tmux("pane");
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_report()
            .is_some_and(|report| report.shown.is_some())),
        "the first pane is shown"
    );
    server.tmux(&["split-window", "-h", "-t", "test", "sh"]);
    server.tmux(&["select-pane", "-t", "test.0"]);
    session.tmux_refresh();
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_menu()
            .and_then(|menu| menu.shown)
            .is_some_and(|shown| shown.windows[0].pane_list.len() == 2)),
        "the two panes in the menu: {:?}",
        session.tmux_menu()
    );
    let menu = session.tmux_menu().expect("the menu of tmux");
    let shown = menu.shown.expect("the session shown");
    let window = &shown.windows[0];
    let second = window.pane_list[1].id;
    session.tmux_choose(Choice::Pane {
        session: shown.id,
        window: window.id,
        pane: second,
    });

    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_report()
            .is_some_and(|report| report.shown == Some(second))),
        "the second pane is shown: {:?}",
        session.tmux_report()
    );
    session.write(b"echo slow-$((2+3))\r");
    assert!(
        pump_within(&mut session, 200, |session| terminal_text(session)
            .contains("slow-5")),
        "typed into the pane shown: {}",
        terminal_text(&mut session)
    );
    session.disconnect();
}

/// At 9600 a bell rung in a window not shown is a mark of that window, and
/// showing the window takes it down.
#[cfg(unix)]
#[test]
fn a_bell_of_a_hidden_window_is_marked_on_a_slow_line() {
    use crate::session::TmuxLook;
    use crate::tmux::{Calling, Choice};
    let (mut session, server) = slow_tmux("bell");
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_report()
            .is_some_and(|report| report.shown.is_some())),
        "the first pane is shown"
    );
    server.tmux(&["new-window", "-d", "-t", "test", "sh"]);
    session.tmux_refresh();
    assert!(pump_within(&mut session, SNAPSHOT, |session| session
        .tmux_menu()
        .and_then(|menu| menu.shown)
        .is_some_and(|shown| shown.windows.len() == 2)));
    server.tmux(&["send-keys", "-t", "test:1", "printf '\\a'", "Enter"]);
    assert!(
        pump_within(&mut session, 400, |session| matches!(
            session.tmux_look(),
            Some(TmuxLook::Shown { calling: 1, .. })
        )),
        "the bell counted: {:?}",
        session.tmux_look()
    );
    let menu = session.tmux_menu().expect("the menu of tmux");
    let shown = menu.shown.expect("the session shown");
    assert_eq!(shown.windows[1].calling, Calling::Bell);
    session.tmux_choose(Choice::Window {
        session: shown.id,
        window: shown.windows[1].id,
    });
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| matches!(
            session.tmux_look(),
            Some(TmuxLook::Shown { calling: 0, .. })
        )),
        "shown, the mark is gone: {:?}",
        session.tmux_look()
    );
    session.disconnect();
}

/// At 9600 the wheel shows each window at once, before a byte of its comes,
/// and tmux is told the one it rests on alone: the window walked past is
/// never made current, so the last window of tmux is the first one.
#[test]
fn the_wheel_walks_the_windows_ahead_of_a_slow_line() {
    use crate::tmux::Choice;
    let (mut session, server) = slow_tmux("wheel");
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_report()
            .is_some_and(|report| report.shown.is_some())),
        "the first pane is shown"
    );
    server.tmux(&["new-window", "-d", "-t", "test:", "sh"]);
    server.tmux(&["new-window", "-d", "-t", "test:", "sh"]);
    session.tmux_refresh();
    assert!(pump_within(&mut session, SNAPSHOT, |session| session
        .tmux_menu()
        .and_then(|menu| menu.shown)
        .is_some_and(|shown| shown.windows.len() == 3)
        && session
            .tmux_report()
            .is_some_and(|report| report.awaiting == 0)));
    let windows: Vec<_> = session.tmux_menu().unwrap().shown.unwrap().windows;
    let first = session.tmux_report().unwrap().shown;

    session.tmux_choose(Choice::Step(1));
    session.tmux_choose(Choice::Step(1));
    assert!(
        pump_within(&mut session, 0, |session| session
            .tmux_menu()
            .is_some_and(|menu| menu.window == Some(windows[2].id))),
        "the third window shown with nothing come from the line"
    );
    assert_ne!(session.tmux_report().unwrap().shown, first);

    session.tmux_choose(Choice::Settle);
    let current = |server: &Server| {
        let out = std::process::Command::new(server.binary)
            .args(["-L", &server.socket, "list-windows", "-t", "test", "-F"])
            .arg("#{window_active}#{window_last_flag}")
            .output()
            .expect("tmux runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(
        pump_within(&mut session, SNAPSHOT, |_| current(&server)
            == "01\n00\n10\n"),
        "the third current, the first the last one: {:?}",
        current(&server)
    );
    session.disconnect();
}

/// At 9600 a window split by another client is drawn side by side: each pane
/// comes to its screen, and a divider let go resizes the pane in tmux.
#[test]
fn the_panes_of_a_window_stand_side_by_side_on_a_slow_line() {
    use crate::tmux::Choice;
    let (mut session, server) = slow_tmux("panes");
    assert!(
        pump_within(&mut session, SNAPSHOT * 2, |session| session
            .tmux_report()
            .is_some_and(|report| report.shown.is_some())),
        "the first pane is shown"
    );
    server.tmux(&["split-window", "-h", "-t", "test", "sh"]);
    assert!(
        pump_within(&mut session, SNAPSHOT * 3, |session| {
            let places = session.tmux_places();
            places.panes.len() == 2 && places.panes.iter().all(|place| !place.waits)
        }),
        "both panes at their screens: {:?}",
        session.tmux_places()
    );
    let divider = session.tmux_places().dividers[0];
    session.tmux_choose(Choice::Resize {
        pane: divider.pane,
        side_by_side: true,
        cells: 20,
    });
    let width = |server: &Server| {
        let out = std::process::Command::new(server.binary)
            .args(["-L", &server.socket, "display", "-p", "-t"])
            .arg(divider.pane.to_string())
            .arg("#{pane_width}")
            .output()
            .expect("tmux runs");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert!(
        pump_within(&mut session, SNAPSHOT, |session| width(&server) == "20"
            && session.tmux_places().dividers[0].at == 20),
        "the pane resized in tmux, and the divider moved here: {}",
        width(&server)
    );
    session.disconnect();
}

/// Whether the scrollback or the screen of what is shown holds the line of a
/// gap.
fn shows_a_gap(session: &mut Session) -> bool {
    let history = session.test_terminal().history_size();
    let mut found = false;
    for offset in [0, history] {
        session.test_terminal().scroll(offset as i32);
        let content = session.test_terminal().content();
        found |= (0..content.rows).any(|row| {
            (0..content.columns).any(|column| {
                content
                    .cell(column, row)
                    .is_some_and(|cell| zyt_term::gap_part(cell.ch).is_some())
            })
        });
        session.test_terminal().scroll_to_bottom();
    }
    found
}

/// The flags tmux holds for the client of this side.
fn client_flags(server: &Server) -> String {
    let out = std::process::Command::new(server.binary)
        .args([
            "-L",
            &server.socket,
            "list-clients",
            "-F",
            "#{client_flags}",
        ])
        .output()
        .expect("tmux runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `tmux.pause` reaches tmux over a line of 9600 and is taken back, while the
/// mode stands. That a pane is then paused is not seen here: tmux ages the
/// output it has sent to its client, and the client of tmux on the far end of
/// the line takes all of it as it comes — 3.3a and 3.8 alike, the window
/// taking or holding the line, a plain client attached or not, the age stayed
/// under a few milliseconds. The pause, `RX` and the gap are the tests of
/// `tmux::tests` and of the window.
#[cfg(unix)]
#[test]
fn pause_after_reaches_tmux_and_is_taken_back_on_a_slow_line() {
    let (mut session, server) = slow_tmux("pause");
    assert!(pump_within(&mut session, SNAPSHOT * 2, |session| session
        .tmux_report()
        .is_some_and(|report| report.shown.is_some())));
    session.set_tmux_pause(Some(3));
    let deadline = Instant::now() + bound(100);
    while !client_flags(&server).contains("pause-after=3") {
        assert!(Instant::now() < deadline, "{}", client_flags(&server));
        session.pump();
        std::thread::sleep(Duration::from_millis(20));
    }
    session.set_tmux_pause(None);
    let deadline = Instant::now() + bound(100);
    while client_flags(&server).contains("pause-after") {
        assert!(Instant::now() < deadline, "{}", client_flags(&server));
        session.pump();
        std::thread::sleep(Duration::from_millis(20));
    }
    session.disconnect();
}

/// With `tmux.pause` off — the default — the pressure of the line holds the
/// program back and nothing is lost: no gap, the end of the output on the
/// screen.
#[cfg(unix)]
#[test]
fn a_flood_with_the_pause_off_loses_nothing_on_a_slow_line() {
    let (mut session, _server) = slow_tmux("pressure");
    assert!(pump_within(&mut session, SNAPSHOT * 2, |session| session
        .tmux_report()
        .is_some_and(|report| report.shown.is_some())));
    session.write(b"seq 1 400; echo held-$((4+4))\r");
    assert!(
        pump_within(&mut session, 400 * 4 * 6, |session| terminal_text(session)
            .contains("held-8")),
        "the end of the output: {}",
        terminal_text(&mut session)
    );
    assert!(terminal_text(&mut session).contains("400"));
    assert!(!shows_a_gap(&mut session), "nothing skipped");
    assert_eq!(session.tmux_lag(), None, "no RX without the pause");
}

/// At 9600 the history tmux held before this client came is offered above
/// the oldest line and loaded a screen at a time when asked.
#[cfg(unix)]
#[test]
fn history_of_tmux_is_loaded_a_screen_at_a_time_on_a_slow_line() {
    let (mut session, _server) = slow_attach("history", |server| {
        server.tmux(&["send-keys", "-t", "test", "seq 1 120", "Enter"]);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let out = std::process::Command::new(server.binary)
                .args([
                    "-L",
                    &server.socket,
                    "display",
                    "-p",
                    "-t",
                    "test",
                    "#{history_size}",
                ])
                .output()
                .expect("tmux runs");
            let lines: u32 = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            if lines > 60 {
                break;
            }
            assert!(Instant::now() < deadline, "the history of the pane");
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    assert!(pump_within(&mut session, SNAPSHOT * 2, |session| session
        .tmux_above_top()
        .is_some()));
    let before = session.test_terminal().history_size();
    session.tmux_load_history();
    assert!(
        pump_within(&mut session, SNAPSHOT * 3, |session| session
            .test_terminal()
            .history_size()
            >= before + 20),
        "a screen of history: {} lines",
        session.test_terminal().history_size()
    );
    session.disconnect();
}

/// At 9600 vim stands on the alternate screen with the keypad of an
/// application; a telnet client that connects then gets both before any byte
/// of the device.
#[cfg(unix)]
#[test]
fn a_client_that_connects_late_gets_the_modes_on_a_slow_line() {
    use std::io::Read as _;

    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session.resize(80, 24);
    session
        .connect_console(&crate::consoles::Console {
            name: "slow vim".to_string(),
            program: format!(
                "{} --quiet -b {BAUD} -- env TERM=xterm vim -u NONE -N -i NONE",
                throttle().display()
            ),
            ..crate::consoles::default_console()
        })
        .expect("the console starts");
    assert!(
        pump_within(&mut session, SNAPSHOT, |session| {
            let modes = session.test_terminal().content().modes;
            modes.alt_screen && modes.app_cursor
        }),
        "vim set its modes"
    );

    let roots = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts")];
    let library = zyt_script::Library::load(&roots);
    let entry = library.find("telnet").expect("the telnet script");
    session
        .start_script(
            entry,
            &roots,
            Direction::Receive,
            Targets::none(),
            &no_values(),
            std::sync::Arc::new(|| {}),
        )
        .expect("the script starts");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port();
    assert!(
        pump_within(&mut session, 0, |session| match session.take_ask() {
            Some(crate::scripts::Waiting::Form(form)) => {
                let text = |text: &str| zyt_script::Value::Text(text.to_string());
                form.answer(Some(std::collections::BTreeMap::from([
                    ("host".to_string(), text("127.0.0.1")),
                    ("port".to_string(), text(&port.to_string())),
                    ("send_state".to_string(), zyt_script::Value::Flag(true)),
                ])));
                true
            }
            _ => false,
        }),
        "the script asks where to listen"
    );

    let deadline = Instant::now() + bound(SNAPSHOT);
    let mut client = loop {
        session.pump();
        if let Ok(client) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            break client;
        }
        assert!(Instant::now() < deadline, "the script listens");
        std::thread::sleep(Duration::from_millis(10));
    };
    client
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("a timeout");
    let mut heard = Vec::new();
    let mut buffer = [0u8; 1024];
    let wanted: [&[u8]; 2] = [b"\x1b[?1049h", b"\x1b[?1h"];
    while !wanted
        .iter()
        .all(|part| heard.windows(part.len()).any(|seen| seen == *part))
    {
        assert!(
            Instant::now() < deadline,
            "the modes came: {:?}",
            String::from_utf8_lossy(&heard)
        );
        session.pump();
        if let Ok(count) = client.read(&mut buffer) {
            heard.extend_from_slice(&buffer[..count]);
        }
    }

    session.cancel_transfer().expect("the script stops");
}
