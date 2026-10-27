use super::*;

const CONFIG: PaneConfig = PaneConfig {
    scrollback: 100,
    clipboard: zyt_term::ClipboardAccess::Disabled,
    pictures: zyt_term::PictureAccess::Drawn,
    overflow: (400, 200),
};

/// A mode that read its opening pair and the session it is on.
fn opened(lossy: bool) -> Control {
    let mut control = Control::new(lossy, CONFIG, (80, 24), 0);
    control.feed(&mut b"%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n".to_vec());
    control
}

fn said(control: &mut Control) -> String {
    String::from_utf8(control.take_out()).expect("commands are text")
}

/// What tmux says in reply to the commands of one line, numbered from
/// `number`.
fn replies(number: u32, bodies: &[&str]) -> Vec<u8> {
    let mut text = String::new();
    for (offset, body) in bodies.iter().enumerate() {
        let number = number + offset as u32;
        text.push_str(&format!("%begin 1 {number} 1\r\n"));
        for line in body.lines() {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text.push_str(&format!("%end 1 {number} 1\r\n"));
    }
    text.into_bytes()
}

const MODES: &str = "20 4 0 0 4294967295 4294967295 2 0 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16";

/// A mode whose pane %0 is shown, holding `$ ls`.
fn showing() -> Control {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    said(&mut control);
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", ""],
    ));
    said(&mut control);
    control.feed(&mut replies(7, &[MODES, "", "$ ls"]));
    control
}

fn shown_text(control: &mut Control) -> String {
    let content = control.shown_mut().expect("a pane is shown").content();
    (0..content.rows)
        .map(|row| {
            (0..content.columns)
                .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

#[test]
fn the_version_is_asked_after_the_opening_pair_and_nothing_before() {
    let mut control = Control::new(false, CONFIG, (80, 24), 0);
    control.feed(&mut b"%begin 1 1 0\r\n".to_vec());
    assert_eq!(said(&mut control), "", "nothing until the pair ended");
    control.feed(&mut b"%end 1 1 0\r\n".to_vec());
    assert_eq!(said(&mut control), "display -p '#{version}'\n");
}

#[test]
fn the_size_and_the_lists_follow_the_version_then_the_snapshot() {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    assert_eq!(
        said(&mut control),
        "refresh-client -C 80x24 ; list-sessions -F '#{session_id} #{session_name}' ; \
         list-windows -a -F '#{session_id} #{window_id} #{window_index} #{window_active} \
         #{window_bell_flag} #{pane_id} #{window_name}' ; list-panes -a -F '#{session_id} \
         #{window_id} #{pane_id} #{window_index} #{pane_index} #{pane_current_command}' ; \
         refresh-client -B 'zyterm-bells:@*:#{window_bell_flag}'\n",
        "the bells subscribed to from 3.2"
    );
    control.feed(&mut replies(
        3,
        &[
            "",
            "$0 zyt",
            "$0 @0 0 0 0 %0 sh\n$0 @1 1 1 0 %4 vim",
            "$0 @1 %4 1 0 vim",
            "",
        ],
    ));
    let snapshot = said(&mut control);
    assert!(
        snapshot.starts_with("display -p -t %4 '#{pane_width} "),
        "{snapshot}"
    );
    assert!(
        snapshot.ends_with(" ; capture-pane -p -e -J -a -q -t %4 ; capture-pane -p -e -J -t %4\n"),
        "{snapshot}"
    );
    assert!(
        control.shown().is_none(),
        "nothing shown before the snapshot ends"
    );
    assert_eq!(control.label().as_deref(), Some("1: vim"));
}

#[test]
fn a_snapshot_shows_the_pane() {
    let mut control = showing();
    assert!(control.take_shown_changed());
    assert_eq!(control.shown().map(Terminal::size), Some((20, 4)));
    assert_eq!(shown_text(&mut control), "$ ls");
}

#[test]
fn output_before_the_end_of_the_snapshot_is_not_fed_and_after_it_is() {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", ""],
    ));
    said(&mut control);
    let mut stream = b"%output %0 before\r\n".to_vec();
    stream.extend(replies(7, &[MODES, "", "$ ls"]));
    stream.extend(b"%output %0 \\015\\012after\r\n");
    control.feed(&mut stream);
    let text = shown_text(&mut control);
    assert!(text.contains("after") && !text.contains("before"), "{text}");
}

#[test]
fn an_older_tmux_is_left() {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["2.9a"]));
    assert_eq!(said(&mut control), "detach\n");
    assert_eq!(control.phase(), Phase::Leaving);
    assert_eq!(control.take_told().len(), 1);
}

