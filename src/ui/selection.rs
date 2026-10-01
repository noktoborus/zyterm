//! The plate of the selection, over the terminal.
//!
//! What was dragged over is drawn in the colours of a selection and nowhere
//! counted, so how much of it there is has to be guessed at from the shape of
//! the highlight. A line of a log, a block of a table, a key somebody is about
//! to paste into another window: each of them is a size somebody wanted to know
//! before letting the button go, and the answer arrives after the paste, in the
//! program that received it. This plate is that answer while the hand is still
//! on the mouse.
//!
//! It stands in the corner the moving end of the selection is furthest from.
//! That end is where the selection is growing — the pointer of a drag, the
//! caret of the keys — so a plate beside it would cover the very rows being
//! taken, and the corner opposite is the one place on the page the growing end
//! never reaches.
//!
//! It is that end and not the pointer, because a selection is made with the
//! keyboard as readily as with a hand: there is no pointer in that one, and the
//! pointer of a drag that is over is on its way somewhere else — a menu,
//! another window, the button that copies — while nothing about the selection
//! has changed. The end that moved last moves only when the selection does, so
//! the plate moves only then too. While that end is off the page — scrolled
//! away from — the plate keeps the corner it stands in.
//!
//! The plate is laid on the cell grid of the terminal — both corners of it fall
//! on a cell boundary and its width is a whole number of cells — so it covers
//! whole characters and never half of one. A frame whose edge runs down the
//! middle of a column leaves a sliver of every letter behind it, which reads as
//! damaged output rather than as something lying over it.
//!
//! A press on the plate lets the selection go: it is the one thing over the
//! terminal that is about the selection, so it is where a hand goes to be rid
//! of it.
//!
//! The three counts are of the text the selection would copy, and not of the
//! cells it spans: the number a plate shows and the number the program on the
//! other end of the clipboard shows are then the same number.

use crate::app::App;
use crate::ui::widgets::label_width;
use rust_i18n::t;
use zyt_term::SelectionSize;

/// Draws the plate while a selection stands in the terminal.
///
/// The area is the one the terminal widget took, so the corners are the corners
/// of the text and not of the window: the status bar below is not a place the
/// plate may reach into.
pub fn plate(app: &mut App, ui: &mut egui::Ui, area: egui::Rect) {
    if !app.session.terminal.selecting() {
        app.ui.selection_corner = None;
        return;
    }
    // A selection of blank cells counts nothing, and the plate says so rather
    // than going away: what it is answering while the mode stands is "how much
    // is in it", and none is an answer.
    let size = app
        .session
        .terminal
        .selection_size()
        .unwrap_or(SelectionSize {
            columns: 0,
            lines: 0,
            characters: 0,
        });

    let cell = app.font.cell_size(ui.ctx());
    let edge = app
        .session
        .terminal
        .selection_edge()
        .map(|(column, row)| edge_position(area, cell, column, row));
    let pivot = pivot(app.ui.selection_corner, edge, area);
    app.ui.selection_corner = Some(pivot);

    let plate = egui::Area::new(ui.id().with("selection_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(anchor(area, cell, pivot))
        .pivot(pivot)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| counts(ui, cell.x, size));
        });

    // A press on the plate lets the selection go: it is the one thing standing
    // over the terminal that is about the selection, so it is where a hand goes
    // to be rid of it.
    //
    // The area answers a press by itself, and that answer is read as it stands.
    // Asking it to sense one again (`Response::interact`) registers the same
    // identifier a second time in one frame, which the toolkit answers by
    // painting a complaint over the window.
    let pressed = plate
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked();
    if pressed {
        app.session.terminal.selection_clear();
        app.ui.selection_corner = None;
    }
}

/// The corner the plate takes now: the one the moving end is furthest from.
///
/// An end that is off the page says nothing about which corner to take, so the
/// plate keeps the one it has. With neither — a selection that appeared with
/// its end already scrolled away — it goes where a hand coming to the text is
/// least likely to be.
fn pivot(kept: Option<egui::Align2>, edge: Option<egui::Pos2>, area: egui::Rect) -> egui::Align2 {
    let Some(edge) = edge else {
        return kept.unwrap_or(egui::Align2::RIGHT_BOTTOM);
    };
    let center = area.center();
    let x = match edge.x < center.x {
        true => egui::Align::Max,
        false => egui::Align::Min,
    };
    let y = match edge.y < center.y {
        true => egui::Align::Max,
        false => egui::Align::Min,
    };
    egui::Align2([x, y])
}

/// The middle of the cell that end stands on.
///
/// A cell and not a corner of one, because what the corner of the plate is
/// decided against is which half of the page that end is in, and a cell on the
/// middle line of the page would answer differently at its top and its bottom.
fn edge_position(area: egui::Rect, cell: egui::Vec2, column: usize, row: usize) -> egui::Pos2 {
    egui::pos2(
        area.left() + (column as f32 + 0.5) * cell.x,
        area.top() + (row as f32 + 0.5) * cell.y,
    )
}

/// Where that corner of the plate stands, one cell in from the corner of the
/// grid.
///
/// The right and the lower edge are taken from the last whole cell and not from
/// the edge of the widget, because the pixels left over beside the grid belong
/// to no cell.
fn anchor(area: egui::Rect, cell: egui::Vec2, corner: egui::Align2) -> egui::Pos2 {
    let right = area.left() + (area.width() / cell.x).floor() * cell.x;
    let bottom = area.top() + (area.height() / cell.y).floor() * cell.y;
    let x = match corner.x() {
        egui::Align::Min => area.left() + cell.x,
        _ => right - cell.x,
    };
    let y = match corner.y() {
        egui::Align::Min => area.top() + cell.y,
        _ => bottom - cell.y,
    };
    egui::pos2(x, y)
}

