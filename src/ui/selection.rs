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
//! It stands in the corner the pointer is furthest from, because the pointer is
//! the end of the selection: a plate under the hand would cover the very rows
//! being taken, and the corner opposite is the one place on the page a drag
//! that began anywhere never reaches.
//!
//! The corner is decided while the selection is being made and kept from the
//! moment the button is let go of. Once the drag is over the pointer is on its
//! way somewhere else — a menu, another window, the button that copies — and
//! nothing about the selection has changed, so a plate that followed it would
//! be a plate moving for no reason. It picks a corner again with the next
//! selection.
//!
//! The plate is laid on the cell grid of the terminal — both corners of it fall
//! on a cell boundary and its width is a whole number of cells — so it covers
//! whole characters and never half of one. A frame whose edge runs down the
//! middle of a column leaves a sliver of every letter behind it, which reads as
//! damaged output rather than as something lying over it.
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
/// plate may reach into. `selecting` is whether the selection is still being
/// made, which is what decides whether the plate may move.
pub fn plate(app: &mut App, ui: &mut egui::Ui, area: egui::Rect, selecting: bool) {
    let size = app.session.terminal.selection_size();
    let Some(size) = size.filter(|size| size.characters > 0) else {
        app.ui.selection_corner = None;
        return;
    };

    let cell = app.font.cell_size(ui.ctx());
    let pivot = pivot(
        app.ui.selection_corner,
        selecting,
        ui.ctx().pointer_latest_pos(),
        area,
    );
    app.ui.selection_corner = Some(pivot);

    egui::Area::new(ui.id().with("selection_plate"))
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(false)
        .fixed_pos(anchor(area, cell, pivot))
        .pivot(pivot)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| counts(ui, cell.x, size));
        });
}

/// The corner the plate takes now.
///
/// While the selection is being made it is the one across from the pointer, and
/// from the moment it is not it is the one the plate already stands in. A plate
/// that has no corner yet takes one either way: that is the frame the selection
/// appeared on.
fn pivot(
    kept: Option<egui::Align2>,
    selecting: bool,
    pointer: Option<egui::Pos2>,
    area: egui::Rect,
) -> egui::Align2 {
    match kept {
        Some(kept) if !selecting => kept,
        _ => corner(pointer, area),
    }
}

/// The corner the plate takes: the one the pointer is furthest from.
///
/// A pointer that has left the window keeps the corner it last asked for, which
/// is what `pointer_latest_pos` answers; with no pointer at all the plate goes
/// where a hand coming to the text is least likely to be.
fn corner(pointer: Option<egui::Pos2>, area: egui::Rect) -> egui::Align2 {
    let Some(pointer) = pointer else {
        return egui::Align2::RIGHT_BOTTOM;
    };
    let center = area.center();
    let x = match pointer.x < center.x {
        true => egui::Align::Max,
        false => egui::Align::Min,
    };
    let y = match pointer.y < center.y {
        true => egui::Align::Max,
        false => egui::Align::Min,
    };
    egui::Align2([x, y])
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
    fn the_plate_stands_across_the_page_from_the_pointer() {
        let area = area();
        let cases = [
            (area.left_top(), egui::Align2::RIGHT_BOTTOM),
            (area.right_top(), egui::Align2::LEFT_BOTTOM),
            (area.left_bottom(), egui::Align2::RIGHT_TOP),
            (area.right_bottom(), egui::Align2::LEFT_TOP),
        ];

        for (pointer, expected) in cases {
            assert_eq!(
                corner(Some(pointer), area),
                expected,
                "pointer at {pointer:?}"
            );
        }
    }

    #[test]
    fn the_plate_moves_while_the_selection_is_being_made() {
        let area = area();
        let kept = Some(egui::Align2::LEFT_TOP);
        let pointer = Some(area.left_top());

        assert_eq!(
            pivot(kept, true, pointer, area),
            egui::Align2::RIGHT_BOTTOM,
            "the drag runs, so the corner is the one across from the pointer"
        );
        assert_eq!(
            pivot(kept, false, pointer, area),
            egui::Align2::LEFT_TOP,
            "the drag is over, so the plate stands where it stood"
        );
        assert_eq!(
            pivot(None, false, pointer, area),
            egui::Align2::RIGHT_BOTTOM,
            "a selection that has no plate yet takes a corner whatever happened"
        );
    }

    #[test]
    fn a_pointer_nobody_has_seen_leaves_the_plate_in_one_corner() {
        assert_eq!(corner(None, area()), egui::Align2::RIGHT_BOTTOM);
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
