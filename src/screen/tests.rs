//! The screen fed in its thread, with no window drawing it.

use super::wire::Wire;
use super::*;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A line that hands over what a test put in it and keeps what was written.
#[derive(Default)]
struct FakeWire {
    incoming: Mutex<Vec<u8>>,
    written: Mutex<Vec<u8>>,
}

impl Wire for FakeWire {
    fn read_into(&self, spare: &mut Vec<u8>) {
        spare.clear();
        spare.append(&mut self.incoming.lock().unwrap());
    }

    fn write(&self, bytes: &[u8]) {
        self.written.lock().unwrap().extend_from_slice(bytes);
    }
}

fn settings() -> ScreenSettings {
    ScreenSettings {
        clipboard: zyt_term::ClipboardAccess::Disabled,
        pictures: zyt_term::PictureAccess::default(),
        scrollback_memory: 1024 * 1024,
        marks: true,
        progress: true,
        picture_limit: zyt_term::PictureLimit::default(),
        picture_weight: zyt_term::DEFAULT_PICTURE_WEIGHT,
        picture_budget: zyt_term::DEFAULT_PICTURE_BUDGET,
    }
}

/// A screen of 40 by 6 on a fake line, and what it was told was shown.
fn screen() -> (
    ScreenHandle,
    Arc<FakeWire>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let shown = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = shown.clone();
    let handle = ScreenHandle::spawn(
        Setup {
            columns: 40,
            rows: 6,
            settings: settings(),
        },
        Arc::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
    )
    .expect("the screen starts");
    let wire = Arc::new(FakeWire::default());
    handle.send(ScreenCommand::Wire {
        wire: Some(wire.clone()),
        lossy: false,
    });
    (handle, wire, shown)
}

/// The device says `bytes`, and rings the screen the way a source does.
fn say(handle: &ScreenHandle, wire: &FakeWire, bytes: &[u8]) {
    wire.incoming.lock().unwrap().extend_from_slice(bytes);
    (handle.waker())();
}

fn until(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn text(view: &View) -> String {
    view.snapshot
        .page()
        .cells
        .iter()
        .map(|cell| cell.ch)
        .collect()
}

#[test]
fn a_question_of_a_program_is_answered_with_no_frame() {
    let (handle, wire, _) = screen();

    say(&handle, &wire, b"\x1b[c");

    assert!(
        until(|| !wire.written.lock().unwrap().is_empty()),
        "the screen answered while the window drew nothing"
    );
}

#[test]
fn what_the_device_said_is_on_the_board() {
    let (handle, wire, shown) = screen();
    say(&handle, &wire, b"hello");

    assert!(until(
        || shown.load(std::sync::atomic::Ordering::Relaxed) > 0
    ));
    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("hello")
    }));
}

#[test]
fn a_page_taken_twice_is_the_same_page() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"hello");
    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("hello")
    }));

    handle.take(&mut view);

    assert!(
        text(&view).contains("hello"),
        "the board kept nothing stale"
    );
}

#[test]
fn input_goes_to_the_line() {
    let (handle, wire, _) = screen();

    handle.send(ScreenCommand::Input(b"ls\r".to_vec()));

    assert!(until(|| wire.written.lock().unwrap().as_slice() == b"ls\r"));
}

#[test]
fn a_question_is_answered_on_the_board_with_its_tag() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"alpha beta");
    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("beta")
    }));
    handle.send(ScreenCommand::Terminal(zyt_term::Command::SelectStart {
        kind: zyt_term::SelectionKind::Semantic,
        column: 0,
        row: 0,
    }));

    handle.send(ScreenCommand::Ask {
        tag: 7,
        question: zyt_term::Command::Copy,
    });

    assert!(until(|| {
        handle.take(&mut view);
        !view.told.answers.is_empty()
    }));
    assert!(matches!(
        view.told.answers.as_slice(),
        [(7, zyt_term::Answer::Copied(Some(text)))] if text == "alpha"
    ));
}

