//! Keys with `Ctrl` that make a control byte go to the line.

use super::window;

#[test]
fn ctrl_with_a_sign_sends_its_control_byte() {
    for (key, byte) in [
        (egui::Key::OpenBracket, "\u{1b}"),
        (egui::Key::Backslash, "\u{1c}"),
        (egui::Key::CloseBracket, "\u{1d}"),
    ] {
        let mut window = window();
        window
            .harness
            .key_press_modifiers(egui::Modifiers::CTRL, key);
        window.harness.run_steps(2);
        let sent = window.wait_written(byte);
        assert_eq!(sent, byte, "{key:?}");
    }
}
