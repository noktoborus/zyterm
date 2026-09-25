//! The window that asks for the values a console needs, before it is opened.
//!
//! A console runs a command line and starts in a directory, and either may ask
//! for a value by name. A name nothing answers would be carried into the
//! command as it stands — `ssh @board` — and what comes of that is a failure
//! read in the terminal about a line nobody wrote. So the question is put
//! before the connection: the window names what is wanted, takes the answers,
//! and only then is the console opened.
//!
//! The question is put every time that console is opened, because a name the
//! settings of the console leave out is a name that was left out on purpose:
//! it is the answer to "which one this time", and a window that asked once and
//! then stopped would be a window connecting to yesterday's board. What is
//! typed is kept per source, in a file of its own, and stands in the fields the
//! next time, so the same board is `Enter` and another one is the difference
//! typed over it. It never reaches the settings of the source: those say what
//! the console is, and this says what it is being pointed at this evening.
//!
//! The window is the one thing the user is answering, so it does not move, what
//! is behind it does not answer the keyboard or the mouse while it stands, and
//! the two ways out of it — the cross of its title bar and `Esc` — both lead
//! back to the list of the sources with nothing connected: a question waved
//! away must not leave the window with nothing to open.

use crate::app::{App, Ask};
use crate::consoles::ConsoleId;
use rust_i18n::t;

/// The narrowest a field of the window is, which is what an empty one shows.
const VALUE_LEAST: usize = 24;

/// The widest it grows to as it is written in.
const VALUE_MOST: usize = 64;

/// What this console wants answering before it is opened, or nothing when it
/// wants nothing.
///
/// What is asked for is every name the console names — in its command line, in
/// its arguments, in its directory — that its own settings do not answer. It is
/// asked every time that console is opened and not only the first, and what was
/// answered last time stands in the fields: a connection to the same board is
/// `Enter`, and one to another is the difference typed over it.
pub fn wanted(app: &App, console: ConsoleId) -> Option<Ask> {
    let stored = app.console(&console)?;
    let key = stored.key();
    let values = asked_of(stored, &crate::answers::load(&app.store, &key));

    if values.is_empty() {
        return None;
    }

    Some(Ask {
        console,
        key,
        values,
        opening: true,
    })
}

/// The names this console wants answering, each with what was answered last
/// time, in the order the console names them.
///
/// A name its own settings answer is not among them: that value was decided
/// about the console and is not a question. A row that is there and empty is
/// no answer, so it is a question like a name the settings never mention.
fn asked_of(
    console: &crate::consoles::Console,
    answered: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let settled = console.memory.variable_map();

    console
        .variables()
        .into_iter()
        .filter(|name| settled.get(name).is_none_or(String::is_empty))
        .map(|name| {
            let value = answered.get(&name).cloned().unwrap_or_default();
            (name, value)
        })
        .collect()
}

/// What the window was told to do, on the frame it was told.
enum Act {
    /// Open the console with what stands in the fields.
    Connect,
    /// Show the settings of this connection and open nothing.
    Settings,
}

/// Draws the window, while there is one.
///
/// The window holds the values it is being given, so it is taken out of the
/// application while it is drawn and put back unless it was closed: what is
/// typed goes straight into it, and nothing is copied per frame.
pub fn draw(app: &mut App, context: &egui::Context) {
    let Some(mut ask) = app.ui.ask.take() else {
        return;
    };

    let mut open = true;
    let mut act = None;
    let mut entered = None;
    let mut fields: Vec<egui::Id> = Vec::new();
    let mut connect = None;
    let base = egui::Id::new("ask");

    egui::Window::new(t!("ask.title", console = app.console_shown(ask.console)))
        .id(base)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(context, |ui| {
            let grid = egui::Grid::new(base.with("grid"))
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for (index, (name, value)) in ask.values.iter_mut().enumerate() {
                        ui.label(egui::RichText::new(name.as_str()).monospace());

                        let id = base.with(("value", index));
                        fields.push(id);
                        let width = crate::ui::widgets::written_width(
                            ui,
                            value,
                            VALUE_LEAST,
                            crate::ui::widgets::monospace_width(ui, VALUE_MOST),
                        );
                        let response = crate::ui::widgets::sized_field(
                            ui,
                            width,
                            egui::TextEdit::singleline(value)
                                .id(id)
                                .hint_text(name.as_str())
                                .font(egui::TextStyle::Monospace),
                        );
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            entered = Some(index);
                        }
                        ui.end_row();
                    }
                });

            ui.add_space(10.0);
            let (pressed, id) = buttons(ui, grid.response.rect.width());
            connect = Some(id);
            if pressed.is_some() {
                act = pressed;
            }
        });

    let focus = next_focus(&mut ask, entered, &fields, connect);
    if let Some(id) = focus {
        context.memory_mut(|memory| memory.request_focus(id));
    }

    let escaped =
        context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if !open || escaped {
        app.open_source_menu();
        return;
    }

    match act {
        Some(Act::Connect) => {
            keep(app, &ask);
            let outcome = app.connect_console(ask.console);
            app.report(outcome);
        }
        Some(Act::Settings) => {
            keep(app, &ask);
            app.open_connection_settings();
            app.settings_source = Some(ask.key.clone());
        }
        None => app.ui.ask = Some(ask),
    }
}

