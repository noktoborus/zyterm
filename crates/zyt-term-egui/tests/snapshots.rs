//! Pictures of the widget, rendered and compared with the ones in `img/`.
//!
//! Each picture is both a test and an illustration of the README: a change
//! that moves a pixel of what the widget draws itself fails here.
//! `UPDATE_SNAPSHOTS=1 cargo test -p zyt-term-egui --test snapshots` draws
//! them again.

use egui_kittest::{Harness, SnapshotOptions};
use zyt_term::{ClipboardAccess, Snapshot, Terminal, TerminalConfig};
use zyt_term_egui::{TerminalCache, TerminalFont, TerminalOutput, TerminalTheme, TerminalView};

/// Shows the widget over the page of `terminal` the way its owner does: the
/// page is taken, the widget drawn, and what it asked applied in order.
fn own(
    terminal: &mut Terminal,
    snapshot: &mut Snapshot,
    show: impl FnOnce(&Snapshot) -> (egui::Response, TerminalOutput),
) -> (egui::Response, TerminalOutput) {
    snapshot.take(terminal);
    let (response, output) = show(snapshot);
    for command in output.commands.iter().cloned() {
        terminal.apply(command);
    }
    snapshot.take(terminal);
    (response, output)
}

/// Where the pictures are kept: beside the README, which shows them.
const PICTURES: &str = "img";

/// What the widget draws, with what it draws from.
struct Drawn {
    terminal: Terminal,
    content: Snapshot,
    cache: TerminalCache,
    theme: TerminalTheme,
    font: TerminalFont,
}

/// A terminal of `columns` and `rows`, written by `then`.
fn drawn(columns: usize, rows: usize, then: impl FnOnce(&mut Terminal)) -> Drawn {
    let mut terminal = Terminal::new(TerminalConfig {
        columns,
        rows,
        scrollback: 100,
        clipboard: ClipboardAccess::Disabled,
    })
    .expect("a terminal");
    then(&mut terminal);
    Drawn {
        terminal,
        content: Snapshot::new(),
        cache: TerminalCache::new(),
        theme: TerminalTheme::default(),
        font: TerminalFont::default(),
    }
}

fn picture(state: Drawn, size: egui::Vec2, name: &str) {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_theme(egui::Theme::Dark)
        .build_ui_state(
            |ui, drawn: &mut Drawn| {
                let Drawn {
                    terminal,
                    content,
                    cache,
                    theme,
                    font,
                } = drawn;
                own(terminal, content, |snapshot| {
                    TerminalView::new(snapshot, cache, theme, font)
                        .focused(false)
                        .auto_resize(false)
                        .show(ui)
                });
            },
            state,
        );
    harness.run();
    harness.snapshot_options(name, &SnapshotOptions::new().output_path(PICTURES));
}

/// A gap between the screen before a pause and what came after it: the sign
/// of the pause and the dashed rule drawn by the widget, the text the caller
/// gave in the font of the line.
#[test]
fn a_gap_between_the_screen_before_a_pause_and_after_it() {
    let state = drawn(44, 6, |terminal| {
        terminal.feed(
            b"[  412.30] usb 1-1: reset\r\n[  412.31] usb 1-1: quiet\r\n\
              [  412.32] usb 1-1: config 1\r\n[  412.33] usb 1-1: new device\r\n\
              [  412.34] usb 1-1: Product: modem\r\n[  412.35] usb 1-1: found",
        );
        terminal.mark_gap("14 s skipped, in the history of tmux");
        terminal.feed(b"[  498.02] eth0: link up\r\n$ ");
        terminal.scroll(4);
    });
    picture(state, egui::vec2(440.0, 130.0), "gap");
}

/// Row −1 above the oldest line, offering what lies further up: drawn as the
/// line of a gap is, its sign an arrow up, the text the caller gave.
#[test]
fn row_minus_one_above_the_oldest_line() {
    let mut state = drawn(44, 4, |terminal| {
        terminal.feed(
            b"[  412.33] usb 1-1: new device\r\n[  412.34] usb 1-1: Product: modem\r\n\
              [  412.35] usb 1-1: found\r\n[  498.02] eth0: link up\r\n$ ",
        );
    });
    let history = state.terminal.history_size();
    state.terminal.scroll_to(history);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(440.0, 90.0))
        .with_theme(egui::Theme::Dark)
        .build_ui_state(
            |ui, drawn: &mut Drawn| {
                let Drawn {
                    terminal,
                    content,
                    cache,
                    theme,
                    font,
                } = drawn;
                own(terminal, content, |snapshot| {
                    TerminalView::new(snapshot, cache, theme, font)
                        .focused(true)
                        .auto_resize(false)
                        .above_top(Some(zyt_term_egui::AboveTop {
                            text: "load 24 lines of history".to_string(),
                        }))
                        .show(ui)
                });
            },
            state,
        );
    harness.run_steps(2);
    harness.event(egui::Event::PointerMoved(egui::pos2(380.0, 84.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run();
    harness.snapshot_options("above-top", &SnapshotOptions::new().output_path(PICTURES));
}
