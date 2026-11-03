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
use crate::ui::access::Named;
use rust_i18n::t;
use std::collections::BTreeMap;
use zyt_script::{Choice, Field, FieldKind, Form, Value};

/// The narrowest a field of the window is, which is what an empty one shows.
const VALUE_LEAST: usize = 24;

/// The fewest lines a field of several lines shows, whatever stands in it.
const BLOCK_ROWS_LEAST: usize = 5;

/// The most it grows to as lines are written; past that its text scrolls
/// inside it and the window stays as tall as it was.
const BLOCK_ROWS_MOST: usize = 10;

/// The room between the text of a field of several lines and its edge, set
/// rather than taken from the toolkit so its height is known before it is drawn.
const BLOCK_MARGIN: egui::Margin = egui::Margin::symmetric(4, 2);

/// The room the edge of a field of several lines is drawn in, and the room
/// left around it: an edge drawn against the edge of what scrolls is cut off
/// with it.
const BLOCK_EDGE: egui::Margin = egui::Margin::same(1);

/// The room between the column of labels and the column of values.
const COLUMNS_APART: f32 = 12.0;

/// How wide the column of labels grows before a label wraps.
///
/// A label is a sentence as often as a word — a script says what a field is
/// for in it — and one kept to a single line makes the window as wide as the
/// longest sentence in it, with every field pushed off to the right of that.
/// So it wraps, and the column is this wide at the most.
const LABEL_MOST: usize = 32;

