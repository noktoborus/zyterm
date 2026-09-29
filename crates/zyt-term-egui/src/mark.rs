//! The mark of a run of NUL bytes, drawn rather than taken from a font.
//!
//! Firefox draws the character no font carries the same way — `hexbox`, in
//! `gfx/thebes/gfxFontMissingGlyphs.cpp` — and for the same reason: a code point
//! nothing carries is drawn as a box, so the one thing that must always be
//! drawable cannot be asked of a font at all. It keeps a bitmap of the sixteen
//! hexadecimal digits, three by five pixels each, and blits them with point
//! sampling at a whole-number scale, because it needs sixteen shapes.
//!
//! One shape is needed here, so it is two strokes and no bitmap: a ring with a
//! stroke across it, the empty set. Nothing is sampled and nothing is scaled, so
//! it is as sharp as the toolkit draws a circle at any size and at any scale of
//! the screen — which a bitmap of pixels is not.
//!
//! It is drawn into the shapes of the page like everything else, so it costs the
//! picture two shapes in the cell it stands in and nothing anywhere else.

use egui::{Rect, Shape, Stroke, Vec2};

/// How much of the smaller side of a cell the ring takes.
///
/// A little under three quarters: the ring has to read as a ring rather than as
/// a dot, and it has to stand clear of the frame of the block it is inside and of
/// the cell beside it.
const DIAMETER_SHARE: f32 = 0.72;

/// The thinnest and the thickest the two strokes are drawn.
///
/// A stroke under one point is drawn as a grey smear by every renderer, and one
/// over two turns the ring into a blob at the sizes a terminal is read at.
const STROKE: std::ops::RangeInclusive<f32> = 1.0..=2.0;

/// What the stroke is, as a share of the diameter of the ring.
const STROKE_SHARE: f32 = 1.0 / 6.0;

/// Where the stroke across the ring begins and ends, as a share of the radius.
///
/// Past the ring rather than inside it, which is what says that the two shapes
/// are one glyph: a line that stops at the ring reads as a ring with something
/// in it.
const SLASH_REACH: f32 = 1.15;

/// Draws the mark in the cell it stands in.
///
/// The cell and not the block: a run is the mark and the count beside it, and the
/// mark is one cell of that however many the count takes.
pub(crate) fn paint(shapes: &mut Vec<Shape>, cell: Rect, color: egui::Color32) {
    let diameter = cell.width().min(cell.height()) * DIAMETER_SHARE;
    if diameter < *STROKE.start() * 2.0 {
        return;
    }

    let radius = diameter / 2.0;
    let width = (diameter * STROKE_SHARE).clamp(*STROKE.start(), *STROKE.end());
    let stroke = Stroke::new(width, color);
    let center = cell.center();
    let reach = radius * SLASH_REACH / std::f32::consts::SQRT_2;

    shapes.push(Shape::circle_stroke(center, radius, stroke));
    shapes.push(Shape::line_segment(
        [
            center + Vec2::new(-reach, reach),
            center + Vec2::new(reach, -reach),
        ],
        stroke,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mark is two shapes, and both of them stand inside the cell it was
    /// given: a glyph drawn over the cell beside it is a glyph over somebody
    /// else's letter.
    #[test]
    fn the_mark_is_two_shapes_inside_its_cell() {
        let cell = Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(9.0, 18.0));
        let mut shapes = Vec::new();

        paint(&mut shapes, cell, egui::Color32::WHITE);

        assert_eq!(shapes.len(), 2, "a ring and the stroke across it");
        for shape in &shapes {
            let bounds = shape.visual_bounding_rect();

            assert!(
                cell.contains_rect(bounds),
                "{bounds:?} stands outside {cell:?}"
            );
        }
    }

    /// A cell too small for a ring is left empty rather than drawn as a dot.
    ///
    /// The frame of the block is still there, so the run is still marked: what
    /// goes away is the one shape that would say nothing at that size.
    #[test]
    fn a_cell_too_small_for_a_ring_carries_none() {
        let mut shapes = Vec::new();

        paint(
            &mut shapes,
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(2.0, 2.0)),
            egui::Color32::WHITE,
        );

        assert!(shapes.is_empty());
    }

    /// The stroke grows with the cell and stops, so the ring is a ring in a
    /// window of any size: a mark drawn at one width is a smear in a small cell
    /// and a blot in a large one.
    #[test]
    fn the_stroke_grows_with_the_cell_and_stops() {
        let widths: Vec<f32> = [6.0f32, 12.0, 24.0, 64.0, 256.0]
            .into_iter()
            .map(|side| {
                let mut shapes = Vec::new();
                paint(
                    &mut shapes,
                    Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(side)),
                    egui::Color32::WHITE,
                );
                match shapes.first() {
                    Some(Shape::Circle(ring)) => ring.stroke.width,
                    other => panic!("the ring is the first shape, not {other:?}"),
                }
            })
            .collect();

        assert!(widths.is_sorted(), "it never grows thinner: {widths:?}");
        assert!(
            widths[0] < widths[widths.len() - 1],
            "and a larger cell is drawn thicker: {widths:?}"
        );
        assert!(
            widths.iter().all(|width| STROKE.contains(width)),
            "{widths:?}"
        );
        assert_eq!(
            widths[widths.len() - 1],
            *STROKE.end(),
            "a cell of any size stops at the cap: {widths:?}"
        );
    }
}
