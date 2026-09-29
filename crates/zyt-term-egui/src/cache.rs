//! The picture of the grid, kept between frames.
//!
//! Laying a character out, turning it into a quad and cutting that quad into
//! triangles is the whole cost of a frame, and nothing about it changes while
//! the terminal says nothing new. The picture is therefore built once and kept
//! as one mesh, and a frame that would paint the same picture again paints that
//! mesh.

use crate::font::TerminalFont;
use crate::theme::TerminalTheme;
use egui::epaint::Mesh;
use egui::{Rect, Vec2};
use std::sync::Arc;
use zyt_term::{RenderableContent, Terminal};

/// What the kept picture was built from.
///
/// Everything the grid is painted from stands here, so two frames that agree on
/// all of it would paint the same picture. The page itself is not in it: it is
/// compared where it is rendered, and what stands here is the count of the
/// pages that were another one than the page before them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Painted {
    /// How many pages other than the one before them were rendered.
    pub revision: u64,
    /// Where the grid stands.
    pub area: Rect,
    /// Size of one cell.
    pub cell: Vec2,
    /// Scale the text was laid out for.
    pub pixels_per_point: f32,
    /// Size of the font atlas the glyphs were cut against.
    ///
    /// A mesh names its glyphs by where they stand in that image, as a share of
    /// its size, so an atlas that grew moves every one of them.
    pub font_texture: [usize; 2],
    /// Font the text was laid out with.
    pub font: TerminalFont,
    /// Colors the cells were resolved against.
    pub theme: TerminalTheme,
    /// Whether the colors a program set were taken into account.
    pub program_colors: bool,
    /// Whether the links a program reported were underlined.
    pub links: bool,
}

/// The picture of one terminal view, kept between frames.
///
/// One cache belongs to one view and is held by the caller across frames, the
/// way the snapshot of the grid is. It holds what was last painted and the page
/// it was painted from, so dropping it costs nothing but one rebuilt frame.
#[derive(Debug, Default)]
pub struct TerminalCache {
    mesh: Option<Arc<Mesh>>,
    painted: Option<Painted>,
    revision: u64,
    spare: RenderableContent,
    empty: Mesh,
}

impl TerminalCache {
    /// An empty cache, which paints the first frame in full.
    pub fn new() -> Self {
        Self::default()
    }

    /// Throws the kept picture away.
    ///
    /// A mesh of text names the glyphs of the font it was cut from, where they
    /// stand in the atlas of the toolkit. Fonts that are replaced leave those
    /// glyphs behind, so a caller that changes the fonts of the toolkit says so
    /// here and the next frame lays the text out again.
    pub fn forget(&mut self) {
        self.mesh = None;
        self.painted = None;
    }

    /// The mesh of the picture before this one, emptied and ready to be filled
    /// again.
    ///
    /// A page is tens of thousands of vertices, and a picture that allocates
    /// them again has nothing to show for it.
    pub(crate) fn take_mesh(&mut self) -> Mesh {
        let mut mesh = std::mem::take(&mut self.empty);
        mesh.vertices.clear();
        mesh.indices.clear();
        mesh.texture_id = egui::TextureId::default();
        mesh
    }

    /// Reads the terminal again and says whether the page it answers is another
    /// one than the page the kept picture was built from.
    ///
    /// A terminal that was touched is not a terminal that looks different: a
    /// key that was pressed, a selection that was dragged over the same cells,
    /// a program that repainted a row with what already stood there all leave
    /// the grid saying it changed while the picture of it is the same. Building
    /// that picture again is the whole cost of a frame, so the page is compared
    /// before it is paid.
    ///
    /// The page is compared whole and nothing is followed per cell or per row:
    /// the picture is one mesh, so a difference in one cell costs exactly what
    /// a difference in every cell costs, and finding out where it is buys
    /// nothing.
    ///
    /// The cursor is not compared, because it is not in the picture: it is
    /// drawn over it every frame. It moves with nearly every byte a device
    /// sends and it blinks on its own, and a picture told apart by it would be
    /// built again for a cursor that stood still on the same letter.
    ///
    /// The page that was rendered is handed to the caller and the page before
    /// it is kept here, so the two swap buffers and neither is ever copied.
    pub(crate) fn read_page(&mut self, terminal: &mut Terminal, content: &mut RenderableContent) {
        terminal.render_into(&mut self.spare);
        if !same_page(&self.spare, content) {
            self.revision = self.revision.wrapping_add(1);
        }
        std::mem::swap(&mut self.spare, content);
    }