#[test]
fn a_selection_holds_the_line_unread() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"alpha");
    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("alpha")
    }));
    handle.send(ScreenCommand::Terminal(zyt_term::Command::SelectStart {
        kind: zyt_term::SelectionKind::Semantic,
        column: 0,
        row: 0,
    }));
    assert!(until(|| {
        handle.take(&mut view);
        view.said.held_for_selection
    }));

    say(&handle, &wire, b" beta");
    std::thread::sleep(Duration::from_millis(50));
    handle.take(&mut view);

    assert!(!text(&view).contains("beta"), "nothing was read meanwhile");
    handle.send(ScreenCommand::Terminal(zyt_term::Command::SelectClear));
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("beta")
    }));
}

#[test]
fn a_resize_reaches_the_page() {
    let (handle, _wire, _) = screen();

    handle.send(ScreenCommand::Resize {
        columns: 20,
        rows: 3,
    });

    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        (view.snapshot.page().columns, view.snapshot.page().rows) == (20, 3)
    }));
}

/// What the line was written since the last call, as text.
fn written(wire: &FakeWire) -> String {
    let bytes = std::mem::take(&mut *wire.written.lock().unwrap());
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Takes the board until `check` holds.
fn until_said(
    handle: &ScreenHandle,
    view: &mut View,
    mut check: impl FnMut(&View) -> bool,
) -> bool {
    until(|| {
        handle.take(view);
        check(view)
    })
}

#[test]
fn a_start_that_is_no_tmux_is_given_back_as_text() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"before\x1bP1000phello\r\nafter\r\n");

    let mut view = View::default();
    assert!(until_said(&handle, &mut view, |view| text(view).contains("after")));
    let shown = text(&view);
    assert!(
        shown.contains("before") && shown.contains("hello"),
        "{shown}"
    );
    assert!(view.said.tmux.look.is_none(), "the mode is not standing");
    assert_eq!(written(&wire), "", "nothing was written for it");
}

#[test]
fn input_while_entering_is_held_and_given_back_with_the_start() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"\x1bP1000p");
    let mut view = View::default();
    assert!(until_said(&handle, &mut view, |view| view
        .said
        .tmux
        .look
        .is_some()));

    handle.send(ScreenCommand::Input(b"ls\r".to_vec()));

    assert!(until_said(&handle, &mut view, |view| view.said.tmux.look
        == Some(TmuxLook::Waiting {
            held: 3,
            leaving: false
        })));
    assert_eq!(written(&wire), "");
    handle.send(ScreenCommand::Tmux(TmuxCommand::Press));
    assert!(until_said(&handle, &mut view, |view| view
        .said
        .tmux
        .look
        .is_none()));
    assert!(until(|| wire.written.lock().unwrap().as_slice() == b"ls\r"));
}

#[test]
fn the_mode_starts_on_its_first_line_and_writes_only_commands() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"the plain terminal\r\n");
    let mut view = View::default();
    assert!(until_said(&handle, &mut view, |view| text(view).contains("plain")));

    say(
        &handle,
        &wire,
        b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n",
    );

    assert!(until(|| wire
        .written
        .lock()
        .unwrap()
        .ends_with(b"display -p '#{version}'\n")));
    assert!(until_said(&handle, &mut view, |view| matches!(
        view.said.tmux.look,
        Some(TmuxLook::Shown { .. })
    )));
    assert!(
        text(&view).contains("the plain terminal"),
        "the plain terminal stands until a pane is shown"
    );
    assert!(view.said.tmux.sets_size);
}

#[test]
fn another_line_drops_the_mode_and_writes_nothing() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n");
    assert!(until(|| wire
        .written
        .lock()
        .unwrap()
        .ends_with(b"display -p '#{version}'\n")));
    written(&wire);

    handle.send(ScreenCommand::Wire {
        wire: Some(wire.clone()),
        lossy: false,
    });

    let mut view = View::default();
    assert!(until_said(&handle, &mut view, |view| view
        .said
        .tmux
        .look
        .is_none()));
    assert!(!view.said.tmux.sets_size);
    assert_eq!(written(&wire), "");
}

