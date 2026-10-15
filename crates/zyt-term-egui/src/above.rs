//! Row −1: a row past the oldest line of the scrollback, which the caller may
//! give the view to offer what lies further up — history kept somewhere else,
//! loaded when asked for.
//!
//! The widget does the general part: the view may scroll one row past the
//! oldest line onto it, it is drawn as the line of a gap is, a click on it is
//! reported, and the caller is told when the view stands on it. What the row
//! says, and what standing on it or pressing it does, is the caller's: the
//! words are handed in ([`AboveTop`]), since this crate holds no text.

use egui::{Align2, Color32, FontId, Painter, Rect, Shape, Stroke, Vec2};

/// What row −1 says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboveTop {
    /// The text of the row, drawn between the sign and the rule.
    pub text: String,
}

/// Cells of the rule before the sign.
const LEAD: usize = 2;

/// Whether the view of the widget `id` stands on row −1.
pub(crate) fn standing(ui: &egui::Ui, id: egui::Id) -> bool {
    ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false))
}

/// The view of the widget `id` steps onto row −1, or off it.
pub(crate) fn stand(ui: &egui::Ui, id: egui::Id, on: bool) {
    ui.data_mut(|data| {
        if on {
            data.insert_temp(id, true);
        } else {
            data.remove_temp::<bool>(id);
        }
    });
}

/// Paints row −1 across `row`: the dashes of a rule, the sign of more above —
/// an arrow up to a bar — and the text, in the colour `color` the line of a
/// gap is drawn in.
pub(crate) fn paint(
    painter: &Painter,
    row: Rect,
    cell: Vec2,
    font: &FontId,
    text: &str,
    color: Color32,
    background: Color32,
) {
    painter.rect_filled(row, 0.0, background);
    let columns = (row.width() / cell.x).floor() as usize;
    let width = (cell.y / 12.0).clamp(1.0, 2.0);
    let stroke = Stroke::new(width, color);
    let middle = row.center().y;
    let cell_at = |column: usize| {
        Rect::from_min_size(
            egui::pos2(row.left() + column as f32 * cell.x, row.top()),
            cell,
        )
    };
    let dash = |painter: &Painter, at: Rect| {
        painter.line_segment(
            [
                egui::pos2(at.left() + at.width() * 0.15, middle),
                egui::pos2(at.left() + at.width() * 0.75, middle),
            ],
            stroke,
        );
    };

    for column in 0..LEAD.min(columns) {
        dash(painter, cell_at(column));
    }
    if columns <= LEAD + 2 {
        return;
    }

    let sign = cell_at(LEAD + 1);
    let x = sign.center().x;
    let top = sign.top() + sign.height() * 0.2;
    let bottom = sign.bottom() - sign.height() * 0.2;
    let wing = sign.width() * 0.3;
    painter.add(Shape::line(
        vec![
            egui::pos2(x - wing, top + wing),
            egui::pos2(x, top),
            egui::pos2(x + wing, top + wing),
        ],
        stroke,
    ));
    painter.line_segment([egui::pos2(x, top), egui::pos2(x, bottom)], stroke);

    let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    let text_at = cell_at(LEAD + 3).left_top();
    let text_cells = (galley.size().x / cell.x).ceil() as usize;
    painter.galley(
        Align2::LEFT_TOP.anchor_size(text_at, galley.size()).min,
        galley,
        color,
    );
    for column in (LEAD + 4 + text_cells)..columns {
        dash(painter, cell_at(column));
    }
}