    /// How many pages other than the one before them were rendered, which is
    /// what tells two pictures of the same grid apart.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether the kept mesh is the picture this frame would paint.
    pub(crate) fn holds(&self, painted: &Painted) -> bool {
        self.painted.as_ref() == Some(painted) && self.mesh.is_some()
    }

    /// Whether the kept picture names its glyphs in an atlas of another size
    /// than the one the toolkit holds now.
    ///
    /// A mesh of text says where each glyph stands in the atlas as a share of
    /// its size, so an atlas that grew moves every one of them and a picture
    /// built against the size before is drawn with the wrong part of the image
    /// in every cell. The atlas grows when something is laid out that was not
    /// there before — a panel that opened, a hint under the pointer, a letter
    /// the page had never shown — and that can happen after this picture was
    /// painted and kept, in the same pass.
    ///
    /// The picture is built again on the pass that notices, which is what makes
    /// this worth asking at the end of one: a window that is not asked for
    /// another pass does not draw one, and what stands on the screen stays
    /// wrong until something else asks — a key, a resized window.
    pub fn wants_another_pass(&self, atlas: [usize; 2]) -> bool {
        self.painted
            .as_ref()
            .is_some_and(|painted| painted.font_texture != atlas)
    }

    /// True while there is no kept picture at all.
    ///
    /// A cache that holds nothing has to read the page before it paints one,
    /// whatever the terminal says about having changed: the page it would paint
    /// from is the one left over from the last picture it built, and a caller
    /// that threw its picture away — the fonts were replaced, the palette
    /// changed — has no reason to have touched the terminal as well.
    pub(crate) fn is_empty(&self) -> bool {
        self.mesh.is_none()
    }

    /// Keeps the mesh of the grid, together with what it was painted from, and
    /// takes the buffers of the picture it replaces for the next one.
    pub(crate) fn keep(&mut self, painted: Painted, mesh: Mesh) {
        if let Some(old) = self.mesh.replace(Arc::new(mesh))
            && let Some(old) = Arc::into_inner(old)
        {
            self.empty = old;
        }
        self.painted = Some(painted);
    }

    /// The kept mesh, to be painted again.
    pub(crate) fn mesh(&self) -> Option<Arc<Mesh>> {
        self.mesh.clone()
    }
}