#[test]
fn a_start_that_failed_says_why_and_sends_nothing() {
    let mut control = Control::new(false, CONFIG, (80, 24), 0);
    control.feed(&mut b"%begin 1 1 0\r\nno sessions\r\n%error 1 1 0\r\n%exit\r\n".to_vec());
    assert_eq!(said(&mut control), "");
    assert_eq!(control.phase(), Phase::Leaving);
    let told = control.take_told();
    assert!(told[0].contains("no sessions"), "{told:?}");
}

#[test]
fn keys_go_one_send_keys_at_a_time() {
    let mut control = showing();
    said(&mut control);
    control.input(b"ls\r");
    assert_eq!(
        said(&mut control),
        "send-keys -t %0 -l -- 'ls' ; send-keys -t %0 -H 0d\n"
    );
    control.input(b"a");
    control.input(b"b");
    assert_eq!(
        said(&mut control),
        "",
        "held while a send-keys is on its way"
    );
    control.feed(&mut replies(10, &["", ""]));
    assert_eq!(said(&mut control), "send-keys -t %0 -l -- 'ab'\n");
}

#[test]
fn a_key_tmux_knows_goes_by_its_name_in_its_place() {
    let mut control = showing();
    said(&mut control);
    control.key("Home", b"\x1bOH");
    assert_eq!(
        said(&mut control),
        "send-keys -t %0 -- 'Home'\n",
        "tmux encodes it for the pane, not ESC O H"
    );
    control.input(b"a");
    control.key("C-End", b"\x1b[1;5F");
    control.input(b"b");
    assert_eq!(said(&mut control), "", "held behind the one on its way");
    control.feed(&mut replies(10, &[""]));
    assert_eq!(
        said(&mut control),
        "send-keys -t %0 -H 61 ; send-keys -t %0 -- 'C-End' ; send-keys -t %0 -H 62\n"
    );
}

#[test]
fn a_key_held_when_the_mode_ends_is_its_bytes() {
    let mut control = showing();
    said(&mut control);
    control.input(b"x");
    control.key("Home", b"\x1b[H");
    assert_eq!(control.keys_held(), 3);
    assert_eq!(control.take_keys(), b"\x1b[H");
}

#[test]
fn a_paste_goes_in_pieces() {
    let mut control = showing();
    said(&mut control);
    control.input(&vec![b'x'; PIECE * 2 + 10]);
    let first = said(&mut control);
    assert_eq!(first.matches('x').count(), PIECE);
    control.feed(&mut replies(10, &[""]));
    assert_eq!(said(&mut control).matches('x').count(), PIECE);
    control.feed(&mut replies(11, &[""]));
    assert_eq!(said(&mut control).matches('x').count(), 10);
}

#[test]
fn keys_typed_before_a_pane_is_shown_go_to_the_first_one() {
    let mut control = opened(false);
    control.input(b"q");
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", ""],
    ));
    said(&mut control);
    control.feed(&mut replies(7, &[MODES, "", "$"]));
    assert_eq!(said(&mut control), "send-keys -t %0 -H 71\n");
}

#[test]
fn the_size_is_asked_of_tmux_and_the_layout_sizes_the_pane() {
    let mut control = showing();
    said(&mut control);
    control.resize(100, 30);
    assert_eq!(said(&mut control), "refresh-client -C 100x30\n");
    let body = "100x29,0,0,0";
    let sum = zyt_tmux::Layout::parse(format!("{:04x},{body}", checksum(body)).as_bytes());
    assert!(sum.is_ok(), "a layout of the test is one tmux would write");
    let line = format!(
        "%layout-change @0 {:04x},{body} {:04x},{body} *\r\n",
        checksum(body),
        checksum(body)
    );
    control.feed(&mut line.into_bytes());
    assert_eq!(control.shown().map(Terminal::size), Some((100, 29)));
}

fn checksum(text: &str) -> u16 {
    text.bytes().fold(0u16, |sum, byte| {
        sum.rotate_right(1).wrapping_add(u16::from(byte))
    })
}

#[test]
fn an_emulator_answers_nothing_to_the_line() {
    let mut control = showing();
    said(&mut control);
    control.feed(&mut b"%output %0 \\033[c\\033[6n\r\n".to_vec());
    assert_eq!(said(&mut control), "", "tmux already answered the program");
}

