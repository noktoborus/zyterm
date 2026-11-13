//! The wheel over the terminal with a key held: `Shift` turns it a screen at a
//! time, `Ctrl` turns the size of the font.

use super::{Window, window};

/// Where the pointer stands over the terminal.
const ON_THE_TERMINAL: egui::Pos2 = egui::pos2(60.0, 60.0);

/// The window with a scrollback of several screens.
fn with_history() -> Window {
    let mut window = window();
    let lines: String = (0..300).map(|line| format!("line {line}\r\n")).collect();
    window.device_says(lines.as_bytes());
    window.step_until("the history", |window| {
        window.harness.state_mut().session.page().history_size > 200
    });
    window
}

/// One notch of the wheel away from the user, with `held` down.
fn notch(window: &mut Window, held: egui::Modifiers) {
    let events = &mut window.harness.input_mut().events;
    events.push(egui::Event::PointerMoved(ON_THE_TERMINAL));
    events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: held,
    });
    window.harness.run_steps(20);
}

/// How far up the view stands, and how many rows a screen is.
fn offset_and_rows(window: &mut Window) -> (usize, usize) {
    let page = window.harness.state_mut().session.page();
    (page.display_offset, page.rows)
}

#[test]
fn the_wheel_with_shift_turns_a_screen_at_a_time() {
    let mut window = with_history();
    notch(&mut window, egui::Modifiers::SHIFT);
    let (offset, rows) = offset_and_rows(&mut window);
    assert_eq!(offset, rows, "one screen up");

    notch(&mut window, egui::Modifiers::SHIFT);
    let (offset, rows) = offset_and_rows(&mut window);
    assert_eq!(offset, rows * 2, "and one more");
}

#[test]
fn the_wheel_alone_turns_less_than_a_screen() {
    let mut window = with_history();
    notch(&mut window, egui::Modifiers::NONE);
    let (offset, rows) = offset_and_rows(&mut window);
    assert!(offset > 0 && offset < rows, "{offset} of {rows}");
}

#[test]
fn the_wheel_with_ctrl_turns_the_size_of_the_font() {
    let mut window = with_history();
    let size = window.harness.state().settings.font_size;

    notch(&mut window, egui::Modifiers::CTRL);
    assert_eq!(window.harness.state().settings.font_size, size + 1.0);
    let (offset, _) = offset_and_rows(&mut window);
    assert_eq!(offset, 0, "the view stays");

    let events = &mut window.harness.input_mut().events;
    events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -2.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::CTRL,
    });
    window.harness.run_steps(20);
    assert_eq!(window.harness.state().settings.font_size, size - 1.0);
}

#[test]
fn the_font_turned_by_the_wheel_stays_in_its_sizes() {
    let mut window = window();
    for _ in 0..40 {
        notch(&mut window, egui::Modifiers::CTRL);
    }
    let largest = *crate::config::FONT_SIZES.end();
    assert_eq!(window.harness.state().settings.font_size, largest);
}
