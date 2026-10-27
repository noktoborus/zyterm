//! Central panel holding the terminal widget.

use crate::app::{App, Focus};
use crate::ui::{menu, selection};
use rust_i18n::t;
use zyt_term_egui::TerminalView;

/// Draws the terminal, its links and its context menu.
pub fn draw(app: &mut App, ui: &mut egui::Ui, context: &egui::Context) {
    app.font.size = app.settings.font_size;

    let links = app.osc().links;
    let program_colors = app.osc().palette;
    // While a selection is being picked out the pointer belongs to it, whatever
    // the program on the line asked for: a drag that reported itself to the
    // program would pick out nothing, and the mode is what the person asked for
    // by starting one. A pane of tmux shown before its snapshot has not said
    // yet whether its program reads the mouse.
    let selecting = app.session.page().selecting;
    let mouse_reports = app.mouse_reports(context) && !selecting && !app.session.tmux_loading();
    // The cursors of a selection are drawn while one is being made whatever the
    // setting says: the end that moves is what the keys are moving.
    let selection_ends = app.settings.show_selection_ends || selecting;
    let resize_delay = crate::config::resize_delay(app.settings.resize_delay);
    let history_by_scrolling = app.settings.tmux.history;
    let App {
        session,
        terminal_theme,
        terminal_cache,
        font,
        ui: state,
        ..
    } = app;
    let focused = state.focus == Focus::Terminal;

    let size_held = session.size_held();
    let above_top = session
        .tmux_above_top()
        .map(|text| zyt_term_egui::AboveTop { text });
    let (response, mut output) =
        TerminalView::new(session.snapshot(), terminal_cache, terminal_theme, font)
            .focused(focused)
            .auto_resize(!size_held)
            .resize_delay(resize_delay)
            .links(links)
            .program_colors(program_colors)
            .mouse_reports(mouse_reports)
            .selection_ends(selection_ends)
            .above_top(above_top)
            .show(ui);

    session.apply(std::mem::take(&mut output.commands));
    if let Some((columns, rows)) = output.resized {
        session.resize(columns, rows);
    }
    // History of tmux comes a screen at a time: pressed for on the row, or —
    // with `tmux.history` on — as soon as the view stands on it.
    if output.above_top_pressed || (history_by_scrolling && output.at_above_top) {
        session.tmux_load_history();
    }
    if size_held {
        let cell = font.cell_size(ui.ctx());
        margins(ui, response.rect, cell, &output, terminal_theme.foreground);
    }
    loading_plate(ui, response.rect, session.tmux_loading());
    // A cell of another size is a window of another size with the same grid in
    // it, and the pixels of the window are what a program drawing reads.
    session.follow_cell_size();
    app.send_input(&output.bytes);

    if output.paste_requested {
        app.paste_selection_or_clipboard();
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

    selection::plate(app, ui, response.rect);
    super::transfer::plate(app, ui, response.rect);

    if let Some(target) = &app.hovered_link {
        link_preview(ui, response.rect, &target.uri);
    }

    if output.wants_mouse {
        app.link_menu = None;
        app.image_menu = None;
    } else if response.clicked_by(egui::PointerButton::Secondary) {
        // The right button and not what the toolkit calls a secondary click: a
        // finger held on the text is one of those, and what it asks for is a
        // selection.
        open_menu(app, output.link_menu.clone(), output.image_menu);
    }
}

/// Opens the menu of what the pointer stands on.
///
/// What it stands on is taken from this click and from nothing else, so a menu
/// opened beside a link or a picture is the menu of the terminal, whatever the
/// last click was on. A link wins over a picture: a link is a cell saying where
/// it leads, which is a narrower answer than the picture it may be drawn over.
///
/// A picture is asked of the screen, and its menu opens when the answer comes
/// ([`open_picture_menu`]).
fn open_menu(
    app: &mut App,
    link: Option<zyt_term_egui::LinkTarget>,
    picture: Option<zyt_term::ImageId>,
) {
    app.link_menu = link;
    app.image_menu = None;
    if app.link_menu.is_none()
        && let Some(id) = picture
    {
        app.session.ask(
            crate::session::Purpose::PictureMenu(id),
            zyt_term::Command::Picture(id),
        );
        return;
    }
    show_menu(app);
}

/// Opens the menu of a picture the screen answered with, or the menu of the
/// terminal when the picture was dropped meanwhile.
pub fn open_picture_menu(app: &mut App, menu: Option<crate::app::PictureMenu>) {
    app.link_menu = None;
    app.image_menu = menu;
    show_menu(app);
}

/// Opens the menu of what stands in `app`: a link, a picture, or the terminal.
fn show_menu(app: &mut App) {
    let items = if app.link_menu.is_some() {
        menu::link_items(app)
    } else if app.image_menu.is_some() {
        menu::image_items(app)
    } else {
        menu::terminal_items(app)
    };
    if let Err(error) = app.menu.open(items) {
        log::debug!("terminal menu: {error}");
    }
}

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

/// Gap between two lines of the hatching past a smaller window, in points.
const HATCH_GAP: f32 = 6.0;
/// Alpha of the hatching: the foreground of the terminal, barely there.
const HATCH_ALPHA: f32 = 0.18;
/// Height and length of one wave of the line along a cut edge, in points.
const WAVE: (f32, f32) = (2.0, 8.0);

/// What says a pane of tmux is not the size of the area. Smaller, the rest of
/// the area is hatched, from the corner of the area and not along the rows of
/// the grid: the hatching is where the window of tmux ends, so no line is drawn
/// along its edge. Larger, a red wavy line runs along each edge that cuts it.
///
/// What is short of a whole cell is not hatched: the plain terminal leaves it
/// too, and it says nothing of the window of tmux.
fn margins(
    ui: &egui::Ui,
    area: egui::Rect,
    cell: egui::Vec2,
    output: &zyt_term_egui::TerminalOutput,
    foreground: egui::Color32,
) {
    let grid = output.grid;
    if grid.is_positive() {
        let hatch = egui::Stroke::new(1.0, foreground.gamma_multiply(HATCH_ALPHA));
        let right_short = area.max.x - grid.max.x >= cell.x;
        let below_short = area.max.y - grid.max.y >= cell.y;
        let right = egui::Rect::from_min_max(egui::pos2(grid.max.x, area.min.y), area.max);
        let below_end = if right_short { grid.max.x } else { area.max.x };
        let below = egui::Rect::from_min_max(
            egui::pos2(area.min.x, grid.max.y),
            egui::pos2(below_end, area.max.y),
        );
        for (part, short) in [(right, right_short), (below, below_short)] {
            if short {
                hatching(&ui.painter().with_clip_rect(part), area, hatch);
            }
        }
    }
    let cut = egui::Stroke::new(1.5, ui.visuals().error_fg_color);
    let painter = ui.painter().with_clip_rect(area);
    if output.cut_right {
        let x = area.max.x - WAVE.0 - 1.0;
        painter.add(egui::Shape::line(
            wave(area.min.y, area.max.y, |along, across| {
                egui::pos2(x + across, along)
            }),
            cut,
        ));
    }
    if output.cut_bottom {
        let y = area.max.y - WAVE.0 - 1.0;
        painter.add(egui::Shape::line(
            wave(area.min.x, area.max.x, |along, across| {
                egui::pos2(along, y + across)
            }),
            cut,
        ));
    }
}

/// Diagonal lines over the whole area, the painter clipped to the part hatched.
fn hatching(painter: &egui::Painter, area: egui::Rect, stroke: egui::Stroke) {
    let height = area.height();
    let mut offset = -height;
    while offset < area.width() {
        let start = egui::pos2(area.min.x + offset, area.max.y);
        let end = egui::pos2(area.min.x + offset + height, area.min.y);
        painter.line_segment([start, end], stroke);
        offset += HATCH_GAP;
    }
}

/// The points of a wavy line from `from` to `to`, placed by `at`.
fn wave(from: f32, to: f32, at: impl Fn(f32, f32) -> egui::Pos2) -> Vec<egui::Pos2> {
    let step = WAVE.1 / 4.0;
    let mut points = Vec::new();
    let mut along = from;
    let mut quarter = 0usize;
    while along <= to {
        let across = match quarter % 4 {
            1 => WAVE.0,
            3 => -WAVE.0,
            _ => 0.0,
        };
        points.push(at(along, across));
        along += step;
        quarter += 1;
    }
    points
}

/// How long a pane of tmux is shown without its screen before the plate says
/// so: on a fast line the snapshot comes first, and the plate would flicker.
const LOADING_PLATE_DELAY: f64 = 0.15;

/// The plate over a pane of tmux shown as it is while its snapshot is on its
/// way: what stands is not its screen yet. It comes after a delay from when
/// the pane shown began to wait, and stays while the pane shown waits, one
/// pane after another.
fn loading_plate(ui: &egui::Ui, area: egui::Rect, loading: bool) {
    let id = ui.id().with("tmux_loading");
    if !loading {
        ui.data_mut(|data| data.remove_temp::<f64>(id));
        return;
    }
    let now = ui.input(|input| input.time);
    let since = ui.data_mut(|data| *data.get_temp_mut_or_insert_with(id, || now));
    let left = since + LOADING_PLATE_DELAY - now;
    if left > 0.0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(left));
        return;
    }
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(area.center())
        .pivot(egui::Align2::CENTER_CENTER)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(t!("tmux.loading_screen"));
            });
        });
}