#[test]
fn control_mode_inside_a_pane_is_detached() {
    let mut control = showing();
    said(&mut control);
    control.feed(&mut b"%output %0 \\033P1000p%begin 1 1 0\\015\\012\r\n".to_vec());
    assert_eq!(
        said(&mut control),
        "send-keys -t %0 -l -- 'detach' ; send-keys -t %0 -H 0a\n"
    );
    assert_eq!(control.take_told().len(), 1);
    assert!(!shown_text(&mut control).contains("%begin"));
}

#[test]
fn a_snapshot_cut_short_is_asked_again() {
    let mut control = opened(true);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", ""],
    ));
    said(&mut control);
    let mut stream = replies(7, &[MODES, ""]);
    stream.extend(b"%begin 1 9 1\r\n$ ls\r\n%en\xff 1 9 1\r\n%output %0 x\r\n");
    control.feed(&mut stream);
    let again = said(&mut control);
    assert!(again.starts_with("display -p -t %0 "), "{again}");
    control.feed(&mut replies(10, &[MODES, "", "$ ls"]));
    assert_eq!(shown_text(&mut control), "$ ls");
}

#[test]
fn a_byte_dropped_from_the_middle_leaves_the_mode_standing() {
    let mut control = showing();
    said(&mut control);
    control.feed(&mut b"%outp\r\n%output %0 \\015\\012still here\r\n".to_vec());
    assert_eq!(control.phase(), Phase::Control);
    assert!(shown_text(&mut control).contains("still here"));
}

#[test]
fn a_lost_line_forgets_what_was_sent_and_asks_again_when_back() {
    let mut control = showing();
    said(&mut control);
    control.input(b"x");
    said(&mut control);
    control.line_lost();
    control.resize(90, 20);
    assert_eq!(
        said(&mut control),
        "",
        "nothing is written while the line is gone"
    );
    control.line_back();
    let asked = said(&mut control);
    assert!(
        asked.starts_with("refresh-client -C 90x20 ; list-sessions"),
        "{asked}"
    );
    assert!(
        asked.contains("display -p -t %0 "),
        "the pane shown again: {asked}"
    );
}

#[test]
fn a_closed_pane_goes_with_its_terminal() {
    let mut control = showing();
    said(&mut control);
    control.feed(&mut b"%window-close @0\r\n".to_vec());
    assert!(control.shown().is_none());
    assert_eq!(control.report().panes, 0);
}

#[test]
fn versions_read() {
    assert_eq!(version_number("3.3a"), Some((3, 3)));
    assert_eq!(version_number("3.0a"), Some((3, 0)));
    assert_eq!(version_number("next-3.6"), Some((3, 6)));
    assert_eq!(version_number("master"), None);
}

/// A mode on session $0 with windows @0 (pane %0, shown) and @1 (pane %1),
/// and a session $1 with window @2 (pane %2).
fn two_windows_and_two_sessions() -> Control {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    said(&mut control);
    control.feed(&mut replies(
        3,
        &[
            "",
            "$0 zyt\n$1 other",
            "$0 @0 0 1 0 %0 sh\n$0 @1 1 0 0 %1 vim\n$1 @2 0 1 0 %2 make",
            "$0 @0 %0 0 0 sh\n$0 @1 %1 1 0 vim\n$1 @2 %2 0 0 make",
            "",
        ],
    ));
    said(&mut control);
    control.feed(&mut replies(7, &[MODES, "", "$ ls"]));
    said(&mut control);
    control
}

#[test]
fn every_choice_writes_its_commands() {
    let mut control = two_windows_and_two_sessions();
    let cases = [
        (
            Choice::Window {
                session: SessionId(0),
                window: WindowId(1),
            },
            "select-window -t @1\n",
        ),
        (
            Choice::Window {
                session: SessionId(1),
                window: WindowId(2),
            },
            "switch-client -t $1 ; select-window -t @2\n",
        ),
        (Choice::Session(SessionId(1)), "switch-client -t $1\n"),
        (Choice::NewWindow, "new-window -t $0\n"),
        (Choice::CloseWindow(WindowId(1)), "kill-window -t @1\n"),
        (
            Choice::RenameWindow {
                window: WindowId(1),
                name: "logs; tail".to_string(),
            },
            "rename-window -t @1 -- 'logs; tail'\n",
        ),
    ];
    for (choice, line) in cases {
        control.choose(choice.clone());
        let sent = said(&mut control);
        assert!(sent.starts_with(line), "{choice:?}: {sent}");
    }
    control.choose(Choice::Detach);
    assert_eq!(said(&mut control), "detach\n");
    assert_eq!(control.phase(), Phase::Leaving);
    control.choose(Choice::NewWindow);
    assert_eq!(said(&mut control), "", "nothing more once leaving");
}

