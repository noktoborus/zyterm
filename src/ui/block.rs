//! The plate of a command of several lines.
//!
//! A line typed into the terminal is typed into the device as it is written,
//! which is the right answer for a line and the wrong one for a block: a loop,
//! a here-document, a configuration pasted into a board all have to stand
//! whole before any of them is sent, and a shell reading them one line at a
//! time answers each one as it arrives. So they are written here and sent
//! together.
//!
//! It is a plate and not a window, the same shape the signals rise in: it
//! stands against one edge of the terminal, edge to edge of it, and it does not
//! move under the hand. What is in the field is the thing being worked on, and
//! a window dragged out from under the eye is a window looked for the next
//! time.
//!
//! It is as wide as the terminal, because a line of a block is as long as the
//! lines under it: a field narrower than the output wraps what the device will
//! not, and a block read in one width and run in another is a block read twice.
//! It is as tall as the field and the row of buttons together and no taller — a
//! plate with room under its contents looks like something is missing from it.
//!
//! Which edge it stands against is answered by the one button at the left end
//! of that row, and the answer is kept for the session: the plate covers
//! output, and which half of the screen may be covered is a question about what
//! is being read now. One button and not two, because a plate at an edge has
//! one move: the other edge.
//!
//! The switch left of the button that sends reads the carets of the block as
//! control codes — `crate::caret`. That one is kept with the source and not
//! with the session: which it is, is a fact about what is at the other end of
//! the line, so it is written to `SourceMemory::block_caret` as it is turned and
//! read back when the plate opens. It is off on a source nobody has turned it
//! for: a caret is a character of a shell before it is a notation, and a block
//! that grew a byte nobody asked for is worse than one that needs a switch
//! turned.
//!
//! While it stands it holds the keyboard. `App::handle_keyboard` dispatches
//! nothing then, so every key is the field's; a press that landed beside the
//! plate and left the keyboard nowhere is taken back on the next frame, or the
//! field would go quiet with no sign of why. The keys of the plate are answered
//! from its second frame onwards (`UiState::block_drawn`): the keys of a frame
//! are read before anything is drawn, so the press that opened the plate is
//! still in the events when it first stands.
//!
//! `Esc` closes it and sends nothing. What was typed is dropped with the plate,
//! because a command half written is not a command somebody meant to keep.

use crate::app::App;
use crate::ui::PLATE_GAP;
use crate::ui::icons;
use rust_i18n::t;

/// How many lines the field stands at before anything is typed into it.
///
/// It grows with what is written, so this is the room a block is begun in and
/// not a limit: five lines is a loop with something in it, which is the
/// shortest thing worth opening the plate for.
const ROWS: usize = 5;

/// Draws the plate while one is open, against the given edge of the terminal.
///
/// `area` is what is left of the window once the status bar has taken its own,
/// so the plate at the foot stands over the output and never over the bar.
pub fn draw(app: &mut App, context: &egui::Context, area: egui::Rect) {
    let Some(mut text) = app.ui.block.clone() else {
        return;
    };

    let base = egui::Id::new("block");
    let field = base.with("field");
    let (anchor, pivot) = match app.ui.block_at_top {
        true => (
            egui::pos2(area.left(), area.top() + PLATE_GAP),
            egui::Align2::LEFT_TOP,
        ),
        false => (
            egui::pos2(area.left(), area.bottom() - PLATE_GAP),
            egui::Align2::LEFT_BOTTOM,
        ),
    };

    let mut sent = false;
    let stood = app.ui.block_drawn;
    app.ui.block_drawn = true;

    egui::Area::new(base.with("plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(anchor)
        .pivot(pivot)
        .show(context, |ui| {
            let frame = egui::Frame::popup(ui.style());
            let inner = (area.width() - frame.total_margin().sum().x).max(0.0);
            frame.show(ui, |ui| {
                ui.set_width(inner);
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .id(field)
                        .desired_rows(ROWS)
                        .desired_width(inner)
                        .font(egui::TextStyle::Monospace),
                );
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let send = ui
                            .button(t!("block.send"))
                            .on_hover_text(t!("block.send_hint"));
                        sent = send.clicked();
                        caret_switch(app, ui);
                        move_button(app, ui);
                    });
                });
            });
        });

    app.ui.block = Some(text);

    if !holds_keyboard(context, field) {
        context.memory_mut(|memory| memory.request_focus(field));
    }
    let escaped = stood
        && context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let entered = stood && sends(context);

    if escaped {
        app.ui.block = None;
        return;
    }
    if entered || sent {
        app.send_block_input();
    }
}

/// Whether the press that sends stands in the events of this frame.
///
/// `Ctrl` with `Enter` is it, and the combination that opens the plate is the
/// same with `Shift`. The toolkit matches a pattern of fewer modifiers against
/// a press of more, so the one with `Shift` is taken out of the events first
/// and answered by nothing: a key pressed to open a plate that already stands
/// must not send what is in it.
fn sends(context: &egui::Context) -> bool {
    context.input_mut(|input| {
        let opening = input.consume_key(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Enter,
        );
        let sending = input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter);
        sending && !opening
    })
}

/// The switch that says to read the carets of the block as control codes.
///
/// The word stands left of the switch and carries the same hint, because the
/// word is the wider target and the sentence is what says that `^C` is a byte
/// here. The hint names the sequences a block is most often ended with and the
/// one that is a line of its own, rather than the whole table: the table is in
/// `crate::caret`, and a hint that is a table is a hint nobody finishes.
///
/// What it turns is `App::set_block_caret` and not the field of the interface,
/// because the answer belongs to the source: it goes into the file of that port
/// or console as it is turned.
fn caret_switch(app: &mut App, ui: &mut egui::Ui) {
    let hint = t!("block.caret_hint").to_string();
    let mut caret = app.ui.block_caret;
    let turned = crate::ui::widgets::switch(ui, &mut caret).on_hover_text(hint.clone());
    ui.label(t!("block.caret")).on_hover_text(hint);
    if turned.changed() {
        app.set_block_caret(caret);
    }
}

/// The button that takes the plate to the other edge of the terminal.
///
/// It shows the way it would move and not where it is: a plate at the foot
/// offers the head, and the arrow on it is the one the hand is asking for.
fn move_button(app: &mut App, ui: &mut egui::Ui) {
    let at_top = app.ui.block_at_top;
    let (icon, hint) = move_control(at_top);
    if ui.button(icon).on_hover_text(t!(hint)).clicked() {
        app.ui.block_at_top = !at_top;
    }
}

/// The arrow and the sentence of that button, for a plate standing where it is.
fn move_control(at_top: bool) -> (&'static str, &'static str) {
    match at_top {
        true => (icons::DOWN, "block.to_bottom"),
        false => (icons::UP, "block.to_top"),
    }
}

/// Whether the field is the thing the keyboard is in.
///
/// It is asked after the plate is drawn, which is where a button of it has
/// just taken the keyboard off the field, and where `Esc` has left it nowhere.
fn holds_keyboard(context: &egui::Context, field: egui::Id) -> bool {
    context.memory(|memory| memory.focused() == Some(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_offers_the_edge_the_plate_does_not_stand_against() {
        assert_eq!(move_control(false).0, icons::UP);
        assert_eq!(move_control(true).0, icons::DOWN);
    }
}
