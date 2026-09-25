//! Confirmation for actions that throw something away.

use rust_i18n::t;

/// What the user answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The window is still open.
    Pending,
    /// The action was confirmed.
    Yes,
    /// The action was dropped.
    No,
}

/// Asks once, in front of the rest of the interface.
///
/// Escape, a click beside the window and the second button all answer `No`,
/// Enter and the first button answer `Yes`.
pub fn ask(context: &egui::Context, id: &str, question: &str, detail: &str) -> Answer {
    let mut answer = Answer::Pending;

    let modal = egui::Modal::new(egui::Id::new(id)).show(context, |ui| {
        ui.set_width(320.0);
        ui.heading(question);
        if !detail.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(detail).strong());
        }
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.button(t!("confirm.delete")).clicked() {
                answer = Answer::Yes;
            }
            if ui.button(t!("confirm.cancel")).clicked() {
                answer = Answer::No;
            }
        });
    });

    if modal.backdrop_response.clicked() {
        answer = Answer::No;
    }
    if context.input(|input| input.key_pressed(egui::Key::Escape)) {
        answer = Answer::No;
    }
    if context.input(|input| input.key_pressed(egui::Key::Enter)) {
        answer = Answer::Yes;
    }

    answer
}
