//! Row −1: the view scrolls one row past the oldest line onto it while the
//! caller gives one, says so, and reports a click on it.

use egui_kittest::Harness;
use zyt_term::{ClipboardAccess, Snapshot, Terminal, TerminalConfig};
use zyt_term_egui::{
    AboveTop, TerminalCache, TerminalFont, TerminalOutput, TerminalTheme, TerminalView,
};

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

struct State {
    terminal: Terminal,
    content: Snapshot,
    cache: TerminalCache,
    theme: TerminalTheme,
    font: TerminalFont,
    above: Option<AboveTop>,
    output: TerminalOutput,
}

fn harness(above: Option<AboveTop>) -> Harness<'static, State> {
    let mut terminal = Terminal::new(TerminalConfig {
        columns: 30,
        rows: 4,
        scrollback: 100,
        clipboard: ClipboardAccess::Disabled,
    })
    .expect("a terminal");
    for line in 0..8 {
        terminal.feed(format!("line {line}\r\n").as_bytes());
    }
    let state = State {
        terminal,
        content: Snapshot::new(),
        cache: TerminalCache::new(),
        theme: TerminalTheme::default(),
        font: TerminalFont::default(),
        above,
        output: TerminalOutput::default(),
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 120.0))
        .build_ui_state(
            |ui, state: &mut State| {
                let State {
                    terminal,
                    content,
                    cache,
                    theme,
                    font,
                    above,
                    output,
                } = state;
                let (_, out) = own(terminal, content, |snapshot| {
                    TerminalView::new(snapshot, cache, theme, font)
                        .focused(true)
                        .auto_resize(false)
                        .above_top(above.clone())
                        .show(ui)
                });
                *output = out;
            },
            state,
        );
    harness.run_steps(2);
    harness
}

fn wheel(harness: &mut Harness<'static, State>, lines: f32) {
    harness.event(egui::Event::PointerMoved(egui::pos2(100.0, 40.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, lines),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(2);
}

#[test]
fn the_view_steps_onto_row_minus_one_past_the_top_and_off_it() {
    let mut harness = harness(Some(AboveTop {
        text: "load more".to_string(),
    }));
    let history = harness.state().terminal.history_size();
    harness.state_mut().terminal.scroll_to(history);
    harness.run_steps(2);
    assert!(
        !harness.state().output.at_above_top,
        "at the top, not past it"
    );

    wheel(&mut harness, 1.0);
    assert!(harness.state().output.at_above_top, "one row past the top");

    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(100.0, 4.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: egui::pos2(100.0, 4.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    assert!(harness.state().output.above_top_pressed, "the row clicked");

    wheel(&mut harness, -1.0);
    harness.run_steps(2);
    assert!(!harness.state().output.at_above_top, "off it again");
    assert!(
        harness.state().terminal.display_offset() < history,
        "one line of the wheel takes the view off the row, the rest scroll on"
    );
}

#[test]
fn without_a_row_the_view_stops_at_the_oldest_line() {
    let mut harness = harness(None);
    let history = harness.state().terminal.history_size();
    harness.state_mut().terminal.scroll_to(history);
    wheel(&mut harness, 3.0);
    assert!(!harness.state().output.at_above_top);
    assert_eq!(harness.state().terminal.display_offset(), history);
}