/// Whether two snapshots would be painted into the same picture.
///
/// The cells of the active page and the colors they are resolved against are
/// the whole of it. What a snapshot carries beside them — the cursor, the
/// offset into the history, how much history there is, the modes of the
/// terminal and the addresses of its links — is drawn beside the picture or not
/// drawn at all, and a page whose cells agree carries the same link in the same
/// cell either way.
fn same_page(left: &RenderableContent, right: &RenderableContent) -> bool {
    left.columns == right.columns
        && left.rows == right.rows
        && left.background == right.background
        && left.foreground == right.foreground
        && left.cells == right.cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(text: &str) -> RenderableContent {
        let mut content = RenderableContent {
            columns: text.chars().count(),
            rows: 1,
            ..RenderableContent::default()
        };
        content.cells = text
            .chars()
            .map(|ch| zyt_term::Cell {
                ch,
                ..zyt_term::Cell::default()
            })
            .collect();
        content
    }

    fn terminal_showing(text: &str) -> Terminal {
        let mut terminal = Terminal::new(zyt_term::TerminalConfig {
            columns: 20,
            rows: 4,
            ..zyt_term::TerminalConfig::default()
        })
        .expect("the terminal is created");
        terminal.feed(text.as_bytes());
        terminal
    }

    fn painted(revision: u64) -> Painted {
        Painted {
            revision,
            area: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(100.0, 100.0)),
            cell: Vec2::new(8.0, 16.0),
            pixels_per_point: 1.0,
            font_texture: [128, 128],
            font: TerminalFont::default(),
            theme: TerminalTheme::dark(),
            program_colors: true,
            links: true,
        }
    }

    #[test]
    fn a_picture_that_did_not_change_is_the_one_that_is_kept() {
        let mut cache = TerminalCache::new();
        cache.keep(painted(0), Mesh::default());

        assert!(cache.holds(&painted(0)));
        assert!(cache.mesh().is_some());
    }

    #[test]
    fn an_empty_cache_holds_nothing() {
        let cache = TerminalCache::new();

        assert!(!cache.holds(&painted(0)));
        assert!(cache.mesh().is_none());
    }

    #[test]
    fn a_page_that_is_another_one_is_painted_again() {
        let mut cache = TerminalCache::new();
        let mut terminal = terminal_showing("ab");
        let mut content = RenderableContent::default();
        cache.read_page(&mut terminal, &mut content);
        cache.keep(painted(cache.revision()), Mesh::default());

        terminal.feed(b"\r\nc");
        cache.read_page(&mut terminal, &mut content);

        assert!(!cache.holds(&painted(cache.revision())));
    }

    #[test]
    fn a_page_that_says_the_same_is_not_painted_again() {
        let mut cache = TerminalCache::new();
        let mut terminal = terminal_showing("ab");
        let mut content = RenderableContent::default();
        cache.read_page(&mut terminal, &mut content);
        cache.keep(painted(cache.revision()), Mesh::default());

        terminal.feed(b"\rab");
        cache.read_page(&mut terminal, &mut content);

        assert!(cache.holds(&painted(cache.revision())));
    }

    #[test]
    fn a_cursor_that_moved_alone_keeps_the_picture() {
        let mut cache = TerminalCache::new();
        let mut terminal = terminal_showing("ab");
        let mut content = RenderableContent::default();
        cache.read_page(&mut terminal, &mut content);
        cache.keep(painted(cache.revision()), Mesh::default());
        let cursor = content.cursor;

        terminal.feed(b"\r");
        cache.read_page(&mut terminal, &mut content);

        assert_ne!(cursor, content.cursor);
        assert!(cache.holds(&painted(cache.revision())));
    }

    #[test]
    fn the_page_that_was_read_reaches_the_caller() {
        let mut cache = TerminalCache::new();
        let mut terminal = terminal_showing("ab");
        let mut content = RenderableContent::default();

        cache.read_page(&mut terminal, &mut content);

        assert_eq!(content.cell(0, 0).expect("the cell is there").ch, 'a');
        assert_eq!(content.cell(1, 0).expect("the cell is there").ch, 'b');
    }

    #[test]
    fn two_pages_of_the_same_cells_are_one_page() {
        assert!(same_page(&content("ab"), &content("ab")));
        assert!(!same_page(&content("ab"), &content("ac")));
        assert!(!same_page(&content("ab"), &content("abc")));
    }

    #[test]
    fn a_grid_that_moved_is_painted_again() {
        let mut cache = TerminalCache::new();
        cache.keep(painted(0), Mesh::default());

        let moved = Painted {
            area: Rect::from_min_size(egui::pos2(10.0, 0.0), Vec2::new(100.0, 100.0)),
            ..painted(0)
        };

        assert!(!cache.holds(&moved));
    }

    #[test]
    fn a_grid_of_another_scale_is_painted_again() {
        let mut cache = TerminalCache::new();
        cache.keep(painted(0), Mesh::default());

        assert!(!cache.holds(&Painted {
            pixels_per_point: 2.0,
            ..painted(0)
        }));
    }

    #[test]
    fn an_atlas_that_grew_is_painted_again() {
        let mut cache = TerminalCache::new();
        cache.keep(painted(0), Mesh::default());

        assert!(!cache.holds(&Painted {
            font_texture: [128, 256],
            ..painted(0)
        }));
    }

    #[test]
    fn a_cache_that_forgot_paints_everything_again() {
        let mut cache = TerminalCache::new();
        cache.keep(painted(0), Mesh::default());
        cache.forget();

        assert!(!cache.holds(&painted(0)));
        assert!(cache.mesh().is_none());
    }
}

#[cfg(test)]
mod atlas_tests {
    use super::*;

    #[test]
    fn a_picture_built_against_another_atlas_asks_for_a_pass() {
        let mut cache = TerminalCache::new();
        assert!(
            !cache.wants_another_pass([64, 64]),
            "a cache with no picture wants nothing"
        );

        let painted = Painted {
            revision: 1,
            area: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(100.0, 100.0)),
            cell: Vec2::new(8.0, 16.0),
            pixels_per_point: 1.0,
            font_texture: [64, 64],
            font: TerminalFont::default(),
            theme: TerminalTheme::dark(),
            program_colors: false,
            links: false,
        };
        cache.keep(painted, Mesh::default());

        assert!(!cache.wants_another_pass([64, 64]));
        assert!(
            cache.wants_another_pass([64, 128]),
            "an atlas that grew moves every glyph of the kept picture"
        );
    }
}
