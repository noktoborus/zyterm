//! The room the plates over the terminal take, one frame at a time.
//!
//! Several plates rise against the head or the foot of the terminal: the
//! signals edge to edge, the times above the status bar, the counts of a
//! selection in a corner. Each was placed as if it stood there alone, so two of
//! them that stood at once lay one over the other, and the one underneath was a
//! plate nobody could read.
//!
//! So each plate that stands says where it stood, and a plate drawn after it
//! stands past it: above it at the foot, below it at the head. It stands where it
//! always did when nothing is in the way. Only a plate it would cross counts —
//! one in the other corner of the same edge is not in the way.
//!
//! What was taken is of this frame alone and is cleared at the start of the next,
//! so a plate that went away leaves no room behind it. The order is the order
//! they are drawn in: the signals first, then the times, then the selection.

use crate::ui::PLATE_GAP;

/// The layer every plate over the terminal is drawn in.
///
/// One below the plate of a command of several lines, the menus and the
/// windows that ask, which stand in `Foreground`. A layer keeps its areas in the
/// order they last rose, so a plate of the signals raised while a command is
/// being written would lie over the text being written. The command is the one
/// plate somebody is typing into, and nothing that is only read may cover it.
pub const ORDER: egui::Order = egui::Order::Middle;

/// The edge of the terminal a plate stands against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The head of the terminal.
    Top,
    /// The foot of the terminal, above the status bar.
    Bottom,
}

/// The plates drawn so far in this frame, with the edge each stands against.
#[derive(Debug, Default)]
pub struct Plates {
    taken: Vec<(Edge, egui::Rect)>,
}

impl Plates {
    /// Forgets every plate, at the start of a frame.
    pub fn clear(&mut self) {
        self.taken.clear();
    }

    /// Notes a plate that stands against that edge.
    pub fn take(&mut self, edge: Edge, rect: egui::Rect) {
        self.taken.push((edge, rect));
    }

    /// Where the near side of a plate across `span` stands, when it would stand
    /// at `at` alone: a gap past every plate against the same edge it crosses.
    pub fn free(&self, edge: Edge, span: egui::Rangef, at: f32) -> f32 {
        self.taken
            .iter()
            .filter(|(taken, rect)| *taken == edge && rect.x_range().intersects(span))
            .fold(at, |at, (_, rect)| match edge {
                Edge::Top => at.max(rect.bottom() + PLATE_GAP),
                Edge::Bottom => at.min(rect.top() - PLATE_GAP),
            })
    }
}

/// How far across a plate reaches from `x`: rightwards when it hangs from its
/// left side, leftwards otherwise.
///
/// The width is the one it was drawn at in the frame before, because a plate is
/// placed before it is laid out. A plate that was not there before is as wide as
/// that one point, which is the corner it hangs from.
pub fn span(context: &egui::Context, id: egui::Id, x: f32, pivot: egui::Align) -> egui::Rangef {
    let width = context
        .memory(|memory| memory.area_rect(id))
        .map_or(0.0, |rect| rect.width());
    match pivot {
        egui::Align::Min => egui::Rangef::new(x, x + width),
        _ => egui::Rangef::new(x - width, x),
    }
}

/// Where a plate pinned by a press stands after the next one: at the foot, then
/// at the head, then nowhere.
pub fn next_pin(pin: Option<Edge>) -> Option<Edge> {
    match pin {
        None => Some(Edge::Bottom),
        Some(Edge::Bottom) => Some(Edge::Top),
        Some(Edge::Top) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom))
    }

    #[test]
    fn a_plate_stands_past_the_one_it_would_cross() {
        let mut plates = Plates::default();
        plates.take(Edge::Bottom, rect(0.0, 300.0, 800.0, 400.0));
        plates.take(Edge::Top, rect(0.0, 0.0, 800.0, 100.0));

        let across = egui::Rangef::new(10.0, 50.0);
        assert_eq!(plates.free(Edge::Bottom, across, 390.0), 300.0 - PLATE_GAP);
        assert_eq!(plates.free(Edge::Top, across, 10.0), 100.0 + PLATE_GAP);
    }

    #[test]
    fn a_plate_with_nothing_in_its_way_stands_where_it_always_did() {
        let mut plates = Plates::default();
        plates.take(Edge::Bottom, rect(0.0, 300.0, 200.0, 400.0));

        let beside = egui::Rangef::new(600.0, 700.0);
        assert_eq!(
            plates.free(Edge::Bottom, beside, 390.0),
            390.0,
            "the other corner"
        );
        let across = egui::Rangef::new(0.0, 100.0);
        assert_eq!(plates.free(Edge::Top, across, 10.0), 10.0, "the other edge");

        plates.clear();
        assert_eq!(
            plates.free(Edge::Bottom, across, 390.0),
            390.0,
            "a new frame"
        );
    }

    #[test]
    fn plates_at_one_edge_stack() {
        let mut plates = Plates::default();
        plates.take(Edge::Bottom, rect(0.0, 300.0, 800.0, 400.0));
        plates.take(Edge::Bottom, rect(0.0, 200.0, 300.0, 294.0));

        let across = egui::Rangef::new(10.0, 50.0);
        assert_eq!(plates.free(Edge::Bottom, across, 390.0), 200.0 - PLATE_GAP);
    }

    #[test]
    fn a_press_takes_the_plate_down_then_up_then_away() {
        assert_eq!(next_pin(None), Some(Edge::Bottom));
        assert_eq!(next_pin(Some(Edge::Bottom)), Some(Edge::Top));
        assert_eq!(next_pin(Some(Edge::Top)), None);
    }
}
