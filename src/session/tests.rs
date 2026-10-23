//! The session with a console and with no source.

use super::*;
use std::time::Duration;
use zyt_script::Targets;
use zyt_term::Color;

fn row_text(content: &zyt_term::RenderableContent, row: usize) -> String {
    (0..content.columns)
        .filter_map(|column| content.cell(column, row).map(|cell| cell.ch))
        .collect()
}

/// The widget only says what size the area holds; the session is what
/// gives the grid that size.
#[test]
fn a_resize_gives_the_grid_its_size() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session.resize(20, 4);

    assert_eq!(session.test_terminal().size(), (20, 4));
}

#[test]
fn a_notice_does_not_pull_a_reader_back_to_the_end() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    for line in 0..40 {
        session
            .test_terminal()
            .feed(format!("line{line}\r\n").as_bytes());
    }

    session.test_terminal().scroll(5);
    assert!(session.test_terminal().display_offset() > 0);
    session.test_terminal().feed(&crate::scripts::notice_line(
        zyt_script::NoticeKind::Info,
        "something happened",
    ));
    assert!(
        session.test_terminal().display_offset() > 0,
        "the view stays in the history"
    );

    session.test_terminal().scroll_to_bottom();
    assert_eq!(session.test_terminal().display_offset(), 0);
    session.test_terminal().feed(&crate::scripts::notice_line(
        zyt_script::NoticeKind::Info,
        "and again",
    ));
    assert_eq!(
        session.test_terminal().display_offset(),
        0,
        "at the end, the end is followed"
    );
}

/// A share stands for as long as the program reports one, and the report
/// that says it is done leaves nothing standing: the bar of the status bar
/// and the button of the taskbar both go by whether there is a share at
/// all, so `Removed` is no report rather than a report of its own.
#[test]
fn a_share_stands_until_the_program_says_it_is_done() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session.test_terminal().feed(b"\x1b]9;4;1;40\x07");
    session.pump();
    assert_eq!(
        session.terminal_progress(),
        Some(zyt_term::ProgressState::Set(40))
    );

    session.test_terminal().feed(b"\x1b]9;4;0\x07");
    session.pump();
    assert_eq!(
        session.terminal_progress(),
        None,
        "the report that it is done takes the share away"
    );
}

/// Clearing the screen clears what the program said about its progress,
/// which is `AppCommand::Clear` reaching this.
#[test]
fn clearing_the_screen_takes_the_share_with_it() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session.test_terminal().feed(b"\x1b]9;4;1;70\x07");
    session.pump();
    assert_eq!(
        session.terminal_progress(),
        Some(zyt_term::ProgressState::Set(70))
    );

    session.clear_reports();
    assert_eq!(session.terminal_progress(), None);
}

/// The times of the plate are readings of output that is gone, so they go
/// with it: what is left standing would measure a screen nobody can look
/// at any more.
#[test]
fn clearing_the_screen_takes_the_times_with_it() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session.write(b"a command\r");
    session.settle();
    // Nothing is connected here, so the answer is put where a read would
    // have put it: what is under test is what clearing does with the times
    // and not the path they came by.
    session.counters.last_data = Some(Moment::now());
    session.counters.first_data = session.counters.last_data;
    assert!(
        session.counters.last_written.is_some(),
        "something was sent"
    );

    session.clear_reports();
    assert!(session.counters.last_written.is_none());
    assert!(session.counters.first_data.is_none());
    assert!(session.counters.last_data.is_none());
}

/// The pictures go with the screen, and the memory they held with them.
#[test]
fn clearing_the_screen_takes_the_pictures_with_it() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session
        .test_terminal()
        .feed(b"\x1bPq\"1;1;64;36#0;2;100;0;0#0~~~~$-\x1b\\");
    assert_eq!(
        session.test_terminal().pictures_held().0,
        1,
        "the picture is kept"
    );

    session.clear_screen();
    let (pictures, held, _) = session.test_terminal().pictures_held();
    assert_eq!((pictures, held), (0, 0), "and gone with the screen");
}

/// The cursor a program set goes with the screen: one that was hidden is
/// shown again and one that was asked for as a beam is the block this
/// terminal starts with.
#[test]
fn clearing_the_screen_puts_the_cursor_back() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session.test_terminal().feed(b"\x1b[?25l\x1b[5 q");
    session.pump();
    assert_eq!(
        session.test_terminal().content().cursor,
        None,
        "the program hid the cursor"
    );

    session.clear_screen();
    session.pump();
    let cursor = session
        .test_terminal()
        .content()
        .cursor
        .expect("the cursor is shown again");
    assert_eq!(cursor.shape, zyt_term::CursorShape::Block);
}

