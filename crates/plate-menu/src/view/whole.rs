//! The plate of the whole of a label that was cut: where it stands and what it
//! takes from the menu.
//!
//! Beside the menu it takes width, and the menu moves left by half of it. Above
//! the menu it takes height, and the menu moves down and gives up plates until
//! the two fit the window one over the other.

use super::{PADDING, PlateMenu};
use crate::state::Row;
use egui::{Align2, Area, Color32, Context, Frame, Order, Rect, Vec2};

/// What stands between the menu and the plate beside it.
const WHOLE_GAP: f32 = 8.0;

/// What the menu took on the frame it was last drawn, apart from its plates.
///
/// The plates are what a menu under a whole gives up, so they are counted by
/// the step of one; everything else is measured, because it is laid out by the
/// toolkit and a notice wraps to as many lines as it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Chrome {
    /// The frame, the notice and the query.
    pub(super) around: f32,
    /// The line below the plates, once it has been drawn.
    pub(super) footer: Option<f32>,
}

impl Chrome {
    /// What the menu of `height` took apart from `plates` and `footer`, the
    /// line below kept from `before` while it is not drawn.
    pub(super) fn of(height: f32, plates: f32, footer: f32, before: Option<Self>) -> Self {
        Self {
            around: height - plates - footer,
            footer: (footer > 0.0)
                .then_some(footer)
                .or(before.and_then(|chrome| chrome.footer)),
        }
    }
}

/// How the menu stands under the plate of a whole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Stack {
    /// How many plates it shows.
    pub(super) plates: usize,
    /// How far below the middle of the window it stands.
    pub(super) lower: f32,
}

impl PlateMenu {
    /// How far left the menu stands to leave room for the plate beside it.
    ///
    /// The two together keep the middle of the window, so the menu moves by
    /// half of what the plate and the gap take. It moves only while the
    /// selection has a whole, and gives the room back as soon as it has none.
    ///
    /// A window too narrow for the two side by side keeps none: the plate
    /// stands above the menu there, and a menu that moved left for a plate that
    /// is not beside it would be a menu standing off the middle for nothing.
    pub(super) fn room(&self, shown: bool, width: f32, beside: bool) -> f32 {
        if !shown || !beside {
            return 0.0;
        }
        (self.beside_width(width) + WHOLE_GAP) / 2.0
    }

    /// What the plate beside the menu takes, which is what the menu takes: the
    /// frame is the same frame and the text is set to the same width.
    ///
    /// It is measured off the menu drawn last frame, because that is the one
    /// that was drawn; the width the layout asked for answers for the first
    /// frame, where there is no menu yet.
    pub(super) fn beside_width(&self, width: f32) -> f32 {
        match self.rect == Rect::NOTHING {
            true => width,
            false => self.rect.width(),
        }
    }

    /// The plate of the whole of what the label of the selected entry was cut
    /// from.
    ///
    /// It stands right of the menu, which moved left to make room for it, in an
    /// area of its own hanging from its top right corner: a whole of three
    /// lines and a whole of one leave every entry exactly where it was.
    /// Nothing of it can be chosen — it is there to be read, which is what an
    /// entry too long for its plate asks for.
    ///
    /// Where the window is too narrow for the two side by side it hangs from
    /// the top left corner instead and grows upwards, above the menu. It is
    /// above the menu and not a row inside it for the reason it hangs from a
    /// corner at all: a plate inside the frame would move the entries under the
    /// pointer every time the selection walked onto a whole of another height.
    pub(super) fn whole(&self, context: &Context, whole: Option<String>, width: f32, beside: bool) {
        if self.rect == Rect::NOTHING {
            return;
        }
        let Some(full) = whole else {
            return;
        };

        let area = Area::new(self.id.with("whole"))
            .order(Order::Foreground)
            .constrain(true)
            .movable(false);
        let area = match beside {
            true => area
                .fixed_pos(self.rect.right_top() + Vec2::new(WHOLE_GAP, 0.0))
                .pivot(Align2::LEFT_TOP),
            false => area
                .fixed_pos(self.rect.left_top() - Vec2::new(0.0, WHOLE_GAP))
                .pivot(Align2::LEFT_BOTTOM),
        };

        area.show(context, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width);
                ui.add(egui::Label::new(full).wrap());
            });
        });
    }

    /// How many plates the menu keeps and where it stands, so the plate of a
    /// whole above it does not lie over it.
    ///
    /// The menu first moves down, into the room below it, because that costs
    /// no entry; what still does not fit is taken from the plates, down to one.
    /// The room is made for the whole of the selection, so the place and the
    /// length of the menu follow it while the selection walks.
    ///
    /// Nothing is taken before the menu was measured once, and nothing beside
    /// the menu: the plate there takes width and no height.
    pub(super) fn stack(
        &self,
        context: &Context,
        rows: &[Row],
        whole: Option<&str>,
        width: f32,
        beside: bool,
        plates: usize,
    ) -> Stack {
        let unchanged = Stack { plates, lower: 0.0 };
        let Some(chrome) = self.chrome.filter(|_| !beside) else {
            return unchanged;
        };
        let Some(above) = whole.map(|text| whole_height(context, text, width)) else {
            return unchanged;
        };

        let step = plate_step(context);
        let footer = chrome.footer.unwrap_or(step);
        let height = |plates: usize| {
            let below = if plates < rows.len() { footer } else { 0.0 };
            chrome.around + plates as f32 * step + below
        };
        stacked(
            context.content_rect().height(),
            above + WHOLE_GAP,
            height,
            plates,
        )
    }
}

