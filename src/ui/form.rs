//! The window a script asks a question with.
//!
//! A script describes what it wants to know and waits on one call; this is
//! what draws that description. Nothing of the toolkit reaches the script: it
//! named the fields, and what comes back is a value per name.
//!
//! The window does not move and what is behind it answers nothing while it
//! stands, because it is the one thing being answered. The two ways out of it
//! — the button that gives up and `Esc` — are the same answer, and a script
//! told its question was waved away is expected to stop.

use crate::app::App;
use rust_i18n::t;
use std::collections::BTreeMap;
use zyt_script::{Choice, Field, FieldKind, Form, Value};

/// The narrowest a field of the window is, which is what an empty one shows.
const VALUE_LEAST: usize = 24;

/// The widest it grows to as it is written in.
const VALUE_MOST: usize = 64;

/// How wide the column of labels grows before a label wraps.
///
/// A label is a sentence as often as a word — a script says what a field is
/// for in it — and one kept to a single line makes the window as wide as the
/// longest sentence in it, with every field pushed off to the right of that.
/// So it wraps, and the column is this wide at the most.
const LABEL_MOST: usize = 32;

/// How much of the window is left around a dialog, each side of it.
///
/// The dialog grows with what it asks and stops at the window: past that there
/// is nothing to grow into, and a row below the edge is a row nobody can
/// answer. What is left is a margin and not a share, so a form of three rows is
/// three rows tall and one of thirty is as tall as the window allows and
/// scrolls.
const ROOM_MARGIN: f32 = 24.0;

/// A dialog standing, with what has been answered so far.
pub struct State {
    /// What the script asked.
    pub form: Form,
    /// What stands in the fields now.
    pub values: BTreeMap<String, Value>,
    /// True while this source, this script and this form are to be asked every
    /// time.
    pub always_ask: bool,
    /// Which source the answers belong to, when there is one.
    key: Option<crate::sources::SourceKey>,
    /// The script the answers belong to.
    script: String,
    /// The name the answers of this form are kept under.
    form_id: String,
    /// Where the answer goes.
    pending: crate::scripts::Pending,
    /// True until the first field has been given the keyboard.
    opening: bool,
}

impl State {
    /// A dialog for what a script is waiting on, with what it was answered
    /// before already in the fields.
    pub fn new(
        pending: crate::scripts::Pending,
        key: Option<crate::sources::SourceKey>,
        script: String,
        form_id: String,
        saved: Option<crate::forms::Answered>,
        always_ask: bool,
    ) -> Self {
        let form = pending.form.clone();
        let mut values = form.defaults();
        if let Some(saved) = &saved {
            for (name, value) in &saved.values {
                if values.contains_key(name) {
                    values.insert(name.clone(), value.clone());
                }
            }
        }

        Self {
            form,
            values,
            always_ask,
            key,
            script,
            form_id,
            pending,
            opening: true,
        }
    }

    /// Picks one entry of a list, for the menu that offers them.
    pub fn pick(&mut self, field: &str, id: &str) {
        let Some(found) = self.form.field(field) else {
            return;
        };
        match &found.kind {
            FieldKind::Select { .. } | FieldKind::OneOf { .. } => {
                self.values
                    .insert(field.to_string(), Value::One(id.to_string()));
            }
            _ => {}
        }
    }

    /// What one list field stands on now.
    pub fn picked(&self, field: &str) -> String {
        self.values.get(field).map(Value::text).unwrap_or_default()
    }
}

