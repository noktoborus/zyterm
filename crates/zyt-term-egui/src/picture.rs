//! Pictures of the grid, drawn as textures of the toolkit.
//!
//! A picture is not a glyph and does not belong in the atlas the glyphs are cut
//! from, so each one is a texture of its own, uploaded the first time it is
//! seen and kept until it has not been seen for a while. The keeping is what
//! makes a picture scrolled off the page and back again cost nothing: the
//! snapshot stops carrying it, and the texture is still there.
//!
//! What is drawn is one quad per run of cells. A cell of a picture says which
//! cell of it that is, so the part to draw is read off the cell and nothing has
//! to look for the rest of the picture; neighbouring cells of one row are then
//! merged, so a picture of eighty columns is one quad a row rather than eighty.
//!
//! A run is drawn only while it starts at the left edge of the picture: the
//! emulation cuts what a narrower page cannot hold, so a run that begins
//! anywhere else is not a row of a picture at all. Every cell of a picture that
//! is drawn draws no character of its own, which is `crate::view`; a cell of a
//! picture nothing draws — folded, or dropped by the store — shows the line it
//! carries.

use egui::{Rect, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use zyt_term::{ImageId, RenderableContent, SnapshotPicture};

/// Frames a picture is kept for after the last one it was seen in.
///
/// A picture is uploaded once and scrolls on and off the page as the output
/// does, so it is kept well past the frame it left: the cost of keeping it is
/// memory the terminal is already holding the pixels of, and the cost of
/// dropping it too early is an upload every time a page is scrolled over.
const KEPT_FRAMES: u64 = 600;

/// The textures of the pictures of one view.
#[derive(Default)]
pub struct Pictures {
    kept: HashMap<ImageId, Kept>,
    frame: u64,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pictures")
            .field("kept", &self.kept.len())
            .finish_non_exhaustive()
    }
}

/// Neighbouring cells of one row of one picture, drawn as one quad.
#[derive(Debug, Clone, Copy)]
struct Run {
    /// What the first cell of the run says about itself.
    part: zyt_term::ImagePart,
    /// Column of the page the run begins in.
    column: usize,
    /// Row of the page it stands in.
    row: usize,
    /// How many cells it covers.
    cells: usize,
}

/// The runs of cells the pictures of a page are drawn as.
///
/// One run is one quad: neighbouring cells of one row of one picture that go on
/// saying the next cell of it. A run that does not start at the left edge of
/// the picture is what the page wrapped onto the line below, and it is cut off
/// rather than drawn ([`begins_a_row`]).
///
/// It is read off the page before anything is painted, because the text of the
/// page is laid out against it: every cell of a picture that is drawn draws no
/// character of its own.
#[derive(Debug, Default)]
pub(crate) struct Drawn {
    runs: Vec<Run>,
}

impl Drawn {
    /// Reads the page and answers which runs will be painted.
    pub(crate) fn of(content: &RenderableContent) -> Self {
        let mut runs = Vec::new();
        if content.pictures.is_empty() {
            return Self { runs };
        }

        for (row, cells) in content.rows().take(content.rows).enumerate() {
            let mut column = 0;
            while column < cells.len() {
                let Some(part) = cells[column].image else {
                    column += 1;
                    continue;
                };
                let drawn = content
                    .picture(part.id)
                    .is_some_and(|picture| !picture.folded);

                let mut last = column;
                while last + 1 < cells.len()
                    && cells[last + 1].image.is_some_and(|next| {
                        next.id == part.id
                            && next.row == part.row
                            && next.column == part.column + (last + 1 - column)
                    })
                {
                    last += 1;
                }

                let run = Run {
                    part,
                    column,
                    row,
                    cells: last + 1 - column,
                };
                if drawn && begins_a_row(run) {
                    runs.push(run);
                }
                column = last + 1;
            }
        }
        Self { runs }
    }
}

/// One picture, as the toolkit holds it.
struct Kept {
    texture: TextureHandle,
    size: (usize, usize),
    seen: u64,
}

