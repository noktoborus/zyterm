//! The panes of a window of tmux drawn side by side, as the layout of tmux
//! puts them (`docs/tmux.md`, *Panes side by side*): the pane the keys go to
//! is the main view of `ui::terminal`; this draws the others, shaded and with
//! no cursor, and the dividers between them, which a drag moves.

use rust_i18n::t;
use zyt_term_egui::{TerminalCache, TerminalView};

use crate::app::App;
use crate::tmux::{Choice, Divider, Place, Places};

/// Where a pane stands in the area, the window of tmux drawn from its top
/// left corner.
fn rect_of(place: &Place, area: egui::Rect, cell: egui::Vec2) -> egui::Rect {
    egui::Rect::from_min_size(
        area.min + egui::vec2(place.x as f32 * cell.x, place.y as f32 * cell.y),
        egui::vec2(place.columns as f32 * cell.x, place.rows as f32 * cell.y),
    )
}

/// Where the main view stands while the panes are side by side: the place of
/// the pane the keys go to.
pub fn shown_rect(places: &Places, area: egui::Rect, cell: egui::Vec2) -> Option<egui::Rect> {
    let shown = places.shown?;
    let place = places.panes.iter().find(|place| place.pane == shown)?;
    Some(rect_of(place, area, cell))
}

/// What the main view reports as drawn, for the margins of a window of
/// another size: the whole window, not the pane in it.
pub fn window_output(
    places: &Places,
    area: egui::Rect,
    cell: egui::Vec2,
) -> zyt_term_egui::TerminalOutput {
    let size = egui::vec2(places.columns as f32 * cell.x, places.rows as f32 * cell.y);
    zyt_term_egui::TerminalOutput {
        grid: egui::Rect::from_min_size(area.min, size).intersect(area),
        cut_right: size.x > area.width() + 0.5,
        cut_bottom: size.y > area.height() + 0.5,
        ..Default::default()
    }
}

/// Draws the panes beside the one shown and the dividers. What the mouse
/// reports in a pane — the wheel, a motion — goes to its program, the focus
/// staying; a press gives it the focus as well.
pub fn draw(app: &mut App, ui: &mut egui::Ui, area: egui::Rect, places: &Places) {
    if places.panes.len() < 2 {
        app.beside_caches.clear();
        return;
    }
    app.beside_caches
        .retain(|pane, _| places.panes.iter().any(|place| place.pane == *pane));
    let cell = app.font.cell_size(ui.ctx());
    let links = app.osc().links;
    let program_colors = app.osc().palette;
    let mouse_reports = app.mouse_reports(ui.ctx());
    let shade = f32::from(app.settings.tmux.shade.min(100)) / 100.0;
    for place in &places.panes {
        let rect = rect_of(place, area, cell).intersect(area);
        if Some(place.pane) == places.shown || !rect.is_positive() {
            continue;
        }
        let App {
            session,
            beside_caches,
            terminal_theme,
            font,
            ..
        } = app;
        let output = session.beside(place.pane).map(|snapshot| {
            let cache = beside_caches
                .entry(place.pane)
                .or_insert_with(TerminalCache::new);
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                TerminalView::new(snapshot, cache, terminal_theme, font)
                    .focused(false)
                    .cursor(false)
                    .auto_resize(false)
                    .selection_ends(false)
                    .links(links)
                    .program_colors(program_colors)
                    .mouse_reports(mouse_reports)
                    .show(ui)
            })
            .inner
        });
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_black_alpha((shade * 255.0) as u8),
        );
        crate::ui::terminal::loading_plate(ui, place.pane.0, rect, place.waits);
        let Some((response, mut output)) = output else {
            continue;
        };
        let pressed =
            response.is_pointer_button_down_on() && ui.input(|input| input.pointer.any_pressed());
        if pressed && let Some(window) = places.window {
            app.session.tmux_choose(Choice::Focus {
                window,
                pane: place.pane,
            });
        }
        app.session
            .apply_beside(place.pane, std::mem::take(&mut output.commands));
        app.session.tmux_input_to(place.pane, &output.bytes);
    }
    for divider in &places.dividers {
        drag_divider(app, ui, area, cell, divider);
    }
}

/// A divider: a thin line in the middle of the cell tmux keeps, dragged by
/// the whole cell. While dragged it moves here alone, a cell at a step; let
/// go, the cell before it is resized through a pane of it, and it stands
/// where it was let go until tmux says the layout.
fn drag_divider(
    app: &mut App,
    ui: &mut egui::Ui,
    area: egui::Rect,
    cell: egui::Vec2,
    divider: &Divider,
) {
    let along = |at: f32, from: usize, length: usize| match divider.side_by_side {
        true => egui::Rect::from_min_size(
            area.min + egui::vec2(at * cell.x, from as f32 * cell.y),
            egui::vec2(cell.x, length as f32 * cell.y),
        ),
        false => egui::Rect::from_min_size(
            area.min + egui::vec2(from as f32 * cell.x, at * cell.y),
            egui::vec2(length as f32 * cell.x, cell.y),
        ),
    };
    let id = ui
        .id()
        .with(("tmux_divider", divider.pane, divider.side_by_side));
    let rect = along(divider.at as f32, divider.from, divider.length);
    let response = ui.interact(rect, id, egui::Sense::drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, t!("tmux.divider"))
    });
    let cursor = match divider.side_by_side {
        true => egui::CursorIcon::ResizeHorizontal,
        false => egui::CursorIcon::ResizeVertical,
    };
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(cursor);
    }
    let dragging = id.with("dragging");
    if let Some(pointer) = response
        .interact_pointer_pos()
        .filter(|_| response.dragged())
    {
        let cells = match divider.side_by_side {
            true => (pointer.x - area.min.x) / cell.x,
            false => (pointer.y - area.min.y) / cell.y,
        };
        let at = (cells.floor().max(0.0) as usize).max(divider.start + 1);
        ui.data_mut(|data| data.insert_temp(dragging, at));
    }
    let held = ui.data(|data| data.get_temp::<(usize, usize)>(id));
    let at = ui
        .data(|data| data.get_temp::<usize>(dragging))
        .or(match held {
            Some((stood, wanted)) if stood == divider.at => Some(wanted),
            _ => None,
        })
        .unwrap_or(divider.at);
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove_temp::<usize>(dragging));
    }
    if response.drag_stopped() && at != divider.at {
        let cells = u16::try_from(at - divider.start).unwrap_or(u16::MAX);
        app.session.tmux_choose(Choice::Resize {
            pane: divider.pane,
            side_by_side: divider.side_by_side,
            cells,
        });
        ui.data_mut(|data| data.insert_temp(id, (divider.at, at)));
    }
    let line = along(at as f32, divider.from, divider.length);
    let stroke = egui::Stroke::new(1.0, app.terminal_theme.foreground.gamma_multiply(0.5));
    let ends = match divider.side_by_side {
        true => [line.center_top(), line.center_bottom()],
        false => [line.left_center(), line.right_center()],
    };
    ui.painter().line_segment(ends, stroke);
}