#[test]
fn the_menu_holds_the_sessions_and_windows_kept() {
    let control = two_windows_and_two_sessions();
    let menu = control.menu();
    let shown = menu.shown.expect("the session shown");
    assert_eq!(shown.name, "zyt");
    let labels: Vec<&str> = shown
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect();
    assert_eq!(labels, ["0: sh", "1: vim"]);
    assert_eq!(menu.window, Some(WindowId(0)));
    assert_eq!(menu.others.len(), 1);
    assert_eq!(menu.others[0].windows[0].label, "0: make");
}

#[test]
fn opening_the_menu_asks_the_lists_and_the_notice_and_their_end_refills_it() {
    let mut control = two_windows_and_two_sessions();
    control.take_lists_changed();
    control.refresh();
    let asked = said(&mut control);
    assert!(asked.starts_with("list-sessions"), "{asked}");
    assert!(asked.ends_with("#{session_attached}'\n"), "{asked}");
    control.feed(&mut replies(
        20,
        &[
            "$0 zyt",
            "$0 @0 0 1 0 %0 sh\n$0 @1 1 0 0 %1 vim\n$0 @3 2 0 0 %4 htop",
            "$0 @0 %0 0 0 sh",
            "3.3a 120x40 2",
        ],
    ));
    assert!(control.take_lists_changed());
    let menu = control.menu();
    assert!(
        menu.others.is_empty(),
        "the session a listing no longer names is gone"
    );
    assert_eq!(menu.shown.unwrap().windows.len(), 3);
    assert_eq!(
        menu.info,
        Some(Info {
            version: "3.3a".into(),
            size: "120x40".into(),
            clients: "2".into()
        })
    );
}

#[test]
fn a_window_chosen_shows_its_pane_at_once_and_its_snapshot_under_it() {
    let mut control = two_windows_and_two_sessions();
    control.take_shown_changed();
    control.feed(&mut b"%output %1 half a screen\r\n".to_vec());
    control.feed(&mut b"%session-window-changed $0 @1\r\n".to_vec());
    let snapshot = said(&mut control);
    assert!(snapshot.starts_with("display -p -t %1 "), "{snapshot}");
    assert_eq!(control.report().shown, Some(PaneId(1)), "shown at once");
    assert!(control.take_shown_changed());
    assert!(control.loading(), "as it is, under the plate");
    assert!(shown_text(&mut control).contains("half a screen"));

    control.feed(&mut b"%output %1 !\r\n".to_vec());
    assert!(
        shown_text(&mut control).contains("half a screen!"),
        "it stands fed"
    );
    control.input(b"x");
    assert_eq!(
        said(&mut control),
        "send-keys -t %1 -H 78\n",
        "keys go before the snapshot ends"
    );

    control.feed(&mut replies(30, &[MODES, "", "$ vim"]));
    assert!(!control.loading());
    assert!(
        shown_text(&mut control).contains("$ vim"),
        "the snapshot in its place"
    );

    control.feed(&mut b"%session-window-changed $0 @0\r\n".to_vec());
    assert_eq!(
        said(&mut control),
        "",
        "a fresh pane is shown with nothing asked"
    );
    assert_eq!(control.report().shown, Some(PaneId(0)));
    assert!(!control.loading());
}

/// A mode whose window @0 holds two panes side by side, %0 shown.
fn two_panes() -> Control {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    said(&mut control);
    control.feed(&mut replies(
        3,
        &[
            "",
            "$0 zyt",
            "$0 @0 0 1 0 %0 sh",
            "$0 @0 %0 0 0 sh\n$0 @0 %1 0 1 bash",
            "",
        ],
    ));
    said(&mut control);
    control.feed(&mut replies(7, &[MODES, "", "$ ls"]));
    control.feed(&mut format!("%layout-change @0 {TWO} {TWO} *\r\n").into_bytes());
    said(&mut control);
    control
}

const TWO: &str = "8205,80x24,0,0{40x24,0,0,0,39x24,41,0,1}";

fn size_of(control: &Control, pane: u32) -> (usize, usize) {
    control.state.panes[&PaneId(pane)].terminal.size()
}

