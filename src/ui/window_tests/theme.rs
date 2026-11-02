//! The theme as a program on the line hears of it: a program that set private
//! mode 2031 is told when the page turns dark or light, and the colours it
//! asks for then are the new ones. The window wears the colours of the theme of
//! the terminal.

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

/// The window takes its colours from the theme of the terminal as the theme
/// turns, a light window under a light theme and a dark one under a dark.
#[test]
fn the_window_wears_the_colours_of_the_theme_of_the_terminal() {
    let mut window = window();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        turn(&mut window, mode);
        window.harness.run_steps(2);
        let theme = window.harness.state().terminal_theme.clone();
        let colors = crate::ui::colors::Colors::from_theme(&theme);
        let visuals = window.harness.ctx.global_style().visuals.clone();
        assert_eq!(visuals.panel_fill, colors.panel, "{mode:?}");
        assert_eq!(visuals.extreme_bg_color, theme.background, "{mode:?}");
        assert_eq!(visuals.dark_mode, mode == ThemeMode::Dark, "{mode:?}");
        if let Ok(dir) = std::env::var("ZYTERM_SHOT_DIR") {
            let image = window.harness.render().expect("the window renders");
            image
                .save(std::path::Path::new(&dir).join(format!("theme-{mode:?}.png")))
                .expect("the picture is written");
        }
    }
}

/// The text stands whole once the theme turned: the picture of the grid made
/// after the turn is the one a picture made anew, on a calm pass, would be.
#[test]
fn the_text_is_whole_after_the_theme_turned() {
    let mut window = window();
    window.device_says(b"The quick brown fox jumps over the lazy dog 0123456789\r\n");
    window.harness.run_steps(4);
    for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Light] {
        turn(&mut window, mode);
        window.harness.run_steps(6);
        let turned = window.harness.render().expect("the window renders");
        window.harness.state_mut().terminal_cache.forget();
        window.harness.run_steps(6);
        let anew = window.harness.render().expect("the window renders");
        assert!(
            turned == anew,
            "the grid after turning to {mode:?} is not whole"
        );
    }
}
