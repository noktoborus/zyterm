//! tmux control mode in a session: with the fake device of the tests of
//! transfers, and with a console running a real tmux of every version on the
//! path.

use super::transfer_tests::{connected, pump_until, terminal_text};
use super::*;
use crate::session::mode::TmuxLook;

/// What the device was written since the last call, as text.
fn written(device: &Arc<std::sync::Mutex<super::transfer_tests::Device>>) -> String {
    let bytes = std::mem::take(&mut device.lock().unwrap().written);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn device_says(device: &Arc<std::sync::Mutex<super::transfer_tests::Device>>, bytes: &[u8]) {
    device.lock().unwrap().incoming.extend_from_slice(bytes);
}

#[test]
fn a_start_that_is_no_tmux_is_given_back_as_text() {
    let (mut session, device) = connected();
    written(&device);
    device_says(&device, b"before\x1bP1000phello\r\nafter\r\n");
    assert!(pump_until(&mut session, |session| {
        terminal_text(session).contains("after")
    }));
    let text = terminal_text(&mut session);
    assert!(text.contains("before") && text.contains("hello"), "{text}");
    assert!(session.tmux_look().is_none(), "the mode is not standing");
    assert_eq!(written(&device), "", "nothing was written for it");
}

#[test]
fn input_while_entering_is_held_and_given_back_with_the_start() {
    let (mut session, device) = connected();
    written(&device);
    device_says(&device, b"\x1bP1000p");
    assert!(pump_until(&mut session, |session| session
        .tmux_look()
        .is_some()));
    session.write(b"ls\r");
    assert_eq!(
        session.tmux_look(),
        Some(TmuxLook::Waiting {
            held: 3,
            leaving: false
        })
    );
    assert_eq!(written(&device), "");
    session.press_tmux();
    assert!(session.tmux_look().is_none());
    assert!(pump_until(&mut session, |_| device.lock().unwrap().written == b"ls\r"));
}

#[test]
fn the_mode_starts_on_its_first_line_and_writes_only_commands() {
    let (mut session, device) = connected();
    session.terminal_mut().feed(b"the plain terminal\r\n");
    written(&device);
    device_says(
        &device,
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n",
    );
    assert!(pump_until(&mut session, |_| {
        device
            .lock()
            .unwrap()
            .written
            .ends_with(b"display -p '#{version}'\n")
    }));
    assert!(
        terminal_text(&mut session).contains("the plain terminal"),
        "the plain terminal stands until a pane is shown"
    );
    assert!(matches!(session.tmux_look(), Some(TmuxLook::Shown { .. })));
    assert!(session.start_script_refused());
}

/// Whether the device was written `wanted`, since it was last emptied.
fn has_written(
    device: &Arc<std::sync::Mutex<super::transfer_tests::Device>>,
    wanted: &str,
) -> bool {
    device
        .lock()
        .unwrap()
        .written
        .windows(wanted.len())
        .any(|window| window == wanted.as_bytes())
}

/// What tmux says in reply to the commands of one line.
fn replies(bodies: &[&str]) -> Vec<u8> {
    let mut text = String::new();
    for (number, body) in bodies.iter().enumerate() {
        text.push_str(&format!("%begin 1 {} 1\r\n", number + 50));
        for line in body.lines() {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text.push_str(&format!("%end 1 {} 1\r\n", number + 50));
    }
    text.into_bytes()
}

/// A selection in the pane shown holds the line, as it does in the plain
/// terminal: what is selected does not move under the pointer.
#[test]
fn a_selection_in_the_pane_shown_holds_the_line() {
    let (mut session, device) = connected();
    device_says(
        &device,
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n",
    );
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "display -p '#{version}'"
    )));
    device_says(&device, &replies(&["3.0a"]));
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "list-panes"
    )));
    device_says(
        &device,
        &replies(&["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh"]),
    );
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "capture-pane"
    )));
    device_says(
        &device,
        &replies(&[
            "80 24 0 0 4294967295 4294967295 2 0 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16",
            "",
            "$ ls",
        ]),
    );
    assert!(pump_until(&mut session, |session| session
        .tmux_report()
        .is_some_and(|report| report.shown.is_some())));
    assert!(!session.held_for_selection());
    session
        .terminal_mut()
        .selection_start(zyt_term::SelectionKind::Simple, 0, 0)
        .expect("a cell of the pane");
    assert!(session.held_for_selection(), "the pane's selection holds");
}