#[test]
fn a_pane_chosen_is_shown_and_nothing_is_sent_for_it() {
    let mut control = two_panes();
    assert_eq!(control.label().as_deref(), Some("0.0 sh"));
    control.choose(Choice::Pane {
        session: SessionId(0),
        window: WindowId(0),
        pane: PaneId(1),
    });
    let sent = said(&mut control);
    assert!(
        sent.starts_with("display -p -t %1 "),
        "its snapshot: {sent}"
    );
    assert!(!sent.contains("select-pane"), "{sent}");
    control.feed(&mut replies(20, &[MODES, "", "$ bash"]));
    assert_eq!(control.report().shown, Some(PaneId(1)));
    assert_eq!(control.label().as_deref(), Some("0.1 bash"));
    assert_eq!(shown_text(&mut control), "$ bash");

    control.choose(Choice::Pane {
        session: SessionId(0),
        window: WindowId(0),
        pane: PaneId(0),
    });
    assert_eq!(said(&mut control), "", "a fresh pane is shown at once");
    assert_eq!(control.report().shown, Some(PaneId(0)));
}

#[test]
fn a_pane_chosen_stands_when_tmux_makes_another_active() {
    let mut control = two_panes();
    control.choose(Choice::Pane {
        session: SessionId(0),
        window: WindowId(0),
        pane: PaneId(1),
    });
    said(&mut control);
    control.feed(&mut replies(20, &[MODES, "", "$ bash"]));
    control.feed(&mut b"%window-pane-changed @0 %0\r\n".to_vec());
    assert_eq!(
        control.report().shown,
        Some(PaneId(1)),
        "a select-pane made before the choice may come after it"
    );
    let one = "b25d,80x24,0,0,0";
    control.feed(&mut format!("%layout-change @0 {one} {one} *\r\n").into_bytes());
    assert_eq!(
        control.report().shown,
        Some(PaneId(0)),
        "the pane chosen closed: the active one"
    );
}

#[test]
fn a_pane_of_another_window_is_chosen_through_its_window() {
    let mut control = two_windows_and_two_sessions();
    control.choose(Choice::Pane {
        session: SessionId(0),
        window: WindowId(1),
        pane: PaneId(1),
    });
    let sent = said(&mut control);
    assert!(sent.starts_with("select-window -t @1\n"), "{sent}");
    assert_eq!(control.report().shown, Some(PaneId(1)), "shown at once");
}

#[test]
fn a_window_chosen_is_shown_ahead_of_tmux_until_tmux_says_it() {
    let mut control = two_windows_and_two_sessions();
    control.choose(Choice::Window {
        session: SessionId(0),
        window: WindowId(1),
    });
    let sent = said(&mut control);
    assert!(
        sent.starts_with("select-window -t @1\ndisplay -p -t %1 "),
        "{sent}"
    );
    assert_eq!(control.report().shown, Some(PaneId(1)), "shown at once");
    assert_eq!(control.label().as_deref(), Some("1: vim"));

    control.feed(&mut b"%session-window-changed $0 @0\r\n".to_vec());
    assert_eq!(
        control.report().shown,
        Some(PaneId(1)),
        "what tmux did before the choice does not take it back"
    );
    control.feed(&mut replies(20, &["", MODES, "", "$ vim"]));
    control.feed(&mut b"%session-window-changed $0 @1\r\n".to_vec());
    assert_eq!(control.report().shown, Some(PaneId(1)));

    control.feed(&mut b"%session-window-changed $0 @0\r\n".to_vec());
    assert_eq!(
        control.report().shown,
        Some(PaneId(0)),
        "with nothing on its way, a window somebody else chose is followed"
    );
}

#[test]
fn a_window_tmux_refuses_gives_the_screen_back_to_what_tmux_said() {
    let mut control = two_windows_and_two_sessions();
    control.choose(Choice::Window {
        session: SessionId(0),
        window: WindowId(1),
    });
    said(&mut control);
    assert_eq!(control.report().shown, Some(PaneId(1)));
    control.feed(&mut b"%begin 1 20 1\r\ncan't find window: @1\r\n%error 1 20 1\r\n".to_vec());
    assert_eq!(control.report().shown, Some(PaneId(0)));
    assert_eq!(control.label().as_deref(), Some("0: sh"));
}

#[test]
fn zoom_by_another_client_is_followed_and_never_asked() {
    let mut control = two_panes();
    assert_eq!(size_of(&control, 0), (40, 24));
    let one = "b25e,80x24,0,0,1";
    control.feed(&mut format!("%layout-change @0 {TWO} {one} *Z\r\n").into_bytes());
    control.choose(Choice::Pane {
        session: SessionId(0),
        window: WindowId(0),
        pane: PaneId(1),
    });
    let sent = said(&mut control);
    assert!(!sent.contains("resize-pane"), "{sent}");
    let zoomed = MODES.replacen("20 4", "80 24", 1);
    control.feed(&mut replies(20, &[&zoomed, "", "$ bash"]));
    assert_eq!(
        size_of(&control, 1),
        (80, 24),
        "the zoomed pane fills its window"
    );
    assert_eq!(
        size_of(&control, 0),
        (40, 24),
        "the hidden one keeps its size"
    );
    control.feed(&mut format!("%layout-change @0 {TWO} {TWO} *\r\n").into_bytes());
    assert_eq!(size_of(&control, 1), (39, 24), "back to its part");
    assert_eq!(said(&mut control), "", "nothing asked of tmux");
}