/// Whether the menu and the plate of a whole stand side by side in a window of
/// that width.
///
/// The plate is as wide as the menu, so the pair asks for twice that and the
/// gap between them. A window that cannot give it has the plate above the menu
/// instead: the one thing a plate standing beside a menu it overlaps does is
/// hide the entry that it is the whole of.
pub(super) fn beside_fits(plate: f32, window: f32) -> bool {
    plate * 2.0 + WHOLE_GAP <= window
}

/// The plates a menu keeps under something `above` tall in a window that tall,
/// and how far below the middle it stands.
///
/// A menu `height(plates)` tall in the middle of the window leaves half of what
/// it does not take above it. It moves down by what that half lacks, and gives
/// up plates while the two together are taller than the window.
fn stacked(window: f32, above: f32, height: impl Fn(usize) -> f32, plates: usize) -> Stack {
    let plates = (1..=plates)
        .rev()
        .find(|&plates| above + height(plates) <= window)
        .unwrap_or(1);
    let lower = (above - (window - height(plates)) / 2.0).max(0.0);
    Stack { plates, lower }
}

/// How tall the plate of `text` is when its text is set to `width`.
fn whole_height(context: &Context, text: &str, width: f32) -> f32 {
    let style = context.style_of(context.theme());
    let font = egui::TextStyle::Body.resolve(&style);
    let galley =
        context.fonts_mut(|fonts| fonts.layout(text.to_owned(), font, Color32::WHITE, width));
    galley.size().y + Frame::popup(&style).total_margin().sum().y
}

