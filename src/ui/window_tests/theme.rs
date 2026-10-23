//! The theme as a program on the line hears of it: a program that set private
//! mode 2031 is told when the page turns dark or light, and the colours it
//! asks for then are the new ones.

use crate::config::ThemeMode;

use super::{Window, window};

/// Turns the window to `mode`, the way the settings page does.
fn turn(window: &mut Window, mode: ThemeMode) {
    let app = window.harness.state_mut();
    app.settings.theme = mode;
    app.theme.set_mode(mode);
    app.apply_terminal_theme();
}

/// The background of the terminal as `OSC 11` answers it.
fn background_answer(window: &Window) -> String {
    let color = window.harness.state().terminal_theme.background;
    let channel = |value: u8| format!("{value:02x}{value:02x}");
    format!(
        "\x1b]11;rgb:{}/{}/{}",
        channel(color.r()),
        channel(color.g()),
        channel(color.b())
    )
}

#[test]
fn a_program_that_asked_is_told_the_theme_changed() {
    let mut window = window();
    turn(&mut window, ThemeMode::Dark);
    window.harness.run_steps(2);

    window.device_says(b"\x1b[?2031h\x1b[?996n");
    let asked = window.wait_written("\x1b[?997;1n");
    assert!(
        !asked.contains("997;2n"),
        "a dark page is told as dark: {asked:?}"
    );

    turn(&mut window, ThemeMode::Light);
    window.wait_written("\x1b[?997;2n");
    window.device_says(b"\x1b]11;?\x1b\\");
    let answer = window.wait_written("\x1b]11;");
    assert!(
        answer.contains(&background_answer(&window)),
        "the background asked for on the report is the light one: {answer:?}"
    );

    turn(&mut window, ThemeMode::Dark);
    window.wait_written("\x1b[?997;1n");
}

#[test]
fn a_program_that_did_not_ask_is_told_nothing() {
    let mut window = window();
    turn(&mut window, ThemeMode::Dark);
    window.harness.run_steps(2);
    turn(&mut window, ThemeMode::Light);
    window.harness.run_steps(4);
    let written = window.written();
    assert!(
        !written.contains("997"),
        "nothing goes to a program that did not set the mode: {written:?}"
    );
}