#[test]
fn the_output_of_a_hidden_pane_makes_its_terminal_and_is_read() {
    let mut control = two_windows_and_two_sessions();
    assert_eq!(control.report().panes, 1, "only the pane shown");
    control.feed(&mut b"%output %1 \\033]2;building\\007done\\015\\012\r\n".to_vec());
    assert_eq!(control.report().panes, 2, "the hidden pane has a terminal");
    assert_eq!(control.report().shown, Some(PaneId(0)), "and is not shown");
    assert_eq!(control.state.panes[&PaneId(1)].state, PaneState::Unseen);
    let hidden = &mut control.state.panes.get_mut(&PaneId(1)).unwrap().terminal;
    assert!(
        hidden
            .take_events()
            .contains(&TerminalEvent::Title("building".to_string())),
        "what it says is read"
    );

    control.feed(
        &mut b"%session-window-changed $0 @1
"
        .to_vec(),
    );
    let snapshot = said(&mut control);
    assert!(
        snapshot.starts_with("display -p -t %1 "),
        "shown, an unseen pane is snapshotted first: {snapshot}"
    );
    control.feed(&mut replies(30, &[MODES, "", "$ vim"]));
    assert_eq!(control.report().shown, Some(PaneId(1)));
    assert_eq!(shown_text(&mut control), "$ vim");
}

#[test]
fn the_output_of_a_pane_the_lists_never_named_makes_nothing() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%output %9 hello\r\n".to_vec());
    assert_eq!(control.report().panes, 1);
}

#[test]
fn a_session_left_drops_the_terminals_of_its_panes() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%session-window-changed $0 @1\r\n".to_vec());
    said(&mut control);
    control.feed(&mut replies(30, &[MODES, "", "$ vim"]));
    assert_eq!(control.report().panes, 2);
    control.feed(&mut b"%session-changed $1 other\r\n".to_vec());
    let snapshot = said(&mut control);
    assert!(snapshot.starts_with("display -p -t %2 "), "{snapshot}");
    control.feed(&mut replies(33, &[MODES, "", "$ make"]));
    assert_eq!(control.report().shown, Some(PaneId(2)));
    assert_eq!(
        control.report().panes,
        1,
        "only the pane of the session shown"
    );
}

#[test]
fn sessions_changed_asks_the_lists_again() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%sessions-changed\r\n%sessions-changed\r\n".to_vec());
    let asked = said(&mut control);
    assert_eq!(asked.matches("list-sessions").count(), 1, "{asked}");
}

#[test]
fn a_pane_larger_than_the_limit_is_cut_and_given_back_with_the_area() {
    let mut control = two_windows_and_two_sessions();
    let body = "5000x100,0,0,0";
    let line = format!(
        "%layout-change @0 {:04x},{body} {:04x},{body} *\r\n",
        checksum(body),
        checksum(body)
    );
    control.feed(&mut line.into_bytes());
    assert_eq!(
        control.shown().map(Terminal::size),
        Some((480, 100)),
        "80 + 400 columns"
    );
    control.resize(200, 50);
    said(&mut control);
    assert_eq!(control.shown().map(Terminal::size), Some((600, 100)));
    control.set_config(PaneConfig {
        overflow: (0, 0),
        ..CONFIG
    });
    assert_eq!(
        control.shown().map(Terminal::size),
        Some((200, 50)),
        "held to the area"
    );
}

fn calling_of(control: &Control, window: u32) -> Calling {
    control
        .menu()
        .shown
        .expect("the session shown")
        .windows
        .into_iter()
        .find(|entry| entry.id == WindowId(window))
        .expect("the window")
        .calling
}

#[test]
fn a_bell_of_a_hidden_window_marks_it_until_it_is_shown() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%output %1 \\007\r\n".to_vec());
    assert!(
        control.take_events().contains(&TerminalEvent::Bell),
        "the bell goes on, for the attention of the desktop"
    );
    assert_eq!(calling_of(&control, 1), Calling::Bell);
    assert_eq!(calling_of(&control, 0), Calling::None);
    assert_eq!(control.calling(), 1);

    control.feed(&mut b"%session-window-changed $0 @1\r\n".to_vec());
    said(&mut control);
    control.feed(&mut replies(30, &[MODES, "", "$ vim"]));
    assert_eq!(control.report().shown, Some(PaneId(1)));
    assert_eq!(calling_of(&control, 1), Calling::None, "seen");
    assert_eq!(control.calling(), 0);
}