/// How far one plate moves the next one down: its height and the space after
/// it.
fn plate_step(context: &Context) -> f32 {
    let style = context.style_of(context.theme());
    let font = egui::TextStyle::Body.resolve(&style);
    context.fonts_mut(|fonts| fonts.row_height(&font))
        + PADDING.y * 2.0
        + style.spacing.item_spacing.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::MenuItem;
    use crate::view::MIN_WIDTH;
    use egui::Id;

    /// A window wide enough stands the plate of a whole beside the menu; one
    /// that is not stands it above the menu, where it takes no width at all.
    #[test]
    fn a_whole_stands_above_the_menu_in_a_window_too_narrow_for_it_beside() {
        assert!(beside_fits(MIN_WIDTH, MIN_WIDTH * 2.0 + WHOLE_GAP));
        assert!(!beside_fits(MIN_WIDTH, MIN_WIDTH * 2.0 + WHOLE_GAP - 1.0));

        let whole_of = |width: f32| {
            let context = Context::default();
            let mut menu = PlateMenu::new();
            menu.open(vec![
                MenuItem::new("id0", "an entry").full("the whole of what the label was cut from"),
            ])
            .expect("the menu opens");

            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(width, 600.0),
                    )),
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    menu.show(ui.ctx());
                });
                output.textures_delta.clear();
            }

            let plate = egui::AreaState::load(&context, Id::new("plate_menu").with("whole"))
                .map(|state| state.rect())
                .expect("the plate of the whole is drawn");
            (menu.rect, plate)
        };

        let (menu, plate) = whole_of(1600.0);
        assert!(
            plate.left() >= menu.right(),
            "wide enough: the plate stands beside the menu, {plate:?} beside {menu:?}"
        );

        let (menu, plate) = whole_of(MIN_WIDTH * 2.0);
        assert!(
            plate.bottom() <= menu.top(),
            "too narrow: the plate stands above the menu, {plate:?} above {menu:?}"
        );
        assert!(
            plate.right() <= menu.right() + 1.0 && plate.left() >= menu.left() - 1.0,
            "and over it, no wider than the menu: {plate:?} over {menu:?}"
        );
    }

    /// A menu of `entries` plates, each with a whole, after a few frames in a
    /// window of that size: the rect of the menu, the rect of the plate of the
    /// whole and the rect of the window.
    fn stacked_in(size: Vec2, entries: usize) -> (Rect, Rect, Rect) {
        let context = Context::default();
        let mut menu = PlateMenu::new();
        let whole = "the whole of what the label was cut from, ".repeat(6);
        menu.open(
            (0..entries)
                .map(|index| {
                    MenuItem::new(format!("id{index}"), format!("entry {index}")).full(&whole)
                })
                .collect(),
        )
        .expect("the menu opens");

        let window = Rect::from_min_size(egui::Pos2::ZERO, size);
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(window),
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                menu.show(ui.ctx());
            });
            output.textures_delta.clear();
        }

        let plate = egui::AreaState::load(&context, Id::new("plate_menu").with("whole"))
            .map(|state| state.rect())
            .expect("the plate of the whole is drawn");
        (menu.rect, plate, window)
    }

    /// A whole standing above a menu that fills the window takes plates from
    /// it, so the two stand one over the other and both inside the window.
    #[test]
    fn a_whole_above_the_menu_shortens_the_list_instead_of_lying_over_it() {
        let (menu, plate, window) = stacked_in(Vec2::new(MIN_WIDTH * 2.0, 500.0), 60);

        assert!(
            plate.bottom() <= menu.top() + 0.5,
            "the plate stands above the menu, {plate:?} above {menu:?}"
        );
        assert!(
            plate.top() >= window.top() - 0.5 && menu.bottom() <= window.bottom() + 0.5,
            "both inside the window, {plate:?} and {menu:?} in {window:?}"
        );
    }

    /// The rect of the menu and of the plate of the whole, with the selection
    /// on the first entry and one step down, over entries whose wholes are of
    /// one line and of many in turn.
    fn walked() -> [(Rect, Rect); 2] {
        let context = Context::default();
        let mut menu = PlateMenu::new();
        let tall = "the whole of what the label was cut from, ".repeat(6);
        menu.open(
            (0..30)
                .map(|index| {
                    let whole = if index % 2 == 0 {
                        "short"
                    } else {
                        tall.as_str()
                    };
                    MenuItem::new(format!("id{index}"), format!("entry {index}")).full(whole)
                })
                .collect(),
        )
        .expect("the menu opens");

        let window = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(MIN_WIDTH * 2.0, 500.0));
        let mut frames = |events: Vec<egui::Event>| {
            for events in [events, Vec::new(), Vec::new(), Vec::new()] {
                let input = egui::RawInput {
                    screen_rect: Some(window),
                    events,
                    ..Default::default()
                };
                let mut output = context.run_ui(input, |ui| {
                    menu.show(ui.ctx());
                });
                output.textures_delta.clear();
            }
            let plate = egui::AreaState::load(&context, Id::new("plate_menu").with("whole"))
                .map(|state| state.rect())
                .expect("the plate of the whole is drawn");
            (menu.rect, plate)
        };

        let first = frames(Vec::new());
        let down = egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        [first, frames(vec![down])]
    }

    /// The menu makes room for the whole of the selection: a taller one moves
    /// the menu down and shortens it, and neither lies over the other.
    #[test]
    fn the_menu_follows_the_whole_of_the_selection() {
        let [(short, short_plate), (tall, tall_plate)] = walked();

        assert!(
            tall_plate.height() > short_plate.height(),
            "the plate follows its text: {short_plate:?}, then {tall_plate:?}"
        );
        assert!(
            tall.height() < short.height() && tall.top() > short.top(),
            "the menu moves down and gives up plates: {short:?}, then {tall:?}"
        );
        assert!(short_plate.bottom() <= short.top() + 0.5);
        assert!(tall_plate.bottom() <= tall.top() + 0.5);
    }

    /// A short menu with room above it keeps every plate and its place.
    #[test]
    fn a_menu_with_room_above_it_keeps_its_plates_and_its_place() {
        let (menu, plate, window) = stacked_in(Vec2::new(MIN_WIDTH * 2.0, 900.0), 2);

        assert!(plate.bottom() <= menu.top() + 0.5);
        assert!(
            (menu.center().y - window.center().y).abs() < 1.0,
            "the menu stays in the middle: {menu:?} in {window:?}"
        );
    }

    /// The menu moves down before it gives up a plate, and gives up plates
    /// only while the two do not fit.
    #[test]
    fn the_menu_moves_down_before_it_gives_up_plates() {
        let height = |plates: usize| 20.0 + plates as f32 * 10.0;

        assert_eq!(
            stacked(500.0, 100.0, height, 10),
            Stack {
                plates: 10,
                lower: 0.0
            },
            "a menu of 120 leaves 190 above it, more than the 100 asked for"
        );
        assert_eq!(
            stacked(300.0, 100.0, height, 10),
            Stack {
                plates: 10,
                lower: 10.0
            },
            "a menu of 120 leaves 90 above it, and moves down by the 10 it lacks"
        );
        assert_eq!(
            stacked(200.0, 100.0, height, 10),
            Stack {
                plates: 8,
                lower: 50.0
            },
            "a menu of 100 is all that fits under 100 in 200, and it stands at the bottom"
        );
        assert_eq!(
            stacked(50.0, 100.0, height, 10).plates,
            1,
            "one plate is kept whatever the window"
        );
    }
}
