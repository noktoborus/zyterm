//! File dialog of a transfer, shown in place of the terminal.

use crate::app::App;

/// Share of the area left free along each edge.
///
/// The dialog stands where the terminal stands, and standing there edge to edge
/// it reads as the window itself rather than as something opened inside it. A
/// hundredth of the width and of the height is enough of the panel behind it to
/// see that it is a thing with a border, and little enough that nothing of the
/// list is lost to it.
const EDGE_SHARE: f32 = 0.01;

/// Draws the dialog inside the given area and starts the transfer on a pick.
pub fn draw(app: &mut App, context: &egui::Context, area: egui::Rect) {
    let area = inset(area);
    let config = app.file_dialog.config_mut();
    config.fixed_pos = Some(area.min);
    config.default_size = area.size();
    config.min_size = area.size();
    config.max_size = Some(area.size());
    config.resizable = false;
    config.movable = false;
    config.title_bar = false;
    config.as_modal = false;

    app.file_dialog.update(context);
    app.take_picked_file();
}

/// The area less a hundredth of it along every edge.
fn inset(area: egui::Rect) -> egui::Rect {
    area.shrink2(egui::Vec2::new(
        area.width() * EDGE_SHARE,
        area.height() * EDGE_SHARE,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each edge gives up a hundredth of the side it lies along, so the dialog
    /// keeps the middle of the area and loses a fiftieth of each side.
    #[test]
    fn every_edge_keeps_a_hundredth_of_the_area_free() {
        let area =
            egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::Vec2::new(800.0, 600.0));

        let inside = inset(area);

        assert_eq!(inside.min, egui::Pos2::new(18.0, 26.0));
        assert_eq!(inside.size(), egui::Vec2::new(784.0, 588.0));
        assert_eq!(inside.center(), area.center());
    }
}