impl Pictures {
    /// Draws the runs of the page, and forgets the textures of the pictures
    /// that have been gone for a while.
    ///
    /// Which runs those are was worked out by [`Drawn`], before the text of the
    /// page was laid out, because the two answers are one: a cell a picture is
    /// drawn over is a cell that draws no character.
    pub fn draw(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        content: &RenderableContent,
        drawn: &Drawn,
        origin: egui::Pos2,
        cell: Vec2,
    ) {
        self.frame = self.frame.wrapping_add(1);
        for run in &drawn.runs {
            let Some(picture) = content.picture(run.part.id) else {
                continue;
            };
            self.paint_run(ui, painter, picture, *run, origin, cell);
        }
        self.forget_the_gone();
    }

    /// Draws one run of cells of one picture.
    ///
    /// The quad is the cells and the texture is stretched over it, so what is
    /// drawn fills the rectangle the picture was placed in. The pixels of a
    /// picture rarely divide into whole cells, and the remainder is less than
    /// one cell in each direction: stretching it is a fraction of a cell
    /// either way, where leaving it would be a gap at the edge of every
    /// picture.
    fn paint_run(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        picture: &SnapshotPicture,
        run: Run,
        origin: egui::Pos2,
        cell: Vec2,
    ) {
        let (columns, rows) = (picture.cells.0.max(1), picture.cells.1.max(1));
        let texture = self.texture(ui, picture);

        let uv = Rect::from_min_max(
            egui::pos2(
                run.part.column as f32 / columns as f32,
                run.part.row as f32 / rows as f32,
            ),
            egui::pos2(
                (run.part.column + run.cells) as f32 / columns as f32,
                (run.part.row + 1) as f32 / rows as f32,
            ),
        );
        let dest = Rect::from_min_size(
            egui::pos2(
                origin.x + run.column as f32 * cell.x,
                origin.y + run.row as f32 * cell.y,
            ),
            Vec2::new(run.cells as f32 * cell.x, cell.y),
        );

        painter.add(egui::Shape::image(texture, dest, uv, egui::Color32::WHITE));
    }

    /// The texture of a picture, uploaded the first time it is asked for.
    fn texture(&mut self, ui: &egui::Ui, picture: &SnapshotPicture) -> egui::TextureId {
        let size = (picture.pixels.width, picture.pixels.height);
        let kept = self.kept.entry(picture.id).and_modify(|kept| {
            if kept.size != size {
                kept.texture = upload(ui, picture);
                kept.size = size;
            }
            kept.seen = self.frame;
        });

        let frame = self.frame;
        let kept = kept.or_insert_with(|| Kept {
            texture: upload(ui, picture),
            size,
            seen: frame,
        });
        kept.seen = frame;
        kept.texture.id()
    }

    /// Drops the textures of pictures nothing has drawn for a while.
    fn forget_the_gone(&mut self) {
        let frame = self.frame;
        self.kept
            .retain(|_, kept| frame.saturating_sub(kept.seen) < KEPT_FRAMES);
    }
}

/// Whether a run is the beginning of a row of the picture.
///
/// A row of a picture starts at its left edge, so a run that does is the row —
/// as much of it as the page holds, since the emulation cuts the cells a
/// narrower page cannot hold. A run that begins anywhere else is no row of a
/// picture: cells marked by a terminal that kept its pictures out of sight and
/// was then asked to draw them, and a strip of a picture is read as part of one.
///
/// A row cut off by the top or the bottom of the page is a row like any other:
/// the page holds fewer of them, not narrower ones.
fn begins_a_row(run: Run) -> bool {
    run.part.column == 0
}

/// Hands the pixels of a picture to the toolkit.
///
/// Magnified with no filter and minified with one: a picture drawn larger than
/// its pixels is a picture whose pixels somebody wants to see, and one drawn
/// smaller is a picture that would come apart without averaging.
fn upload(ui: &egui::Ui, picture: &SnapshotPicture) -> TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [picture.pixels.width, picture.pixels.height],
        &picture.pixels.rgba,
    );
    ui.ctx().load_texture(
        format!("zyt-picture-{}", picture.id.0),
        image,
        TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Linear,
            ..TextureOptions::default()
        },
    )
}
