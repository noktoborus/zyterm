//! The block of debugging: the debug window, the journal of the line and the
//! journal of messages.

use crate::app::App;
use crate::config::Settings;
use crate::ui::access::Named;
use crate::ui::choice::{Choice, message_level_label};
use rust_i18n::t;

/// Draws the block; true when a setting of the copy changed.
///
/// The two levels and the directory picked in the dialog are set on the
/// application itself, through their menus and the dialog, as every list of the
/// settings is.
pub(super) fn block(ui: &mut egui::Ui, settings: &mut Settings, app: &mut App) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.debug"));

    egui::Grid::new(ui.make_persistent_id("debug"))
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label(t!("settings.debug_window"))
                .on_hover_text(t!("settings.debug_window_hint"));
            changed |= crate::ui::widgets::switch(
                ui,
                &mut settings.debug.window,
                &t!("settings.debug_window"),
            )
            .on_hover_text(t!("settings.debug_window_hint"))
            .changed();
            ui.end_row();

            ui.label(t!("settings.journal"))
                .on_hover_text(t!("settings.journal_hint"));
            changed |= crate::ui::widgets::switch(
                ui,
                &mut settings.debug.journal,
                &t!("settings.journal"),
            )
            .on_hover_text(t!("settings.journal_hint"))
            .changed();
            ui.end_row();

            ui.label(t!("settings.journal_dir"));
            ui.horizontal(|ui| {
                if let Some(typed) = directory_field(ui, &settings.debug.journal_dir) {
                    settings.debug.journal_dir = typed;
                    changed = true;
                }
                if ui.button(t!("settings.journal_pick")).clicked() {
                    app.pick_journal_directory();
                }
            });
            ui.end_row();

            for (others, key) in [
                (false, "settings.messages"),
                (true, "settings.messages_others"),
            ] {
                ui.label(t!(key))
                    .on_hover_text(t!("settings.messages_hint"));
                let level = match others {
                    true => settings.debug.messages_others,
                    false => settings.debug.messages,
                };
                crate::ui::choice::row(
                    ui,
                    app,
                    Choice::Messages { others },
                    &message_level_label(level),
                );
                ui.end_row();
            }
        });

    let weak = |ui: &mut egui::Ui, text: String| {
        ui.label(egui::RichText::new(text).weak());
    };
    match app.session.journal() {
        Some(journal) => {
            weak(
                ui,
                t!(
                    "settings.journal_writing",
                    path = journal.path().display().to_string(),
                    size = crate::format::size(journal.written())
                )
                .to_string(),
            );
            if ui.button(t!("settings.journal_open")).clicked()
                && let Some(directory) = journal.path().parent()
            {
                app.open_directory(directory);
            }
        }
        None => match settings.debug.directory() {
            Ok(directory) => weak(
                ui,
                t!(
                    "settings.journal_where",
                    dir = directory.display().to_string()
                )
                .to_string(),
            ),
            Err(error) => weak(ui, error.to_string()),
        },
    }
    weak(ui, t!("settings.journal_private").to_string());
    changed
}

/// The field of the directory of the journals, and the directory typed into it
/// once the typing is done.
///
/// What is being typed is kept by the toolkit and not in the settings: a
/// journal is opened anew in a directory that changed, and a half typed name is
/// no directory to open one in. The field takes the setting again whenever it
/// does not hold the keyboard.
fn directory_field(ui: &mut egui::Ui, directory: &std::path::Path) -> Option<std::path::PathBuf> {
    let id = ui.make_persistent_id("journal_dir");
    let mut typed = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| directory.display().to_string());
    let response = ui
        .add(
            egui::TextEdit::singleline(&mut typed)
                .id(id.with("field"))
                .hint_text(t!("settings.journal_dir_empty"))
                .desired_width(320.0),
        )
        .named(&t!("settings.journal_dir"));
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, typed));
        return None;
    }
    ui.data_mut(|data| data.remove::<String>(id));
    let typed = std::path::PathBuf::from(typed.trim());
    (response.lost_focus() && typed != directory).then_some(typed)
}