#[test]
fn a_notification_of_a_hidden_window_is_its_mark_and_names_it() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%output %1 \\033]9;built\\007\r\n".to_vec());
    let events = control.take_events();
    let named = events.iter().find_map(|event| match event {
        TerminalEvent::Notification { title, body, .. } => Some((title.clone(), body.clone())),
        _ => None,
    });
    assert_eq!(
        named,
        Some(("1: vim".to_string(), "built".to_string())),
        "the window named in the heading of the desktop"
    );
    assert!(matches!(
        calling_of(&control, 1),
        Calling::Note { ref body, .. } if body == "built"
    ));
}

#[test]
fn the_bell_flag_of_tmux_comes_with_the_lists_and_the_subscription() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%subscription-changed zyterm-bells $0 @1 1 - : 1\r\n".to_vec());
    assert_eq!(calling_of(&control, 1), Calling::Bell);
    control.feed(&mut b"%subscription-changed zyterm-bells $0 @1 1 - : 0\r\n".to_vec());
    assert_eq!(calling_of(&control, 1), Calling::None, "taken down by tmux");

    control.refresh();
    said(&mut control);
    control.feed(&mut replies(
        40,
        &[
            "$0 zyt\n$1 other",
            "$0 @0 0 1 0 %0 sh\n$0 @1 1 0 0 %1 vim\n$1 @2 0 1 1 %2 make",
            "$0 @0 %0 0 0 sh\n$0 @1 %1 1 0 vim\n$1 @2 %2 0 0 make",
            "3.3a 80x24 1",
        ],
    ));
    let other = control.menu().others.remove(0);
    assert!(other.calling, "a window of another session rang");
}

#[test]
fn an_older_tmux_subscribes_to_nothing() {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.0a"]));
    let start = said(&mut control);
    assert!(start.contains("list-panes"), "{start}");
    assert!(!start.contains("refresh-client -B"), "{start}");
}

/// A mode on 3.3a with `pause-after` asked from its start, %0 shown.
fn paused_after(seconds: u32) -> Control {
    let mut control = opened(false);
    control.set_pause(Some(seconds));
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    let start = said(&mut control);
    assert!(
        start.ends_with(&format!(" ; refresh-client -f pause-after={seconds}\n")),
        "{start}"
    );
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", "", ""],
    ));
    said(&mut control);
    control.feed(&mut replies(9, &[MODES, "", "$ before"]));
    said(&mut control);
    control
}

#[test]
fn pause_after_is_asked_and_taken_back_while_the_mode_stands() {
    let mut control = showing();
    said(&mut control);
    control.set_pause(Some(5));
    assert_eq!(said(&mut control), "refresh-client -f pause-after=5\n");
    control.set_pause(Some(5));
    assert_eq!(said(&mut control), "", "told once");
    control.set_pause(None);
    assert_eq!(said(&mut control), "refresh-client -f '!pause-after'\n");

    let mut older = opened(false);
    older.set_pause(Some(5));
    said(&mut older);
    older.feed(&mut replies(2, &["3.0a"]));
    assert!(!said(&mut older).contains("pause-after"), "before 3.2");
}

#[test]
fn the_age_of_the_output_shown_is_the_lag_of_rx() {
    let mut control = paused_after(5);
    assert_eq!(control.lag(), None);
    control.feed(&mut b"%extended-output %0 2500 : x\r\n".to_vec());
    assert_eq!(control.lag(), Some((Duration::from_millis(2500), false)));
    control.feed(&mut b"%extended-output %0 300 : y\r\n".to_vec());
    assert_eq!(control.lag(), None, "back under a second");
}

#[test]
fn the_pane_shown_paused_is_let_go_under_a_gap() {
    let mut control = paused_after(5);
    control.feed(&mut b"%extended-output %0 6000 : z\r\n".to_vec());
    control.feed(&mut b"%pause %0\r\n".to_vec());
    let sent = said(&mut control);
    assert!(
        sent.starts_with("refresh-client -A '%0:continue' ; display -p -t %0 "),
        "{sent}"
    );
    assert_eq!(
        control.lag().map(|(_, lost)| lost),
        Some(true),
        "red: output was thrown away"
    );
    control.feed(&mut replies(20, &["", MODES, "", "$ now"]));
    assert_eq!(shown_text(&mut control), "$ now");
    let terminal = control.shown_mut().expect("the pane shown");
    terminal.scroll(1);
    let content = terminal.content();
    let gap: String = (0..content.columns)
        .filter_map(|column| content.cell(column, 0).map(|cell| cell.ch))
        .collect();
    assert!(gap.contains('6'), "the gap says how much: {gap:?}");
    assert_eq!(
        zyt_term::gap_part(gap.chars().next().unwrap()),
        Some(zyt_term::GapPart::Rule)
    );
    control.feed(&mut b"%extended-output %0 100 : fresh\r\n".to_vec());
    assert_eq!(control.lag(), None, "RX goes once the screen is current");
}