/// How much of the window is left above and below a dialog.
///
/// The dialog grows with what it asks and stops at the window: past that there
/// is nothing to grow into, and a row below the edge is a row nobody can
/// answer. What is left is a margin and not a share, so a form of three rows is
/// three rows tall and one of thirty is as tall as the window allows and
/// scrolls.
///
/// Nothing is left either side of it: a row is a label and a value, and the
/// width of the two of them is the one thing a dialog cannot give up. So it may
/// grow to the full width of the window, and what is wider than that scrolls.
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
    /// How wide the rows came out the frame before.
    ///
    /// The window is as wide as what stands in it, and the rows scroll inside
    /// what the window offers, so neither of the two would ever grow, and the
    /// row of buttons takes whatever it is offered, so the window would never
    /// narrow: the window is set to this width instead.
    rows_width: f32,
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
        let form = pending.question.clone();
        let values = crate::forms::filled(&form, saved.as_ref());

        Self {
            form,
            values,
            always_ask,
            key,
            script,
            form_id,
            pending,
            opening: true,
            rows_width: 0.0,
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
    // One row of a form is answered by the dialog of the file system, which is
    // a view of the whole window and not a window over it. While it stands the
    // form waits where it is, with everything that has been answered so far,
    // and is drawn again when the view it came from returns.
    if app.ui.view == crate::app::MainView::FileDialog {
        return;
    }

    let Some(mut state) = app.ui.form.take() else {
        return;
    };
    // A dialog outlives nothing that asked it: a run that was stopped or cut
    // off by a disconnect while its question stood is not there to be told.
    if state
        .pending
        .run()
        .is_some_and(|run| app.session.run_number() != Some(run))
    {
        state.pending.answer(None);
        return;
    }

    let mut open = true;
    let mut answered = None;
    let mut fields: Vec<egui::Id> = Vec::new();
    let mut entered = None;
    let mut accept = None;
    let mut asked_for = None;
    let mut asked_path = None;
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
        .auto_sized()
        .collapsible(false)
        .max_size(room)
        .open(&mut open)
        .show(context, |ui| {
            let inside = room.x - egui::Frame::window(ui.style()).total_margin().sum().x;
            ui.set_width(footer_width(ui, &state.form).max(state.rows_width.min(inside)));

            let mut spent = 0.0;
            if let Some(hint) = state.form.hint.clone() {
                spent += ui.label(egui::RichText::new(hint).weak()).rect.height();
                ui.add_space(6.0);
                spent += 6.0;
            }

            let most = value_most(ui, &state.form, room.x);
            let shown = crate::ui::widgets::scroll_area_both(ui)
                .id_salt(base.with("rows"))
                .max_height(rows_most(ui, room.y, spent))
                .max_width(room.x)
                .show(ui, |ui| {
                    let mut wrote = Wrote {
                        fields: &mut fields,
                        entered: &mut entered,
                        asked_for: &mut asked_for,
                        asked_path: &mut asked_path,
                    };
                    rows(ui, &mut state, base, most, &mut wrote);
                });
            if shown.content_size.x != state.rows_width {
                state.rows_width = shown.content_size.x;
                ui.ctx().request_discard("the rows of a form changed width");
            }

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
            if let Some((field, directory)) = asked_path {
                app.pick_form_path(&field, directory);
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
    /// The row whose path is to be picked, and whether it is a directory, when
    /// one was pressed.
    asked_path: &'a mut Option<(String, bool)>,
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
fn rows(ui: &mut egui::Ui, state: &mut State, base: egui::Id, most: f32, wrote: &mut Wrote<'_>) {
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
            .spacing([COLUMNS_APART, 6.0])
            .show(ui, |ui| {
                for index in from..at {
                    let field = state.form.fields[index].clone();
                    row(ui, state, &field, index, base, most, wrote);
                }
            });
    }
}

/// A field of several lines, as wide as its longest line up to `most` and as
/// tall as its lines up to `BLOCK_ROWS_MOST`, its text scrolled inside it past
/// either.
///
/// The edge of the field stands still and the text moves inside it: the field
/// is drawn with no frame of its own inside the area that scrolls, and the
/// frame — its fill, its edge, the edge of the focus — is drawn around that
/// area, the way the toolkit draws it around a field.
///
/// Its lines are not wrapped: a block of a device — a configuration, a script
/// — is read as the lines it will be sent as, and one wrapped at the width of
/// a field is a block that reads otherwise than it runs. A bar is there only
/// while the text goes past an edge, solid while it is, and its handle drawn
/// in the colour of text: a bar that shows only under the pointer does not say
/// there is more past the edge, and a handle in the colour of a widget is lost
/// on the fill of the field.
fn text_block(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut String,
    rows: usize,
    most: f32,
) -> egui::Response {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let tallest = row * BLOCK_ROWS_MOST as f32 + BLOCK_MARGIN.sum().y;
    let bar = ui.spacing().scroll.allocated_width();
    let edge = BLOCK_EDGE.sum();
    let written = crate::ui::widgets::written_width(ui, value, VALUE_LEAST, f32::INFINITY);
    let wide = (written + bar).min(most - edge.x * 2.0);
    let mut unwrapped = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
        let color = ui.visuals().widgets.inactive.text_color();
        let job = egui::text::LayoutJob::simple(
            text.as_str().to_owned(),
            font.clone(),
            color,
            f32::INFINITY,
        );
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let edit = egui::TextEdit::multiline(value)
        .id(id)
        .desired_rows(rows.clamp(BLOCK_ROWS_LEAST, BLOCK_ROWS_MOST))
        .desired_width(written)
        .frame(egui::Frame::new().inner_margin(BLOCK_MARGIN))
        .font(egui::TextStyle::Monospace)
        .layouter(&mut unwrapped);

    ui.allocate_ui_with_layout(
        egui::vec2(wide, tallest + bar) + edge * 2.0,
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.style_mut().spacing.scroll = egui::style::ScrollStyle {
                foreground_color: true,
                ..egui::style::ScrollStyle::solid()
            };
            let mut framed = egui::Frame::new()
                .fill(ui.visuals().text_edit_bg_color())
                .corner_radius(ui.visuals().widgets.inactive.corner_radius)
                .inner_margin(BLOCK_EDGE)
                .outer_margin(BLOCK_EDGE)
                .begin(ui);
            let response = crate::ui::widgets::scroll_area_both(&framed.content_ui)
                .id_salt(id.with("scrolled"))
                .max_width(wide)
                .max_height(tallest)
                .show(&mut framed.content_ui, |ui| ui.add(edit))
                .inner;
            framed.frame.stroke = match response.has_focus() {
                true => ui.visuals().selection.stroke,
                false => ui.style().interact(&response).bg_stroke,
            };
            framed.end(ui);
            response
        },
    )
    .inner
}

/// The widest a value of the window grows to: the room less the frame of the
/// window, the column of labels, the room between the two columns and the bar
/// the rows scroll by when they are taller than the room.
///
/// It is worked out from the room and not from what the window offers, because
/// the window is as wide as what stands in it: a value that asked it would be
/// handed back the width it already had.
fn value_most(ui: &egui::Ui, form: &Form, room: f32) -> f32 {
    let frame = egui::Frame::window(ui.style()).total_margin().sum().x;
    let labels = labels_width(ui, form);
    let bar = ui.spacing().scroll.allocated_width();
    let least = crate::ui::widgets::monospace_width(ui, VALUE_LEAST);
    (room - frame - labels - COLUMNS_APART - bar).max(least)
}

/// How wide the column of labels comes out: the widest label up to the width
/// it wraps at, or the button of a path, which stands in that column.
fn labels_width(ui: &egui::Ui, form: &Form) -> f32 {
    let wraps_at = crate::ui::widgets::monospace_width(ui, LABEL_MOST);
    form.fields
        .iter()
        .map(|field| {
            let label = label_text(field).text().to_string();
            match field.kind {
                FieldKind::Path { .. } => crate::ui::widgets::button_width(ui, &label),
                FieldKind::ManyOf { .. } | FieldKind::Separator => 0.0,
                _ => crate::ui::widgets::label_width(ui, &[&label]).min(wraps_at),
            }
        })
        .fold(0.0, f32::max)
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

/// Draws one row of the window; a value grows with what is written in it up to
/// `most`.
fn row(
    ui: &mut egui::Ui,
    state: &mut State,
    field: &Field,
    index: usize,
    base: egui::Id,
    most: f32,
    wrote: &mut Wrote<'_>,
) {
    if matches!(field.kind, FieldKind::Separator) {
        ui.separator();
        ui.separator();
        ui.end_row();
        return;
    }

    if let FieldKind::Path { directory, .. } = field.kind {
        path_row(ui, state, field, directory, wrote);
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
            let width = crate::ui::widgets::written_width(ui, &value, VALUE_LEAST, most);
            let edit = egui::TextEdit::singleline(&mut value)
                .id(id)
                .password(*password)
                .hint_text(field.name.clone())
                .font(egui::TextStyle::Monospace);
            let response =
                crate::ui::widgets::sized_field(ui, width, edit).named(label_text(field).text());
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
            let response =
                text_block(ui, id, &mut value, *rows, most).named(label_text(field).text());
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
            if crate::ui::widgets::switch(ui, &mut on, label_text(field).text()).changed() {
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
        FieldKind::Path { .. } | FieldKind::ManyOf { .. } | FieldKind::Separator => {}
    }
    ui.end_row();
}

/// One path of the file system: the button that asks for it, and what it stands
/// on now.
///
/// The button is in the column of the labels and carries what the script called
/// the row, because a label beside a button that says the same thing says it
/// twice. It asks for the path with the dialog of the file system and never for
/// one typed: a path typed by hand is a path that is wrong by one letter, and
/// the dialog is the one thing in the program that knows what is there.
///
/// What was picked stands beside it, in the font a path is read in and whole on
/// the pointer, because the room of a row is not the length of a path. A row
/// nobody has answered says so there instead.
fn path_row(
    ui: &mut egui::Ui,
    state: &mut State,
    field: &Field,
    directory: bool,
    wrote: &mut Wrote<'_>,
) {
    let pressed = ui.button(label_text(field));
    let pressed = match &field.hint {
        Some(hint) => pressed.on_hover_text(hint.clone()),
        None => pressed,
    };
    if pressed.clicked() {
        *wrote.asked_path = Some((field.name.clone(), directory));
    }

    let picked = state.picked(&field.name);
    if picked.is_empty() {
        ui.label(egui::RichText::new(t!("dialog.no_path")).weak());
        return;
    }
    ui.add(
        egui::Label::new(egui::RichText::new(&picked).monospace())
            .truncate()
            .sense(egui::Sense::hover()),
    )
    .on_hover_text(picked);
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

/// The room a dialog has to grow into: the whole width of the window, and its
/// height less the margin above and below.
fn room_of(context: &egui::Context) -> egui::Vec2 {
    let window = context.viewport_rect().size();
    egui::vec2(window.x, (window.y - ROOM_MARGIN * 2.0).max(ROOM_MARGIN))
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

    if !form.unsaved && !form.always {
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
/// all: there is nothing for it to decide. Nor has one the script wants seen
/// every time: it is always asked, and the switch would offer the opposite.
fn buttons(ui: &mut egui::Ui, form: &Form, always_ask: &mut bool) -> (Option<bool>, egui::Id) {
    let (accept, reject) = ways_out(form);

    let mut pressed = None;
    let mut id = None;
    ui.horizontal(|ui| {
        if !form.unsaved && !form.always {
            crate::ui::widgets::switch(ui, always_ask, &t!("dialog.always_ask"))
                .on_hover_text(t!("dialog.always_ask_hint"));
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

    /// A dialog may grow to the width of the window and stops short of its
    /// height, so what is above and below it is still there to be read.
    #[test]
    fn a_dialog_grows_to_the_width_of_the_window() {
        let context = egui::Context::default();
        let mut output = context.run_ui(input(), |ui| {
            let window = ui.ctx().viewport_rect().size();
            let room = room_of(ui.ctx());

            assert_eq!(room.x, window.x);
            assert!(room.y < window.y);
        });
        output.textures_delta.clear();
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
            fresh: false,
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
