//! Central panel holding the terminal widget.

use crate::app::{App, Focus};
use crate::ui::{icons, menu};
use rust_i18n::t;
use zyt_term_egui::TerminalView;

/// Draws the terminal, its links and its context menu.
pub fn draw(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    app.font.size = app.settings.font_size;

    let links = app.osc().links;
    let program_colors = app.osc().palette;
    let mouse_reports = app.mouse_reports(context);
    let selection_anchor = app.settings.show_selection_anchor;
    let null_glyph = app.null_glyph(context);
    let App {
        session,
        terminal_theme,
        terminal_cache,
        font,
        ui: state,
        ..
    } = app;
    let focused = state.focus == Focus::Terminal;

    let (response, output) = TerminalView::new(
        &mut session.terminal,
        &mut session.content,
        terminal_cache,
        terminal_theme,
        font,
    )
    .focused(focused)
    .links(links)
    .program_colors(program_colors)
    .mouse_reports(mouse_reports)
    .selection_anchor(selection_anchor)
    .null_glyph(null_glyph)
    .show(ui);

    if let Some((columns, rows)) = output.resized {
        session.resize(columns, rows);
    }
    if !output.bytes.is_empty() && !session.is_transferring() {
        session.write(&output.bytes);
    }
    if let Some(text) = output.copied
        && !text.is_empty()
    {
        app.publish_selection(&text, context);
    }

    if output.paste_requested {
        app.paste_selection_or_clipboard(context);
    }

    app.hovered_link = output.hovered_link.clone();
    if let Some(target) = &output.opened_link {
        let uri = target.uri.clone();
        app.open_link(&uri);
    }

    if response.clicked() {
        app.close_search();
        app.give_keyboard(Focus::Terminal);
    }

    cancel_transfer(app, ui, response.rect);

    if let Some(target) = &app.hovered_link {
        link_preview(ui, response.rect, &target.uri);
    }

    if output.wants_mouse {
        app.link_menu = None;
    } else if response.clicked_by(egui::PointerButton::Secondary) {
        // The right button and not what the toolkit calls a secondary click: a
        // finger held on the text is one of those, and what it asks for is a
        // selection.
        open_menu(app, output.link_menu.clone());
    }
}

/// Opens the menu of what the pointer stands on.
///
/// The link is taken from this click and from nothing else, so a menu opened
/// beside a link is the menu of the terminal, whatever the last click was on.
fn open_menu(app: &mut App, link: Option<zyt_term_egui::LinkTarget>) {
    app.link_menu = link;
    let items = if app.link_menu.is_some() {
        menu::link_items(app)
    } else {
        menu::terminal_items(app)
    };
    if let Err(error) = app.menu.open(items) {
        log::debug!("terminal menu: {error}");
    }
}

/// The way to stop the transfer that holds the line, in the upper right corner
/// of the terminal.
///
/// It stands over the output and not in the status bar because of what it
/// stops: while that transfer runs the keyboard reaches nothing and the device
/// is not read into the terminal, so the window is doing one thing, and the way
/// out of it belongs where the eye already is. The status bar carries the list
/// of everything that runs, which is another question.
fn cancel_transfer(app: &mut App, ui: &mut egui::Ui, area: egui::Rect) {
    if !app.session.is_transferring() {
        return;
    }

    let corner = egui::pos2(area.right() - CORNER_GAP, area.top() + CORNER_GAP);
    let clicked = egui::Area::new(ui.id().with("cancel_transfer"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(corner)
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .show(ui, |ui| {
                    ui.add(egui::Button::new(
                        egui::RichText::new(format!("{} {}", icons::CLOSE, t!("status.transfer")))
                            .color(ui.visuals().warn_fg_color),
                    ))
                    .on_hover_text(t!("status.cancel_transfer_hint"))
                    .clicked()
                })
                .inner
        })
        .inner;

    if clicked {
        let outcome = app.session.cancel_transfer();
        app.report(outcome);
    }
}

/// How far that button stands from the corner it sits in.
const CORNER_GAP: f32 = 8.0;

/// Shows the address under the pointer in the lower left corner.
fn link_preview(ui: &egui::Ui, area: egui::Rect, uri: &str) {
    let painter = ui.painter();
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = painter.layout_no_wrap(uri.to_string(), font, ui.visuals().strong_text_color());
    let size = galley.size() + egui::vec2(10.0, 6.0);
    let rect = egui::Rect::from_min_size(egui::pos2(area.left(), area.bottom() - size.y), size);

    painter.rect_filled(rect, 3.0, ui.visuals().window_fill);
    painter.rect_stroke(
        rect,
        3.0,
        ui.visuals().window_stroke,
        egui::StrokeKind::Inside,
    );
    painter.galley(
        rect.min + egui::vec2(5.0, 3.0),
        galley,
        egui::Color32::WHITE,
    );
}
