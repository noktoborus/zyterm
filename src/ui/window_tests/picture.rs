//! The menu of a picture on the page: its entry Copy hands the pixels to the
//! clipboard of the toolkit.

use egui_kittest::kittest::Queryable;
use rust_i18n::t;

use super::window;

/// A sixel picture of 64 by 36 pixels, painted red.
const PICTURE: &[u8] = b"\x1bPq\"1;1;64;36#0;2;100;0;0#0~~~~$-\x1b\\";

#[test]
fn copy_in_the_menu_of_a_picture_puts_its_pixels_in_the_clipboard() {
    let mut window = window();
    window.device_says(PICTURE);
    window.step_until("the picture on the page", |window| {
        let content = window.harness.state_mut().session.test_terminal().content();
        (0..content.columns).any(|column| {
            content
                .cell(column, 0)
                .is_some_and(|cell| cell.image.is_some())
        })
    });
    let menu = {
        let app = window.harness.state_mut();
        let content = app.session.test_terminal().content();
        let id = (0..content.columns)
            .find_map(|column| {
                content
                    .cell(column, 0)
                    .and_then(|cell| cell.image.map(|part| part.id))
            })
            .expect("the line of the picture names it");
        app.session
            .test_terminal()
            .picture(id)
            .map(|picture| crate::app::PictureMenu { id, picture })
    };
    crate::ui::terminal::open_picture_menu(window.harness.state_mut(), menu);
    window.harness.run_steps(3);

    window
        .harness
        .get_by_label(&t!("command.terminal.copy"))
        .click();
    let mut copied = None;
    window.step_until("the picture in the clipboard", |window| {
        copied = window
            .harness
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyImage(image) => Some(image.size),
                _ => None,
            });
        copied.is_some()
    });
    assert_eq!(copied, Some([64, 36]));
}
