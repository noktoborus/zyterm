//! The search bar, in place of the status bar.
//!
//! One button says how the query is read and opens the menu of the four ways of
//! reading it, two switches stand beside it, the two steps of the search stand right of the
//! field, and the field takes what is left of the width. Every change goes to
//! the terminal at once, so the marks on the screen always answer what the bar
//! shows, and to the settings, so they answer it after the next start as well.
//!
//! The bar is walked with Tab and shift+Tab like any other row of controls. The
//! keyboard cannot leave it: what falls to nobody comes back here, because the
//! terminal behind it stands outside the focus order while the bar is open.

use crate::app::{App, Focus};
use crate::ui::icons;
use rust_i18n::t;
use zyt_term::{SearchDirection, SearchKind};

/// Widget of the field, so a command can hand the keyboard back.
pub fn field_id() -> egui::Id {
    egui::Id::new("search_field")
}

/// Draws the bar and applies what the user changed.
pub fn draw(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    let mut ids = Vec::with_capacity(6);

    ui.horizontal(|ui| {
        let mut options = app.search.options;
        let mut changed = false;

        let kind = ui
            .button(kind_icon(options.kind))
            .on_hover_text(kind_hint(options.kind));
        ids.push(kind.id);
        let chooses_kind = kind.clicked();

        changed |= toggle(
            ui,
            icons::CASE,
            "search.case",
            "search.case_hint",
            &mut options.case_sensitive,
            &mut ids,
        );
        changed |= toggle(
            ui,
            icons::HIGHLIGHT,
            "search.highlight",
            "search.highlight_hint",
            &mut options.highlight_all,
            &mut ids,
        );

        let width = field_width(ui);
        let response = ui.add(field(app, ui.visuals(), width));
        ids.push(response.id);
        if app.search.focus_wanted {
            app.search.focus_wanted = false;
            response.request_focus();
        }

        let mut stepped = None;
        if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            response.request_focus();
            stepped = Some(SearchDirection::Up);
        }
        if step(ui, icons::UP, "command.search.previous", &mut ids) {
            stepped = Some(SearchDirection::Up);
        }
        if step(ui, icons::DOWN, "command.search.next", &mut ids) {
            stepped = Some(SearchDirection::Down);
        }

        if changed {
            app.search.options = options;
            app.settings.search = options;
            app.save_settings();
        }
        if changed || response.changed() {
            app.search.apply(&mut app.session.terminal);
        } else if let Some(direction) = stepped {
            app.search.advance(&mut app.session.terminal, direction);
        }
        if chooses_kind {
            app.open_search_kind_menu();
        }
    });

    keep_keyboard(app, ui, context, &ids);
}

/// Keeps the keyboard inside the bar.
///
/// Tab off the last control and shift+Tab off the first one leave the toolkit
/// with no focused widget at all, and so does a click beside the controls. The
/// bar takes it back: forwards to its first control, backwards to its last one,
/// otherwise to the field, which is where someone who clicked wants to type.
fn keep_keyboard(app: &mut App, ui: &egui::Ui, context: &egui::Context, ids: &[egui::Id]) {
    if app.menu.is_open() || app.ui.pending_delete.is_some() {
        return;
    }
    if context.memory(|memory| memory.focused().is_some()) {
        return;
    }
    let (forwards, backwards) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::Tab) && !input.modifiers.shift,
            input.key_pressed(egui::Key::Tab) && input.modifiers.shift,
        )
    });
    let wanted = match (forwards, backwards) {
        (true, _) => ids.first().copied(),
        (_, true) => ids.last().copied(),
        _ => Some(field_id()),
    };
    if let Some(id) = wanted {
        context.memory_mut(|memory| memory.request_focus(id));
    }
    app.give_keyboard(Focus::Search);
}

/// One step of the search, the same one a key would take.
fn step(ui: &mut egui::Ui, icon: &str, name: &str, ids: &mut Vec<egui::Id>) -> bool {
    let response = ui.button(icon).on_hover_text(t!(name));
    ids.push(response.id);
    response.clicked()
}

/// One switch of the search, a button that stays pressed while it is on.
fn toggle(
    ui: &mut egui::Ui,
    icon: &str,
    name: &str,
    hint: &str,
    on: &mut bool,
    ids: &mut Vec<egui::Id>,
) -> bool {
    let response =
        ui.selectable_label(*on, icon)
            .on_hover_text(format!("{} \u{2014} {}", t!(name), t!(hint)));
    ids.push(response.id);
    if response.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// Icon of a way of reading the query.
fn kind_icon(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Literal => icons::LITERAL,
        SearchKind::Fuzzy => icons::FUZZY,
        SearchKind::Word => icons::WORD,
        SearchKind::Regex => icons::REGEX,
    }
}

/// What the button of the kind says: what it is now, and that a click offers
/// the others.
fn kind_hint(kind: SearchKind) -> String {
    t!("search.kind_hint", kind = t!(crate::search::kind_key(kind))).to_string()
}

/// Width left for the field once the two steps have their own.
fn field_width(ui: &egui::Ui) -> f32 {
    let one = crate::ui::widgets::label_width(ui, &[icons::UP, icons::DOWN])
        + 2.0 * ui.spacing().button_padding.x;
    (ui.available_width() - 2.0 * one).max(0.0)
}

/// The field, painted in the color of an error while nothing is found.
fn field<'a>(app: &'a mut App, visuals: &egui::Visuals, width: f32) -> egui::TextEdit<'a> {
    let missed = app.search.missed && !app.search.query.is_empty();
    let hint = if missed {
        t!("search.not_found")
    } else {
        t!("search.field")
    };
    let mut edit = egui::TextEdit::singleline(&mut app.search.query)
        .id(field_id())
        .hint_text(hint)
        .desired_width(width);
    if missed {
        edit = edit.text_color(visuals.error_fg_color);
    }
    edit
}