/// Draws the window, while there is one.
pub fn draw(app: &mut App, context: &egui::Context) {
    let Some(mut state) = app.ui.form.take() else {
        return;
    };

    let mut open = true;
    let mut answered = None;
    let mut fields: Vec<egui::Id> = Vec::new();
    let mut entered = None;
    let mut accept = None;
    let mut asked_for = None;
    let base = egui::Id::new("script-form");
    let title = match state.form.title.is_empty() {
        true => t!("dialog.title").to_string(),
        false => state.form.title.clone(),
    };

    let room = room_of(context);

    egui::Window::new(title)
        .id(base)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .max_size(room)
        .open(&mut open)
        .show(context, |ui| {
            ui.set_min_width(footer_width(ui, &state.form));

            let mut spent = 0.0;
            if let Some(hint) = state.form.hint.clone() {
                spent += ui.label(egui::RichText::new(hint).weak()).rect.height();
                ui.add_space(6.0);
                spent += 6.0;
            }

            crate::ui::widgets::scroll_area_both(ui)
                .id_salt(base.with("rows"))
                .max_height(rows_most(ui, room.y, spent))
                .max_width(room.x)
                .show(ui, |ui| {
                    let mut wrote = Wrote {
                        fields: &mut fields,
                        entered: &mut entered,
                        asked_for: &mut asked_for,
                    };
                    rows(ui, &mut state, base, &mut wrote);
                });

            ui.separator();
            let (pressed, id) = buttons(ui, &state.form, &mut state.always_ask);
            accept = Some(id);
            if let Some(yes) = pressed {
                answered = Some(yes);
            }
        });

    if let Some(id) = next_focus(&mut state, entered, &fields, accept) {
        context.memory_mut(|memory| memory.request_focus(id));
    }

    let escaped =
        context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if !open || escaped {
        state.pending.answer(None);
        return;
    }

    match answered {
        Some(true) => {
            if state.form.check(&state.values).is_err() {
                app.ui.form = Some(state);
                return;
            }
            keep(app, &state);
            let values = state.values.clone();
            state.pending.answer(Some(values));
        }
        Some(false) => state.pending.answer(None),
        None => {
            app.ui.form = Some(state);
            // The window is standing again before the list of one of its rows
            // is asked for: the menu is built from the dialog, and while the
            // dialog is being drawn there is none to build it from.
            if let Some(field) = asked_for {
                app.open_form_menu(&field);
            }
        }
    }
}

/// What the rows of one window write down as they are drawn: which fields took
/// the keyboard, which of them was left with `Enter`, and which list was asked
/// for.
struct Wrote<'a> {
    /// The identifier of every field that takes the keyboard, in order.
    fields: &'a mut Vec<egui::Id>,
    /// Which of them `Enter` was pressed in.
    entered: &'a mut Option<usize>,
    /// The row whose list of values is to be offered, when one was pressed.
    asked_for: &'a mut Option<String>,
}

/// Draws the fields the window asks with.
///
/// A run of ordinary fields is a grid of two columns, the labels in one and
/// what answers them in the other. A list of several answers is none of that:
/// it is read down, and the names in it are as long as whatever named them, so
/// a cell of the grid would be a column every label of the window is measured
/// against. It stands across the whole width instead, with its label above it,
/// which is also why the grids come one per run — what lies between two lists
/// is a grid of its own.
fn rows(ui: &mut egui::Ui, state: &mut State, base: egui::Id, wrote: &mut Wrote<'_>) {
    let count = state.form.fields.len();
    let mut at = 0;

    while at < count {
        if matches!(state.form.fields[at].kind, FieldKind::ManyOf { .. }) {
            let field = state.form.fields[at].clone();
            listed(ui, state, &field);
            at += 1;
            continue;
        }

        let from = at;
        while at < count && !matches!(state.form.fields[at].kind, FieldKind::ManyOf { .. }) {
            at += 1;
        }

        egui::Grid::new(base.with(("grid", from)))
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for index in from..at {
                    let field = state.form.fields[index].clone();
                    row(ui, state, &field, index, base, wrote);
                }
            });
    }
}

/// What a row is called: its label, or its name where it has none, and strong
/// when it has to be answered.
fn label_text(field: &Field) -> egui::RichText {
    let label = match field.label.is_empty() {
        true => field.name.clone(),
        false => field.label.clone(),
    };
    let mut text = egui::RichText::new(label);
    if field.required {
        text = text.strong();
    }
    text
}

/// The label of a row, kept to the width of its column and wrapped past it.
///
/// What it says about itself is on the pointer, where the label is cut as well
/// as where it is whole: a wrapped sentence is read, a cut one is guessed at.
fn labelled(ui: &mut egui::Ui, field: &Field) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.set_max_width(crate::ui::widgets::monospace_width(ui, LABEL_MOST));
            ui.add(egui::Label::new(label_text(field)).wrap())
        })
        .inner;

    match &field.hint {
        Some(hint) => response.on_hover_text(hint.clone()),
        None => response,
    }
}