#[test]
fn a_hidden_pane_paused_stays_so_until_it_is_shown() {
    let mut control = two_windows_and_two_sessions();
    control.feed(&mut b"%output %1 x\r\n".to_vec());
    control.feed(&mut b"%pause %1\r\n".to_vec());
    assert_eq!(said(&mut control), "", "nothing sent for a hidden pane");
    control.feed(&mut b"%session-window-changed $0 @1\r\n".to_vec());
    let sent = said(&mut control);
    assert!(
        sent.starts_with("refresh-client -A '%1:continue' ; display -p -t %1 "),
        "shown: let go and snapshotted: {sent}"
    );
}

#[test]
fn rx_pressed_throws_away_what_piled_up() {
    let mut control = paused_after(5);
    control.jump_to_now();
    assert_eq!(said(&mut control), "refresh-client -A '%0:pause'\n");
}

/// The reply of `display -p` for a pane of 20x4 holding `history` lines of
/// history, on the alternate screen or not.
fn modes_with(history: u32, alternate: bool) -> String {
    format!(
        "20 4 {history} {} 4294967295 4294967295 2 0 0 3 0 1 0 0 0 1 0 0 0 0 0 8,16",
        u8::from(alternate)
    )
}

/// A mode whose pane %0, 20x4, is shown with `history` lines of tmux above it.
fn showing_with_history(history: u32, alternate: bool) -> Control {
    let mut control = opened(false);
    said(&mut control);
    control.feed(&mut replies(2, &["3.3a"]));
    said(&mut control);
    control.feed(&mut replies(
        3,
        &["", "$0 zyt", "$0 @0 0 1 0 %0 sh", "$0 @0 %0 0 0 sh", ""],
    ));
    said(&mut control);
    control.feed(&mut replies(
        8,
        &[&modes_with(history, alternate), "", "s1\ns2\ns3\n$ "],
    ));
    said(&mut control);
    control
}

#[test]
fn history_above_is_offered_a_screen_at_a_time_and_loaded_deeper() {
    let mut control = showing_with_history(6, false);
    assert_eq!(
        control.above_top(),
        Some(t!("tmux.load_history", lines = 4).to_string()),
        "a screen, the rows of the pane"
    );
    control.load_history();
    let asked = said(&mut control);
    assert!(
        asked.starts_with("display -p -t %0 ") && !asked.contains("capture-pane"),
        "the modes first, alone: {asked}"
    );
    assert_eq!(
        control.above_top(),
        Some(t!("tmux.loading_history").to_string())
    );
    control.feed(&mut replies(20, &[&modes_with(6, false)]));
    assert_eq!(said(&mut control), "capture-pane -p -e -J -S -4 -t %0\n");
    assert_eq!(
        shown_text(&mut control),
        "s1\ns2\ns3\n$",
        "the screen stands meanwhile"
    );
    control.feed(&mut replies(21, &["h3\nh4\nh5\nh6\ns1\ns2\ns3\n$ "]));
    assert_eq!(shown_text(&mut control), "s1\ns2\ns3\n$");
    assert_eq!(
        control.shown().unwrap().history_size(),
        4,
        "a screen of history"
    );
    assert_eq!(
        control.above_top(),
        Some(t!("tmux.load_history", lines = 2).to_string()),
        "what is left above"
    );
}

#[test]
fn history_on_the_alternate_screen_comes_apart_from_it() {
    let mut control = showing_with_history(6, true);
    control.load_history();
    said(&mut control);
    control.feed(&mut replies(20, &[&modes_with(6, true)]));
    assert_eq!(
        said(&mut control),
        "capture-pane -p -e -J -S -4 -E -1 -t %0 ; capture-pane -p -e -J -a -q -t %0 ; \
         capture-pane -p -e -J -t %0\n"
    );
}

#[test]
fn nothing_above_is_no_row() {
    let control = showing_with_history(0, false);
    assert_eq!(control.above_top(), None);
}
