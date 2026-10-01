//! The window of a command of several lines.
//!
//! A line typed into the terminal is typed into the device as it is written,
//! which is the right answer for a line and the wrong one for a block: a loop,
//! a here-document, a configuration pasted into a board all have to stand
//! whole before any of them is sent, and a shell reading them one line at a
//! time answers each one as it arrives. So they are written here and sent
//! together.
//!
//! It is a window with a title and a cross and it does not move, the way the
//! panel of what runs is one: what is in the field is the thing being worked
//! on, and a window dragged out from under the eye is a window looked for the
//! next time.
//!
//! It stands edge to edge of the window, because a line of a block is as long
//! as the lines of the terminal under it: a field narrower than the output
//! wraps what the device will not, and a block read in one width and run in
//! another is a block read twice. It is as tall as the field and the button
//! together and no taller — a window with room under its contents is a window
//! that looks like something is missing from it.
//!
//! `Esc` and the cross close it and send nothing. What was typed is dropped
//! with the window, because a command half written is not a command somebody
//! meant to keep.

use crate::app::App;

/// How many lines the field stands at before anything is typed into it.
///
/// It grows with what is written, so this is the room a block is begun in and
/// not a limit: five lines is a loop with something in it, which is the
/// shortest thing worth opening the window for.
const ROWS: usize = 5;

/// Draws the window while one is open.
pub fn draw(app: &mut App, context: &egui::Context) {
    let Some(mut text) = app.ui.block.clone() else {
        return;
    };

    let base = egui::Id::new("block");
    let field = base.with("field");

    let screen = context.content_rect();

    egui::Area::new(egui::Id::new("CommandArea"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            let margins = ui.style().spacing.window_margin.sum().x;
            let width = (screen.width() - margins).max(0.0);
            ui.set_width(width);
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(field)
                    .desired_rows(ROWS)
                    .desired_width(width)
                    .font(egui::TextStyle::Monospace),
            );
        });

    // The field takes the keyboard the moment the window appears, so the block
    // is typed into it and not looked at.
    if !context.memory(|memory| memory.has_focus(field)) && app.ui.block.as_deref() == Some("") {
        context.memory_mut(|memory| memory.request_focus(field));
    }

    let escaped =
        context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let entered =
        context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));

    app.ui.block = Some(text);
    if escaped {
        app.ui.block = None;
        return;
    }
    if entered {
        app.send_block_input();
    }
}