/// Everything a program dressed the window in goes with the screen: the
/// title it set and the colors it painted, not only the share.
#[test]
fn clearing_the_screen_takes_the_title_and_the_colors_with_it() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");

    session
        .test_terminal()
        .feed(b"\x1b]0;a title\x07\x1b]11;#102030\x07");
    session.pump();
    assert_eq!(session.title.as_deref(), Some("a title"));
    assert!(
        session.test_terminal().content().background.is_some(),
        "the program painted the background"
    );

    session.clear_reports();
    session.settle();
    assert_eq!(session.title, None, "the title is the program's again");
    assert_eq!(
        session.test_terminal().content().background,
        None,
        "the background is the palette of the settings again"
    );
}

#[test]
fn progress_lines_are_marked_and_readable() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session.test_terminal().feed(&crate::scripts::notice_line(
        zyt_script::NoticeKind::Info,
        "Bytes Sent: 1024/4096",
    ));

    let content = session.test_terminal().content();
    let row = (0..content.rows)
        .find(|row| row_text(&content, *row).starts_with('│'))
        .expect("the line is marked");
    assert!(row_text(&content, row).contains("Bytes Sent: 1024/4096"));
    assert_eq!(
        content.cell(0, row).expect("marker cell exists").fg,
        Color::Palette(6)
    );
    assert_eq!(
        content.cell(2, row).expect("text cell exists").fg,
        Color::Foreground
    );
}

#[test]
fn a_second_script_is_refused_while_one_holds_the_line() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session
        .connect_console(&crate::consoles::Console {
            name: "test".to_string(),
            program: "cat".to_string(),
            ..crate::consoles::default_console()
        })
        .expect("the local console starts");

    start_fixture(&mut session, "holds", Targets::none()).expect("the first one starts");
    assert!(matches!(
        start_fixture(&mut session, "holds", Targets::none()),
        Err(AppError::TransferRunning)
    ));

    session.cancel_transfer().expect("the script stops");
}

#[test]
fn a_script_is_known_by_name_and_takes_the_line() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session
        .connect_console(&crate::consoles::Console {
            name: "test".to_string(),
            program: "cat".to_string(),
            ..crate::consoles::default_console()
        })
        .expect("the local console starts");

    start_fixture(&mut session, "holds", Targets::none()).expect("the script starts");

    assert!(session.is_transferring());
    assert_eq!(
        session.script_name(),
        Some("Holds the line"),
        "what is said about a run is the name a person reads"
    );

    let told = session.take_told();
    assert!(
        told.is_empty(),
        "the start is told by the plate of the run and not on the desktop: {told:?}"
    );
    let content = session.test_terminal().content();
    let text: String = (0..content.rows)
        .map(|row| row_text(&content, row))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        !text.contains("Holds the line"),
        "nothing of it is printed into the terminal: {text}"
    );

    session.cancel_transfer().expect("the script stops");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline && session.is_transferring() {
        session.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!session.is_transferring());
}

/// A run on a console gives the grid the size of a far terminal, held to
/// the area and the overflow past it; the window does not size it while the
/// run holds the line, and the end of the run gives the size back.
#[test]
fn a_run_sizes_the_grid_and_gives_the_size_back() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session
        .connect_console(&crate::consoles::Console {
            name: "test".to_string(),
            program: "cat".to_string(),
            ..crate::consoles::default_console()
        })
        .expect("the local console starts");
    session.set_screen_overflow((40, 10));
    let size = |session: &mut Session| {
        let content = session.test_terminal().content();
        (content.columns, content.rows)
    };

    start_fixture(&mut session, "sizes", Targets::none()).expect("the script starts");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline && size(&mut session) != (120, 34) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        size(&mut session),
        (120, 34),
        "held to the area and the overflow"
    );
    assert!(session.size_held());
    session.resize(60, 20);
    assert_eq!(size(&mut session), (120, 34), "the window does not size it");

    session.cancel_transfer().expect("the script stops");
    while std::time::Instant::now() < deadline && session.is_transferring() {
        session.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(size(&mut session), (60, 20), "the size is given back");
}

#[test]
fn a_script_that_ended_does_not_hold_the_keyboard() {
    let mut session = Session::new(std::sync::Arc::new(|| {})).expect("session is created");
    session
        .connect_console(&crate::consoles::Console {
            name: "test".to_string(),
            program: "cat".to_string(),
            ..crate::consoles::default_console()
        })
        .expect("the local console starts");

    start_fixture(&mut session, "quick", Targets::none()).expect("the script starts");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline && session.is_transferring() {
        session.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!session.is_transferring());
    let text: String = {
        let content = session.test_terminal().content();
        (0..content.rows)
            .map(|row| row_text(&content, row))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(text.contains("quick"), "what the script printed: {text}");
}