/// The title and the three counts, as a grid of two columns.
///
/// The counts are the right hand column and stand against the edge of the
/// plate: three numbers of different lengths read as three numbers where their
/// last digits line up, and as a ragged column where their first ones do.
///
/// Where each of the three was drawn is the answer, because that is what says
/// they line up.
fn counts(ui: &mut egui::Ui, cell: f32, size: SelectionSize) -> Vec<egui::Rect> {
    let rows = [
        (t!("selection.columns"), size.columns),
        (t!("selection.rows"), size.lines),
        (t!("selection.characters"), size.characters),
    ];
    let title = t!("selection.title");
    let labels: Vec<String> = rows.iter().map(|(name, _)| name.to_string()).collect();
    let values: Vec<String> = rows.iter().map(|(_, count)| count.to_string()).collect();

    ui.set_width(width(ui, cell, &title, &labels, &values));
    ui.label(egui::RichText::new(title).strong());

    egui::Grid::new(ui.make_persistent_id("selection_counts"))
        .num_columns(2)
        .show(ui, |ui| {
            labels
                .iter()
                .zip(&values)
                .map(|(label, value)| {
                    ui.label(label);
                    let placed = ui
                        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(value).rect
                        })
                        .inner;
                    ui.end_row();
                    placed
                })
                .collect()
        })
        .inner
}

/// Width of the plate, rounded up to a whole number of cells.
///
/// It is the widest row it has to hold, so no row is wrapped and no number is
/// pushed off the edge, and it is the same width while a count grows a digit —
/// a plate that changes size as the drag runs is a plate that is read twice.
fn width(ui: &egui::Ui, cell: f32, title: &str, labels: &[String], values: &[String]) -> f32 {
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let values: Vec<&str> = values.iter().map(String::as_str).collect();
    let row = label_width(ui, &labels) + label_width(ui, &values);
    let wanted = row.max(label_width(ui, &[title]));
    (wanted / cell).ceil() * cell
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 100.0))
    }

    #[test]
    fn the_plate_stands_across_the_page_from_the_moving_end() {
        let area = area();
        let cases = [
            (area.left_top(), egui::Align2::RIGHT_BOTTOM),
            (area.right_top(), egui::Align2::LEFT_BOTTOM),
            (area.left_bottom(), egui::Align2::RIGHT_TOP),
            (area.right_bottom(), egui::Align2::LEFT_TOP),
        ];

        for (edge, expected) in cases {
            assert_eq!(pivot(None, Some(edge), area), expected, "end at {edge:?}");
        }
    }

    #[test]
    fn an_end_off_the_page_leaves_the_plate_where_it_stands() {
        let area = area();
        assert_eq!(
            pivot(Some(egui::Align2::LEFT_TOP), None, area),
            egui::Align2::LEFT_TOP,
            "the end is scrolled away, so the plate does not move"
        );
        assert_eq!(
            pivot(None, None, area),
            egui::Align2::RIGHT_BOTTOM,
            "with no corner either, it goes where a hand is least likely to be"
        );
    }

    #[test]
    fn the_end_is_taken_in_the_middle_of_its_cell() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(80.0, 32.0));
        let cell = egui::vec2(8.0, 16.0);
        assert_eq!(edge_position(area, cell, 0, 0), egui::pos2(4.0, 8.0));
        assert_eq!(edge_position(area, cell, 9, 1), egui::pos2(76.0, 24.0));
    }

    #[test]
    fn every_corner_of_the_plate_falls_on_a_cell() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(85.0, 45.0));
        let cell = egui::vec2(8.0, 16.0);

        for corner in [egui::Align2::LEFT_TOP, egui::Align2::RIGHT_BOTTOM] {
            let at = anchor(area, cell, corner);
            assert_eq!((at.x - area.left()) % cell.x, 0.0, "{corner:?} across");
            assert_eq!((at.y - area.top()) % cell.y, 0.0, "{corner:?} down");
        }
    }

    #[test]
    fn the_counts_end_where_the_plate_does() {
        let context = egui::Context::default();
        let cell = 8.0;
        let size = SelectionSize {
            columns: 7,
            lines: 120,
            characters: 4096,
        };

        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let placed = egui::Frame::popup(ui.style())
                .show(ui, |ui| counts(ui, cell, size))
                .inner;
            let edge = placed[0].right();

            assert_eq!(placed.len(), 3);
            for rect in &placed {
                assert!(
                    (rect.right() - edge).abs() < 0.5,
                    "a count stands at {} and the others at {edge}",
                    rect.right()
                );
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn the_plate_keeps_a_cell_between_itself_and_the_grid() {
        let area = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(80.0, 48.0));
        let cell = egui::vec2(8.0, 16.0);

        assert_eq!(
            anchor(area, cell, egui::Align2::LEFT_TOP),
            egui::pos2(8.0, 16.0)
        );
        assert_eq!(
            anchor(area, cell, egui::Align2::RIGHT_BOTTOM),
            egui::pos2(72.0, 32.0)
        );
    }
}