/// One list of several answers: the label above it, the entries under it, and
/// the whole of it as wide as the window has come to.
///
/// The frame is what says where the list ends, a column of boxes to tick being
/// as ragged as the names in it. It is held to the width of what stands above
/// it rather than to the room of the window, so the list follows the window
/// instead of deciding how wide it is.
fn listed(ui: &mut egui::Ui, state: &mut State, field: &Field) {
    let FieldKind::ManyOf { options, .. } = &field.kind else {
        return;
    };
    let options = options.clone();
    let mut chosen: Vec<String> = state
        .values
        .get(&field.name)
        .map(Value::names)
        .unwrap_or_default();

    let said = ui.label(label_text(field));
    if let Some(hint) = &field.hint {
        said.on_hover_text(hint.clone());
    }

    let span = ui.min_rect().width();
    let mut changed = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let inside = span - ui.spacing().item_spacing.x * 2.0;
        if inside > 0.0 {
            ui.set_min_width(inside);
        }
        for choice in &options {
            let mut on = chosen.contains(&choice.id);
            let picked = ui.checkbox(&mut on, label_of(choice));
            let picked = match &choice.hint {
                Some(hint) => picked.on_hover_text(hint.clone()),
                None => picked,
            };
            if picked.changed() {
                changed = true;
                match on {
                    true => chosen.push(choice.id.clone()),
                    false => chosen.retain(|name| *name != choice.id),
                }
            }
        }
    });
    ui.add_space(4.0);

    if changed {
        state.values.insert(field.name.clone(), Value::Many(chosen));
    }
}

/// Draws one row of the window.
fn row(
    ui: &mut egui::Ui,
    state: &mut State,
    field: &Field,
    index: usize,
    base: egui::Id,
    wrote: &mut Wrote<'_>,
) {
    if matches!(field.kind, FieldKind::Separator) {
        ui.separator();
        ui.separator();
        ui.end_row();
        return;
    }

    labelled(ui, field);

    let id = base.with(("field", index));
    match &field.kind {
        FieldKind::Note => {
            ui.label("");
        }
        FieldKind::Text { password, .. } => {
            wrote.fields.push(id);
            let mut value = state.picked(&field.name);
            let width = crate::ui::widgets::written_width(
                ui,
                &value,
                VALUE_LEAST,
                crate::ui::widgets::monospace_width(ui, VALUE_MOST),
            );
            let edit = egui::TextEdit::singleline(&mut value)
                .id(id)
                .password(*password)
                .hint_text(field.name.clone())
                .font(egui::TextStyle::Monospace);
            let response = crate::ui::widgets::sized_field(ui, width, edit);
            if response.changed() {
                state.values.insert(field.name.clone(), Value::Text(value));
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                *wrote.entered = Some(wrote.fields.len() - 1);
            }
        }
        FieldKind::Textarea { rows, .. } => {
            wrote.fields.push(id);
            let mut value = state.picked(&field.name);
            let focused = ui.memory(|memory| memory.has_focus(id));
            let written = value.lines().count().max(*rows);
            let tall = if focused { written.max(2) } else { written };
            let response = ui.add(
                egui::TextEdit::multiline(&mut value)
                    .id(id)
                    .desired_rows(tall)
                    .desired_width(crate::ui::widgets::monospace_width(ui, VALUE_MOST))
                    .font(egui::TextStyle::Monospace),
            );
            if response.changed() {
                state.values.insert(field.name.clone(), Value::Text(value));
            }
        }
        FieldKind::Switch { .. } => {
            let mut on = state
                .values
                .get(&field.name)
                .map(Value::flag)
                .unwrap_or(false);
            if crate::ui::widgets::switch(ui, &mut on).changed() {
                state.values.insert(field.name.clone(), Value::Flag(on));
            }
        }
        FieldKind::OneOf { options, .. } | FieldKind::Select { options, .. } => {
            let current = state.picked(&field.name);
            let shown = options
                .iter()
                .find(|choice| choice.id == current)
                .map(label_of)
                .unwrap_or_else(|| t!("dialog.pick").to_string());
            if ui.button(shown).clicked() {
                *wrote.asked_for = Some(field.name.clone());
            }
        }
        FieldKind::ManyOf { .. } | FieldKind::Separator => {}
    }
    ui.end_row();
}

/// What an entry of a list is called.
fn label_of(choice: &Choice) -> String {
    match choice.label.is_empty() {
        true => choice.id.clone(),
        false => choice.label.clone(),
    }
}

