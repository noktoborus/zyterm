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

/// How tall a list of several is allowed to grow before it scrolls.
const LIST_MOST: f32 = 160.0;

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

    egui::Window::new(title)
        .id(base)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(context, |ui| {
            if let Some(hint) = state.form.hint.clone() {
                ui.label(egui::RichText::new(hint).weak());
                ui.add_space(6.0);
            }

            let grid = egui::Grid::new(base.with("grid"))
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for index in 0..state.form.fields.len() {
                        let field = state.form.fields[index].clone();
                        let mut wrote = Wrote {
                            fields: &mut fields,
                            entered: &mut entered,
                            asked_for: &mut asked_for,
                        };
                        row(ui, &mut state, &field, index, base, &mut wrote);
                    }
                });

            ui.add_space(10.0);
            let (pressed, id) = buttons(
                ui,
                &state.form,
                &mut state.always_ask,
                base,
                grid.response.rect.width(),
            );
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

    let label = match field.label.is_empty() {
        true => field.name.clone(),
        false => field.label.clone(),
    };
    let mut text = egui::RichText::new(label);
    if field.required {
        text = text.strong();
    }
    let labelled = ui.label(text);
    if let Some(hint) = &field.hint {
        labelled.on_hover_text(hint.clone());
    }

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
        FieldKind::ManyOf { options, .. } => {
            let mut chosen: Vec<String> = state
                .values
                .get(&field.name)
                .map(Value::names)
                .unwrap_or_default();
            let mut changed = false;
            crate::ui::widgets::scroll_area(ui)
                .id_salt(id)
                .max_height(LIST_MOST)
                .show(ui, |ui| {
                    for choice in options {
                        let mut on = chosen.contains(&choice.id);
                        if ui.selectable_label(on, label_of(choice)).clicked() {
                            on = !on;
                            changed = true;
                            match on {
                                true => chosen.push(choice.id.clone()),
                                false => chosen.retain(|name| *name != choice.id),
                            }
                        }
                    }
                });
            if changed {
                state.values.insert(field.name.clone(), Value::Many(chosen));
            }
        }
        FieldKind::Separator => {}
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

/// The two ways out of the window, and the switch that decides whether it
/// comes up again.
///
/// The one that goes on is answered first, so the last field hands it the
/// keyboard and `Enter` presses it: a dialog of three values is answered
/// without the hands leaving the keys.
///
/// Three cells of one width: nothing, the two buttons, the switch. The buttons
/// stand in the middle of the window and the switch beside them rather than
/// below the fields, because it is about this window and not about one of its
/// rows — and what it is turned to is kept with the answers. A form the script
/// does not keep has no switch at all: there is nothing for it to decide.
fn buttons(
    ui: &mut egui::Ui,
    form: &Form,
    always_ask: &mut bool,
    base: egui::Id,
    under: f32,
) -> (Option<bool>, egui::Id) {
    let height = ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y;
    let third = (under / 3.0).max(ui.spacing().interact_size.x);
    let accept = form
        .accept
        .clone()
        .unwrap_or_else(|| t!("dialog.ok").to_string());
    let reject = form
        .reject
        .clone()
        .unwrap_or_else(|| t!("dialog.cancel").to_string());

    let mut pressed = None;
    let mut id = None;
    egui::Grid::new(base.with("ways"))
        .num_columns(3)
        .min_col_width(third)
        .show(ui, |ui| {
            ui.label("");

            ui.allocate_ui_with_layout(
                egui::vec2(third, height),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let response = ui.button(accept);
                    id = Some(response.id);
                    if response.clicked() {
                        pressed = Some(true);
                    }
                    if ui.button(reject).clicked() {
                        pressed = Some(false);
                    }
                },
            );

            if form.unsaved {
                ui.label("");
            } else {
                ui.horizontal(|ui| {
                    crate::ui::widgets::switch(ui, always_ask)
                        .on_hover_text(t!("dialog.always_ask_hint"));
                    ui.label(t!("dialog.always_ask"))
                        .on_hover_text(t!("dialog.always_ask_hint"));
                });
            }
            ui.end_row();
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