/// Over a port, where a byte may be lost, a reply whose `%end` was lost is
/// closed by the notification that comes inside it, and the snapshot it was is
/// asked again.
#[test]
fn a_snapshot_cut_short_on_a_port_is_asked_again() {
    let (mut session, device) = connected();
    device_says(
        &device,
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n",
    );
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "display -p '#{version}'"
    )));
    device_says(&device, &replies(&["3.0a"]));
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "list-panes"
    )));
    device_says(
        &device,
        &replies(&["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh"]),
    );
    assert!(pump_until(&mut session, |_| has_written(
        &device,
        "capture-pane"
    )));
    written(&device);
    let modes = "80 24 0 0 4294967295 4294967295 2 0 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16";
    let mut cut = replies(&[modes, ""]);
    cut.extend_from_slice(b"%begin 1 52 1\r\n$ ls\r\n%output %0 x\r\n");
    device_says(&device, &cut);
    assert!(
        pump_until(&mut session, |_| has_written(&device, "display -p -t %0 ")),
        "the snapshot asked again: {}",
        written(&device)
    );
    assert!(session.tmux_look().is_some(), "the mode stands");
}

#[test]
fn a_disconnect_drops_the_mode_and_writes_nothing() {
    let (mut session, device) = connected();
    device_says(&device, b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n");
    assert!(pump_until(&mut session, |_| {
        device
            .lock()
            .unwrap()
            .written
            .ends_with(b"display -p '#{version}'\n")
    }));
    written(&device);
    session.disconnect();
    assert!(session.tmux_look().is_none());
    assert!(!session.tmux_sets_size());
    assert_eq!(written(&device), "");
}

impl Session {
    /// Whether a script would be refused now, for a test that has no script.
    fn start_script_refused(&self) -> bool {
        self.tmux.is_some() || self.entering.is_some()
    }
}

/// The versions of tmux on the path: the container builds five.
#[cfg(unix)]
fn installed() -> Vec<&'static str> {
    ["tmux", "tmux-3.0a", "tmux-3.2a", "tmux-3.4", "tmux-3.8"]
        .into_iter()
        .filter(|binary| {
            std::process::Command::new(binary)
                .arg("-V")
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .collect()
}

/// A server of tmux of its own, killed when the value goes, failed or not.
#[cfg(unix)]
struct Server {
    binary: &'static str,
    socket: String,
}

#[cfg(unix)]
impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::process::Command::new(self.binary)
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

/// A session with a console that runs `tmux -CC` on a server of its own, its
/// pane a `sh`: a prompt of `$ `, or `# ` for root.
#[cfg(unix)]
fn console_with_tmux(binary: &'static str, tag: &str) -> (Session, Server) {
    let socket = format!("zyterm-test-{}-{tag}", std::process::id());
    let server = Server { binary, socket };
    let mut session = Session::new(1000).expect("session is created");
    session.resize(80, 24);
    session
        .connect_console(
            &crate::consoles::Console {
                name: "tmux".to_string(),
                program: format!(
                    "env ENV= TMUX= {binary} -L {} -f /dev/null -CC new -s test sh",
                    server.socket
                ),
                ..crate::consoles::default_console()
            },
            None,
        )
        .expect("the console starts");
    (session, server)
}

/// Pumps until `done`, for as long as a slow tmux may take.
#[cfg(unix)]
fn pump_for(session: &mut Session, mut done: impl FnMut(&mut Session) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        session.pump();
        if done(session) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// A console running `tmux -CC` shows its active pane, takes input, follows
/// the size and leaves cleanly, on every version on the path.
#[cfg(unix)]
#[test]
fn a_console_running_tmux_shows_its_pane_and_leaves_cleanly() {
    let versions = installed();
    for binary in versions {
        eprintln!("tmux control mode against {binary}");
        let (mut session, _server) = console_with_tmux(binary, binary);
        session.terminal_mut().feed(b"before tmux\r\n");
        assert!(
            pump_for(&mut session, |session| {
                let text = terminal_text(session);
                matches!(
                    session.tmux_look(),
                    Some(TmuxLook::Shown {
                        window: Some(_),
                        ..
                    })
                ) && (text.contains('$') || text.contains('#'))
            }),
            "{binary}: the pane is shown: {:?} {}",
            session.tmux_report(),
            terminal_text(&mut session)
        );

        session.write(b"echo zyt-$((40+2))\r");
        assert!(
            pump_for(&mut session, |session| terminal_text(session)
                .contains("zyt-42")),
            "{binary}: input reaches the pane: {}",
            terminal_text(&mut session)
        );

        session.resize(100, 30);
        assert!(
            pump_for(&mut session, |session| session.terminal().size().0 == 100),
            "{binary}: the size follows: {:?}",
            session.terminal().size()
        );

        session.set_tmux_control(false);
        assert!(
            pump_for(&mut session, |session| session.tmux_look().is_none()),
            "{binary}: detached: {:?}",
            session.tmux_report()
        );
        let text = terminal_text(&mut session);
        assert!(
            text.contains("before tmux") && !text.contains("zyt-42"),
            "{binary}: the plain terminal is back with what it held: {text}"
        );
        assert_eq!(
            session.terminal().size(),
            (100, 30),
            "{binary}: at the size of the area"
        );
        session.disconnect();
    }
}

#[test]
fn with_the_switch_off_the_start_reaches_the_terminal_and_nothing_is_written() {
    let (mut session, device) = connected();
    session.set_tmux_control(false);
    written(&device);
    device_says(&device, b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n");
    assert!(pump_until(&mut session, |_| device
        .lock()
        .unwrap()
        .incoming
        .is_empty()));
    session.pump();
    assert!(session.tmux_look().is_none());
    assert_eq!(written(&device), "");
}

/// The choices of the menu against a real tmux: a new window is shown, and
/// the first one comes back at once from the terminal it kept.
#[cfg(unix)]
#[test]
fn a_console_running_tmux_switches_windows_from_the_menu() {
    use crate::tmux::Choice;
    for binary in installed() {
        let (mut session, _server) = console_with_tmux(binary, &format!("{binary}-menu"));
        assert!(
            pump_for(&mut session, |session| session
                .tmux_report()
                .is_some_and(|report| report.shown.is_some())),
            "{binary}: the first pane is shown"
        );
        let first = session.tmux_report().and_then(|report| report.shown);
        session.write(b"echo first-$((1+1))\r");
        assert!(pump_for(&mut session, |session| terminal_text(session)
            .contains("first-2")));

        session.tmux_choose(Choice::NewWindow);
        assert!(
            pump_for(&mut session, |session| {
                let report = session.tmux_report();
                report
                    .as_ref()
                    .is_some_and(|report| report.shown.is_some() && report.shown != first)
                    && !terminal_text(session).contains("first-2")
            }),
            "{binary}: the new window is shown: {:?}",
            session.tmux_report()
        );

        session.take_tmux_lists_changed();
        session.tmux_refresh();
        assert!(pump_for(&mut session, |session| {
            session.tmux_menu().is_some_and(|menu| menu.info.is_some())
        }));
        let menu = session.tmux_menu().expect("the menu of tmux");
        let windows = menu
            .shown
            .as_ref()
            .expect("the session shown")
            .windows
            .clone();
        assert_eq!(windows.len(), 2, "{binary}: two windows listed");
        assert!(menu.info.is_some(), "{binary}: the notice came");

        let session_id = menu.shown.expect("the session shown").id;
        session.tmux_choose(Choice::Window {
            session: session_id,
            window: windows[0].id,
        });
        assert!(
            pump_for(&mut session, |session| terminal_text(session)
                .contains("first-2")),
            "{binary}: the first window is back with what it held"
        );
        assert_eq!(session.tmux_report().and_then(|report| report.shown), first);
        session.disconnect();
    }
}

/// The history a session of tmux held before this client came is offered
/// above the oldest line and loaded a screen at a time, on every version on
/// the path.
#[cfg(unix)]
#[test]
fn history_held_before_the_attach_is_loaded_on_every_version() {
    for binary in installed() {
        let socket = format!("zyterm-test-{}-{binary}-history", std::process::id());
        let server = Server {
            binary,
            socket: socket.clone(),
        };
        let tmux = |args: &[&str]| {
            std::process::Command::new(binary)
                .args(["-L", &socket])
                .args(args)
                .output()
                .expect("tmux runs")
        };
        tmux(&[
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
        tmux(&["send-keys", "-t", "test", "seq 1 120", "Enter"]);
        let deadline = Instant::now() + Duration::from_secs(5);
        while String::from_utf8_lossy(
            &tmux(&["display", "-p", "-t", "test", "#{history_size}"]).stdout,
        )
        .trim()
        .parse::<u32>()
        .unwrap_or(0)
            < 60
        {
            assert!(
                Instant::now() < deadline,
                "{binary}: the history of the pane"
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        let mut session = Session::new(1000).expect("session is created");
        session.resize(80, 24);
        session
            .connect_console(
                &crate::consoles::Console {
                    name: "tmux".to_string(),
                    program: format!(
                        "env ENV= TMUX= {binary} -L {socket} -f /dev/null -CC attach -t test"
                    ),
                    ..crate::consoles::default_console()
                },
                None,
            )
            .expect("the console starts");
        assert!(
            pump_for(&mut session, |session| session.tmux_above_top().is_some()),
            "{binary}: the history offered"
        );
        let before = session.terminal().history_size();
        session.tmux_load_history();
        assert!(
            pump_for(&mut session, |session| session.terminal().history_size()
                >= before + 20),
            "{binary}: a screen of history, {} lines",
            session.terminal().history_size()
        );
        session.disconnect();
        drop(server);
    }
}