/// Writes down what was answered, so the next run of this form of this script
/// on this source comes up with it — or does not come up at all.
///
/// A form the script marked as not to be kept is written nowhere: what it asks
/// is about the device as it stands now, and an answer about a moment is no
/// answer to the next one.
fn keep(app: &mut App, state: &State) {
    let Some(key) = &state.key else {
        return;
    };
    if state.script.is_empty() || state.form.unsaved {
        return;
    }

    let outcome = crate::forms::save(
        &app.store,
        key,
        &state.script,
        &state.form_id,
        &state.values,
        state.always_ask,
    );
    app.report(outcome);
}

/// The room a dialog has to grow into: the window, less the margin around it.
fn room_of(context: &egui::Context) -> egui::Vec2 {
    let window = context.viewport_rect().size();
    egui::vec2(
        (window.x - ROOM_MARGIN * 2.0).max(ROOM_MARGIN),
        (window.y - ROOM_MARGIN * 2.0).max(ROOM_MARGIN),
    )
}

/// How tall the rows of the window may stand before they scroll.
///
/// What is left of the room once the hint above them and the row of buttons
/// under them have had theirs: both are always there to be read, so neither is
/// what scrolls. The floor is three rows, because a window showing none of
/// what it asks is a window nobody can answer.
fn rows_most(ui: &egui::Ui, room: f32, spent: f32) -> f32 {
    let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
    (room - spent - row * 3.0).max(row * 3.0)
}

/// What the two ways out say: what the script called them, or what this window
/// calls them when it was told nothing.
fn ways_out(form: &Form) -> (String, String) {
    let accept = form
        .accept
        .clone()
        .unwrap_or_else(|| t!("dialog.ok").to_string());
    let reject = form
        .reject
        .clone()
        .unwrap_or_else(|| t!("dialog.cancel").to_string());
    (accept, reject)
}

/// The width the row that ends the window takes, which is the narrowest the
/// window is.
///
/// A row laid out by what its widgets take is a row that is squeezed when what
/// stands above it is narrower than it is: a form of one short field would put
/// two buttons and a switch in the width of that field. What the window is
/// answered with is not what gives way, so the window is held to this width
/// and the rows above it have the room of it.
fn footer_width(ui: &egui::Ui, form: &Form) -> f32 {
    let (accept, reject) = ways_out(form);
    let spacing = ui.spacing().item_spacing.x;
    let mut width = crate::ui::widgets::button_width(ui, &accept)
        + spacing
        + crate::ui::widgets::button_width(ui, &reject);

    if !form.unsaved {
        let said = t!("dialog.always_ask").to_string();
        width += ui.spacing().interact_size.y * 2.0
            + spacing
            + crate::ui::widgets::label_width(ui, &[&said])
            + spacing;
    }
    width
}

/// The two ways out of the window, and the switch that decides whether it
/// comes up again.
///
/// The one that goes on is answered first, so the last field hands it the
/// keyboard and `Enter` presses it: a dialog of three values is answered
/// without the hands leaving the keys.
///
/// One row under the rows of the window: the switch where reading starts and
/// the two ways out at the end of it, the one that goes on last of all. It is
/// laid out by what each of them takes and not by a measured share of the
/// width — three cells of a third each put two buttons in the room of one and
/// drew them over each other as soon as their words were longer than that
/// third. The switch stands here rather than below the fields because it is
/// about this window and not about one of its rows, and what it is turned to is
/// kept with the answers. A form the script does not keep has no switch at
/// all: there is nothing for it to decide.
fn buttons(ui: &mut egui::Ui, form: &Form, always_ask: &mut bool) -> (Option<bool>, egui::Id) {
    let (accept, reject) = ways_out(form);

    let mut pressed = None;
    let mut id = None;
    ui.horizontal(|ui| {
        if !form.unsaved {
            crate::ui::widgets::switch(ui, always_ask).on_hover_text(t!("dialog.always_ask_hint"));
            ui.label(t!("dialog.always_ask"))
                .on_hover_text(t!("dialog.always_ask_hint"));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = ui.button(accept);
            id = Some(response.id);
            if response.clicked() {
                pressed = Some(true);
            }
            if ui.button(reject).clicked() {
                pressed = Some(false);
            }
        });
    });

    (pressed, id.unwrap_or_else(|| ui.id().with("accept")))
}