/// The two ways on from the window, under the grid and in the middle of it.
///
/// They stand outside the grid because they are not a row of it: a row names
/// one value and holds the field for it, and these are what is done once the
/// values are given. One under the other rather than side by side, because they
/// are not a pair to choose between at a glance — one of them is what the
/// window is for and the other is the way to change what it is asking about.
///
/// The width of the grid is what they are centred in: a widget in a column that
/// centres is centred by its own width, which is what a button has and what a
/// row of them laid out the ordinary way has not — that one fills the width it
/// is given and starts at the left of it.
///
/// The one that connects is answered as well, so the caller has its identifier:
/// the last field hands the keyboard to it, and a button that holds the
/// keyboard is pressed by `Enter`.
fn buttons(ui: &mut egui::Ui, under: f32) -> (Option<Act>, egui::Id) {
    let height = ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y;

    let mut act = None;
    let mut id = None;
    ui.allocate_ui_with_layout(
        egui::vec2(under, height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            let response = ui.button(t!("ask.connect"));
            id = Some(response.id);
            if response.clicked() {
                act = Some(Act::Connect);
            }
            if ui.button(t!("ask.settings")).clicked() {
                act = Some(Act::Settings);
            }
        },
    );

    (act, id.unwrap_or_else(|| ui.id().with("connect")))
}

/// The field or the button the keyboard goes to next.
///
/// `Enter` in a field is an answer given: it hands the keyboard to the field
/// under it, and the last field hands it to the button that connects, so a
/// window of three values is answered without the hands leaving the keys. A
/// window that has just come up hands it to the first field, because a window
/// that asks for something and leaves the keyboard behind it has to be clicked
/// into before it can be answered.
fn next_focus(
    ask: &mut Ask,
    entered: Option<usize>,
    fields: &[egui::Id],
    connect: Option<egui::Id>,
) -> Option<egui::Id> {
    if std::mem::take(&mut ask.opening) {
        return fields.first().copied().or(connect);
    }

    let index = entered?;
    fields.get(index + 1).copied().or(connect)
}

/// Writes down what was typed, so the next time this connection is picked the
/// window comes up with it already in the fields.
fn keep(app: &mut App, ask: &Ask) {
    let values = ask
        .values
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let outcome = crate::answers::save(&app.store, &ask.key, &values);
    app.report(outcome);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consoles::Console;
    use crate::sources::{SourceMemory, SourceVariable};
    use std::collections::BTreeMap;

    fn console(program: &str, directory: &str, settled: &[(&str, &str)]) -> Console {
        Console {
            name: "board".to_string(),
            program: program.to_string(),
            directory: directory.to_string(),
            memory: SourceMemory {
                variables: settled
                    .iter()
                    .map(|(name, value)| SourceVariable {
                        name: (*name).to_string(),
                        value: (*value).to_string(),
                    })
                    .collect(),
                ..SourceMemory::default()
            },
            ..crate::consoles::default_console()
        }
    }

    fn answered(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// What is asked for is what the command line and the directory name and
    /// the settings of the console do not answer — and a row of those settings
    /// that stands empty answers nothing.
    #[test]
    fn what_the_settings_answer_is_not_asked_for() {
        let console = console(
            "ssh {user}@{host}",
            "{work}",
            &[("user", "root"), ("host", "")],
        );

        let asked = asked_of(&console, &BTreeMap::new());

        let names: Vec<&str> = asked.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["host", "work"]);
    }

    /// The answer of last time stands in the field, and the names are asked
    /// again whether they were answered or not: a name outside the settings is
    /// the answer to "which one this time", and last time's is what somebody is
    /// about to keep or type over.
    #[test]
    fn the_answer_of_last_time_is_what_the_window_opens_with() {
        let console = console("ssh {host}", "{work}", &[]);

        let both = asked_of(
            &console,
            &answered(&[("host", "board-7"), ("work", "/srv")]),
        );

        assert_eq!(
            both,
            vec![
                ("host".to_string(), "board-7".to_string()),
                ("work".to_string(), "/srv".to_string()),
            ]
        );

        let one = asked_of(&console, &answered(&[("host", "board-7")]));
        assert_eq!(one.len(), 2, "a name nothing answered is asked with them");
        assert!(one.iter().any(|(_, value)| value.is_empty()));

        let answered_by_settings = super::tests::console("ssh {host}", "", &[("host", "board-7")]);
        assert!(
            asked_of(&answered_by_settings, &answered(&[])).is_empty(),
            "a console whose settings answer everything is never asked about"
        );
    }

    /// `Enter` walks the fields and stops on the button that connects; a window
    /// that has just come up starts on the first field.
    #[test]
    fn the_keyboard_walks_the_fields_and_lands_on_the_button() {
        let fields = [egui::Id::new("one"), egui::Id::new("two")];
        let connect = Some(egui::Id::new("connect"));
        let board = ConsoleId::new();
        let mut ask = Ask {
            console: board,
            key: crate::sources::SourceKey::Console(board),
            values: Vec::new(),
            opening: true,
        };

        assert_eq!(
            next_focus(&mut ask, None, &fields, connect),
            Some(fields[0]),
            "the window opens on the first field"
        );
        assert!(!ask.opening, "and only on the frame it opened");

        assert_eq!(
            next_focus(&mut ask, Some(0), &fields, connect),
            Some(fields[1])
        );
        assert_eq!(next_focus(&mut ask, Some(1), &fields, connect), connect);
        assert_eq!(next_focus(&mut ask, None, &fields, connect), None);
    }
}
