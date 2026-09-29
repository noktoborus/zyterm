//! Interface of the application.

pub mod ask;
pub mod choice;
pub mod confirm;
pub mod connect;
mod debug;
mod file_dialog;
pub mod icons;
pub mod menu;
pub mod search;
mod selection;
pub mod settings;
mod signals;
mod statusbar;
mod tasks;
mod terminal;
pub mod widgets;

use crate::app::{App, MainView};
use rust_i18n::t;

/// Asks before a file of the session is thrown away, when the settings say to.
fn confirm_file(app: &mut App, context: &egui::Context) {
    let Some(pending) = app.pending_file.clone() else {
        return;
    };

    let question = t!(pending.question_key());
    let detail = pending.path().display().to_string();
    match confirm::ask(context, "file", question.as_ref(), &detail) {
        confirm::Answer::Yes => {
            app.pending_file = None;
            app.run_pending_file(pending);
        }
        confirm::Answer::No => app.pending_file = None,
        confirm::Answer::Pending => {}
    }
}

/// Takes down a question about a source the window cannot reach that was left
/// without an answer.
///
/// The question is the menu of plates, which is closed by `Esc` — and by a click
/// beside it when the settings say so. A question waved away leads to the picker
/// the way choosing it does, through `App::disconnect`, because it must not be
/// the one that closes the window.
fn lost_source(app: &mut App) {
    if app.lost.is_none() || app.menu.is_open() {
        return;
    }

    app.lost = None;
    app.disconnect();
}

/// Says what a file held over the window will do when it is let go.
fn hovered_files(app: &App, context: &egui::Context) {
    if context.input(|input| input.raw.hovered_files.is_empty()) {
        return;
    }

    let hint = if app.session.has_source() {
        t!("drop.hint")
    } else {
        t!("drop.not_connected")
    };

    egui::Area::new(egui::Id::new("hovered_files"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(hint).strong());
            });
        });
}

/// Draws everything below this point at the size the status bar was asked for.
///
/// A bar is as tall as what stands in it, so the height of the setting is a
/// factor and not a number of points to pad with: the letters, the buttons, the
/// room between them and the bar of progress are all multiplied by it, and the
/// panel comes out the height that was asked for with nothing standing small in
/// the middle of it.
fn grow(ui: &mut egui::Ui, scale: f32) {
    if (scale - 1.0).abs() < f32::EPSILON {
        return;
    }

    let style = ui.style_mut();
    for font in style.text_styles.values_mut() {
        font.size *= scale;
    }
    style.spacing.interact_size *= scale;
    style.spacing.button_padding *= scale;
    style.spacing.item_spacing *= scale;
    style.spacing.icon_width *= scale;
    style.spacing.icon_width_inner *= scale;
}

/// A margin of the bar at that factor, in the whole points a margin is written
/// in and never so large that it stops being one.
fn margin(points: f32, scale: f32) -> i8 {
    (points * scale).round().clamp(0.0, 127.0) as i8
}

/// Draws one frame.
///
/// The status bar carries the way out of the settings, so it stands there even
/// when it is hidden everywhere else. The search bar takes its place while it
/// is open, so it is there even when the status bar is switched off.
pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let context = ui.ctx().clone();

    menu::draw(app, &context);
    ask::draw(app, &context);
    tasks::draw(app, &context);
    debug::draw(app, &context);
    confirm_file(app, &context);
    lost_source(app);
    hovered_files(app, &context);

    if app.search.open || app.settings.show_status_bar || app.ui.view == MainView::Settings {
        let scale = crate::config::status_bar_scale(app.settings.status_bar_height);
        egui::Panel::bottom("status_bar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(
                        margin(6.0, scale),
                        margin(3.0, scale),
                    )),
            )
            .show(ui, |ui| {
                grow(ui, scale);
                if app.search.open {
                    search::draw(app, ui, &context);
                } else {
                    statusbar::draw(app, ui, &context);
                }
            });
    }

    let area = ui.available_rect_before_wrap();
    let frame = egui::Frame::NONE.fill(ui.visuals().panel_fill);
    match app.ui.view {
        MainView::Terminal => {
            egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                // The window asking for the values of a connection is the
                // one thing being answered while it stands, so what is
                // behind it is drawn and answers nothing.
                if app.ui.ask.is_some() {
                    ui.disable();
                }
                terminal::draw(app, ui, &context);
            });
        }
        MainView::Settings => {
            egui::CentralPanel::default()
                .frame(frame.inner_margin(egui::Margin::same(8)))
                .show(ui, |ui| settings::draw(app, ui, &context));
        }
        MainView::FileDialog => {
            egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |_ui| {});
            file_dialog::draw(app, &context, area);
        }
    }
}