/// The field or the button the keyboard goes to next.
fn next_focus(
    state: &mut State,
    entered: Option<usize>,
    fields: &[egui::Id],
    accept: Option<egui::Id>,
) -> Option<egui::Id> {
    if std::mem::take(&mut state.opening) {
        return fields.first().copied().or(accept);
    }

    let index = entered?;
    fields.get(index + 1).copied().or(accept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        }
    }

    /// The ways out stand side by side in one row, so neither of them is laid
    /// in the room of the other: two buttons stacked inside a cell measured
    /// for one were drawn over each other as soon as their words were longer
    /// than that cell.
    #[test]
    fn the_two_ways_out_of_the_window_stand_in_one_row() {
        let context = egui::Context::default();
        let form = Form {
            accept: Some("Take them off the device".to_string()),
            reject: Some("Leave the device alone".to_string()),
            ..Form::default()
        };
        let mut always_ask = true;
        let mut footer = egui::Rect::NOTHING;
        let mut row = 0.0;
        let mut wanted = 0.0;

        let mut output = context.run_ui(input(), |ui| {
            footer = ui
                .vertical(|ui| {
                    buttons(ui, &form, &mut always_ask);
                })
                .response
                .rect;
            row = ui.spacing().interact_size.y;
            wanted = crate::ui::widgets::button_width(ui, "Take them off the device")
                + crate::ui::widgets::button_width(ui, "Leave the device alone");
        });
        output.textures_delta.clear();

        assert!(
            footer.height() <= row * 1.5,
            "one row of buttons and not two: {} against {row}",
            footer.height()
        );
        assert!(
            footer.width() >= wanted,
            "both of them fit: {} against {wanted}",
            footer.width()
        );
    }

    /// The rows take what the room leaves them once the hint and the row of
    /// buttons have had theirs, and never less than three rows of it.
    #[test]
    fn the_rows_take_what_is_left_of_the_room() {
        let context = egui::Context::default();

        let mut output = context.run_ui(input(), |ui| {
            let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            let tall = rows_most(ui, 40.0 * row, 0.0);
            let under = rows_most(ui, 40.0 * row, 10.0 * row);
            let squeezed = rows_most(ui, row, 0.0);

            assert!(tall < 40.0 * row, "the buttons under them keep theirs");
            assert!(under < tall, "what the hint takes is not theirs");
            assert_eq!(squeezed, row * 3.0, "three rows whatever the room");
        });
        output.textures_delta.clear();
    }

    /// A label is a sentence as often as a word, so it wraps instead of making
    /// the window as wide as the longest of them.
    #[test]
    fn a_label_longer_than_its_column_wraps() {
        let context = egui::Context::default();
        let short = Field {
            name: "chunk".to_string(),
            label: "Chunk".to_string(),
            hint: None,
            required: false,
            kind: FieldKind::Switch { value: false },
        };
        let long = Field {
            label: "A label long enough to measure the column of labels with".to_string(),
            ..short.clone()
        };

        let mut output = context.run_ui(input(), |ui| {
            let one = labelled(ui, &short).rect;
            let many = labelled(ui, &long).rect;
            let column = crate::ui::widgets::monospace_width(ui, LABEL_MOST);

            assert!(
                many.height() > one.height(),
                "it is laid out over more than one line: {} against {}",
                many.height(),
                one.height()
            );
            assert!(
                many.width() <= column,
                "and inside the column: {} against {column}",
                many.width()
            );
        });
        output.textures_delta.clear();
    }

    /// The window is held to the width of the row that ends it, so that row is
    /// never squeezed: both ways out are in that width, and the switch of a
    /// form whose answers are kept is in it as well.
    #[test]
    fn the_row_that_ends_the_window_is_measured_whole() {
        let context = egui::Context::default();
        let accept = "Take them off the device";
        let reject = "Leave the device alone";
        let form = Form {
            accept: Some(accept.to_string()),
            reject: Some(reject.to_string()),
            ..Form::default()
        };
        let moment = Form {
            unsaved: true,
            ..form.clone()
        };

        let mut output = context.run_ui(input(), |ui| {
            let ways = crate::ui::widgets::button_width(ui, accept)
                + crate::ui::widgets::button_width(ui, reject);
            let kept = footer_width(ui, &form);
            let unsaved = footer_width(ui, &moment);

            assert!(unsaved >= ways, "{unsaved} against {ways}");
            assert!(kept > unsaved, "the switch takes its own room");
        });
        output.textures_delta.clear();
    }
}
