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
                let mut directory = settings.debug.journal_dir.display().to_string();
                let edited = ui
                    .add(
                        egui::TextEdit::singleline(&mut directory)
                            .hint_text(t!("settings.journal_dir_empty"))
                            .desired_width(320.0),
                    )
                    .named(&t!("settings.journal_dir"))
                    .changed();
                if edited {
                    settings.debug.journal_dir = directory.trim().into();
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
    match settings.debug.directory() {
        Ok(directory) => weak(
            ui,
            t!(
                "settings.journal_where",
                dir = directory.display().to_string()
            )
            .to_string(),
        ),
        Err(error) => weak(ui, error.to_string()),
    }
    weak(ui, t!("settings.journal_private").to_string());
    changed
}
