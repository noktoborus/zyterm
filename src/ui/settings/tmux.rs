//! The block *tmux* of the page: what is of tmux in control mode alone.
//!
//! A row is a switch, and under it, in place of a hint, a line that says what
//! the switch does in the position it stands in, changing with it: a switch of
//! a protocol is read for what it does now, not for what it is called.

use rust_i18n::t;

use crate::config::Settings;

/// The block; true when a value changed.
pub(super) fn block(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut changed = false;
    ui.heading(t!("settings.tmux"));

    ui.horizontal(|ui| {
        changed |= crate::ui::widgets::switch(ui, &mut settings.tmux.history).changed();
        ui.label(t!("settings.tmux_history"));
    });
    let line = match settings.tmux.history {
        true => t!("settings.tmux_history_on"),
        false => t!("settings.tmux_history_off"),
    };
    ui.label(egui::RichText::new(line).weak());
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        changed |= crate::ui::widgets::switch(ui, &mut settings.tmux.pause).changed();
        ui.label(t!("settings.tmux_pause"));
    });
    let line = match settings.tmux.pause {
        true => t!(
            "settings.tmux_pause_on",
            seconds = settings.tmux.pause_after
        ),
        false => t!("settings.tmux_pause_off"),
    };
    ui.label(egui::RichText::new(line).weak());

    changed
}