/// The screen reads the line as the bytes come, so the start of the mode may
/// arrive a byte at a time; it is found all the same.
#[test]
fn the_mode_starts_when_it_comes_a_byte_at_a_time() {
    let (handle, wire, _) = screen();
    for byte in b"\x1bP1000p%begin 1 1 0\r\n%end 1 1 0\r\n%session-changed $0 zyt\r\n" {
        say(&handle, &wire, &[*byte]);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(until(|| wire
        .written
        .lock()
        .unwrap()
        .ends_with(b"display -p '#{version}'\n")));
}

/// A fault in a turn makes the screen anew: the page starts clean, the window
/// is told, and what the device says next is fed as before.
#[test]
fn a_panic_in_a_turn_makes_the_screen_anew() {
    let (handle, wire, _) = screen();
    say(&handle, &wire, b"before");
    let mut view = View::default();
    assert!(until_said(&handle, &mut view, |view| text(view).contains("before")));

    handle.send(ScreenCommand::Panic);
    assert!(until_said(&handle, &mut view, |view| !view
        .told
        .messages
        .is_empty()));
    say(&handle, &wire, b"after");

    assert!(until_said(&handle, &mut view, |view| text(view).contains("after")));
    assert!(!text(&view).contains("before"), "the page started clean");
}

/// A turn that goes on is seen from outside while it lasts, and not after.
#[test]
fn a_long_turn_is_seen_while_it_lasts() {
    let (handle, _wire, _) = screen();

    handle.send(ScreenCommand::Sleep(Duration::from_millis(300)));

    assert!(until(|| handle
        .busy_for()
        .is_some_and(|took| took >= Duration::from_millis(100))));
    assert!(until(|| handle
        .busy_for()
        .is_none_or(|took| took < Duration::from_millis(100))));
}

/// What a script shows reaches the page with no frame of the window, its
/// echo and the line of its notice in the order they were said.
#[test]
fn what_a_script_shows_reaches_the_page_with_no_frame() {
    use zyt_script::Prompt as _;

    let (handle, _wire, _) = screen();
    let echo = zyt_script::ByteSwap::with_capacity(0);
    handle.send(ScreenCommand::Echo(Some(echo.clone())));
    let (talker, _heard) = crate::scripts::talker(
        Arc::new(|| {}),
        echo,
        handle.waker(),
        handle.state_reader(),
        Default::default(),
        zyt_script::Cancel::new(),
    );

    talker.echo(b"echoed ");
    talker.notice(zyt_script::NoticeKind::Info, "noted".to_string());

    let mut view = View::default();
    assert!(until(|| {
        handle.take(&mut view);
        text(&view).contains("echoed \u{2502} noted")
    }));
}

/// The state a script asks for holds every byte it showed before it asked.
#[test]
fn the_state_a_script_asks_for_holds_what_it_showed() {
    use zyt_script::Prompt as _;

    let (handle, _wire, _) = screen();
    let echo = zyt_script::ByteSwap::with_capacity(0);
    handle.send(ScreenCommand::Echo(Some(echo.clone())));
    let (talker, _heard) = crate::scripts::talker(
        Arc::new(|| {}),
        echo,
        handle.waker(),
        handle.state_reader(),
        Default::default(),
        zyt_script::Cancel::new(),
    );

    talker.echo(b"\x1b[?1049h\x1b]2;a title\x07");
    let mut bytes = Vec::new();
    assert!(talker.term_state(&mut bytes));
    let bytes = String::from_utf8_lossy(&bytes);
    assert!(
        bytes.contains("\x1b[?1049h") && bytes.contains("a title"),
        "{bytes:?}"
    );
}

/// The settings the window sends every frame wake the screen, and publish
/// nothing: a page is copied only when something changed.
#[test]
fn settings_sent_every_frame_publish_nothing() {
    let (handle, _wire, shown) = screen();
    std::thread::sleep(Duration::from_millis(50));
    let before = shown.load(std::sync::atomic::Ordering::Relaxed);
    for _ in 0..5 {
        handle.send(ScreenCommand::Tmux(TmuxCommand::Overflow((80, 24))));
        handle.send(ScreenCommand::Tmux(TmuxCommand::Pause(None)));
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(shown.load(std::sync::atomic::Ordering::Relaxed), before);
}
