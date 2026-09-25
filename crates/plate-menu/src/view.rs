//! Drawing the menu and reading the keyboard and the pointer.

use crate::error::{MenuError, Result};
use crate::item::MenuItem;
use crate::state::{MenuState, Row};
use egui::{Align2, Area, Color32, Context, FontId, Frame, Id, Order, Rect, Sense, Vec2};

/// Share of the window height the menu leaves free when nothing else is said.
const HEIGHT_SHARE: f32 = 0.8;
/// Width of a plate before the longest label widens it.
const MIN_WIDTH: f32 = 240.0;
/// Widest a plate grows on its own.
const MAX_WIDTH: f32 = 720.0;
/// How far the wheel has to be turned to step to the next plate.
const SCROLL_STEP: f32 = 24.0;
/// Space around the label inside a plate.
const PADDING: Vec2 = Vec2::new(10.0, 6.0);
/// What stands between the menu and the plate beside it.
const WHOLE_GAP: f32 = 8.0;
/// Mark of an entry that leads into entries of its own, the right guillemet.
pub(crate) const INTO: &str = "\u{203a}";
/// Mark of the entry that leads back, the left guillemet.
pub(crate) const BACK: &str = "\u{2039}";
/// What stands between the level of an entry and the entry, the middle dot.
const PARENTS: &str = "\u{b7}";
/// What stands between the first and the last plate of the window, the dash.
const RANGE: &str = "\u{2013}";
/// Every glyph the menu draws by itself, for the test that the fonts carry them.
#[cfg(test)]
const GLYPHS: &[&str] = &[INTO, BACK, PARENTS, RANGE];

/// What a click beside the menu does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Beside {
    /// Closes it, which is what a menu somebody opened wants: the pointer that
    /// opened it takes it away again.
    #[default]
    Closes,
    /// Nothing. The menu is left by an entry or by `Esc`, and a click anywhere
    /// else is no answer — which is what a menu that is the answer to something
    /// that happened wants, because the question outlives a stray click.
    Ignored,
}

/// When the menu makes room for the plate beside it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shift {
    /// Only while the plate is there, so the room is given back as soon as the
    /// selection stands on an entry that carries no whole.
    #[default]
    Auto,
    /// For as long as the level has an entry that carries a whole, so the menu
    /// stands still while the selection walks through it.
    Always,
}

/// What one frame of the menu answers: the entry that was chosen and what was
/// held down while it was chosen.
///
/// The keys held are reported and never read: `Enter` and `Shift`+`Enter` both
/// choose the entry the selection stands on, and a click and `Shift`+a click
/// both choose the entry under the pointer. What a second way of choosing the
/// same entry means is the caller's, because it is the caller that knows what
/// the entry is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Identifier of the entry, as the caller gave it.
    pub id: String,
    /// The modifiers that were down on the frame it was chosen on.
    pub held: egui::Modifiers,
}

/// A menu of wide plates.
///
/// The widget keeps its own state: the caller opens it, draws it every frame and
/// is told which entry was chosen. The menu stands in the middle of the window,
/// wherever it was opened from.
///
/// ```no_run
/// # use plate_menu::{MenuItem, PlateMenu};
/// # fn f(context: &egui::Context) {
/// let mut menu = PlateMenu::new();
/// menu.open(vec![MenuItem::new("copy", "Copy")])
///     .expect("the menu has an entry");
/// if let Some(chosen) = menu.show(context) {
///     println!("chosen: {}", chosen.id);
/// }
/// # }
/// ```
#[derive(Debug)]
pub struct PlateMenu {
    id: Id,
    state: Option<MenuState>,
    notice: Option<String>,
    beside: Beside,
    max_plates: Option<usize>,
    shift: Shift,
    rect: Rect,
    frames: u32,
    first: usize,
    scrolled: f32,
    layout: Option<Layout>,
    generation: Option<String>,
}

/// Size of the window, measured once per level and kept until the level, the
/// query or the window itself changes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Layout {
    /// Width of a plate.
    width: f32,
    /// How many plates are shown at once.
    plates: usize,
}

impl Default for PlateMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl PlateMenu {
    /// A closed menu.
    pub fn new() -> Self {
        Self {
            id: Id::new("plate_menu"),
            state: None,
            notice: None,
            beside: Beside::default(),
            max_plates: None,
            shift: Shift::default(),
            rect: Rect::NOTHING,
            frames: 0,
            first: 0,
            scrolled: 0.0,
            layout: None,
            generation: None,
        }
    }

    /// Identifier of the area, for a second menu in the same window.
    pub fn with_id(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Id::new(id);
        self
    }

    /// How many plates are shown before the rest has to be scrolled to.
    ///
    /// Without it the menu shows what fits into the height of the window less a
    /// fifth of it.
    pub fn max_plates(mut self, plates: usize) -> Self {
        self.max_plates = Some(plates.max(1));
        self
    }

    /// When the menu makes room for the plate of a whole beside it.
    ///
    /// It is kept until it is set again, the way the number of plates is: a
    /// menu whose entries carry no whole is not moved by either of them.
    pub fn shift(&mut self, shift: Shift) {
        self.shift = shift;
    }

    /// The line above the plates, which is what the menu is about.
    ///
    /// It is set after the menu was opened, because an opening clears it: a
    /// sentence that belongs to one question must not stand over the next menu.
    /// It stands above the query and above every level, so it is still there
    /// when the entries of an entry are stepped into.
    pub fn notice(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.notice = (!text.trim().is_empty()).then_some(text);
        self.layout = None;
    }

    /// What a click beside the menu does.
    ///
    /// It is set after the menu was opened, because an opening puts it back to
    /// [`Beside::Closes`]: a menu that holds on to a click is the exception, and
    /// the next menu must not inherit it.
    pub fn beside(&mut self, beside: Beside) {
        self.beside = beside;
    }

    /// Opens the menu with these entries.
    ///
    /// Fails when nothing in them can be chosen, because a menu of lines and
    /// disabled entries would only have to be closed again.
    pub fn open(&mut self, items: Vec<MenuItem>) -> Result<()> {
        if !items.iter().any(MenuItem::is_reachable) {
            return Err(MenuError::Empty);
        }

        self.notice = None;
        self.beside = Beside::default();
        self.state = Some(MenuState::new(items));
        self.rect = Rect::NOTHING;
        self.frames = 0;
        self.first = 0;
        self.scrolled = 0.0;
        self.layout = None;
        self.generation = None;
        Ok(())
    }

    /// Opens the menu with these entries and the selection on one of them.
    ///
    /// The entry is named by its identifier, which is what a list of values
    /// wants: it opens on the value in use, so the one that is set is the one
    /// under the cursor and the way to the neighbouring values is one key. An
    /// identifier none of the entries carries opens the menu on the first
    /// entry, the way [`Self::open`] does.
    pub fn open_at(&mut self, items: Vec<MenuItem>, id: &str) -> Result<()> {
        self.open(items)?;
        if let Some(state) = self.state.as_mut() {
            state.select_id(id);
        }
        Ok(())
    }

    /// Puts other entries in place of the ones the menu is showing.
    ///
    /// The query, the level and the selection are kept: a list that is read
    /// again while it stands — the ports of this machine, looked up once a
    /// second — must not take them away from whoever is walking it. The
    /// selection stays on the entry it stood on while that entry is still
    /// there.
    ///
    /// Fails the way [`Self::open`] does when nothing in the new entries can be
    /// chosen, and the menu then keeps the entries it had: a list that came
    /// back empty is nothing to show, and closing the menu over it would take
    /// the question away as well. A menu that is not open is
    /// [`MenuError::Closed`]: entries are given to it by opening it.
    pub fn refill(&mut self, items: Vec<MenuItem>) -> Result<()> {
        if !items.iter().any(MenuItem::is_reachable) {
            return Err(MenuError::Empty);
        }
        let Some(state) = self.state.as_mut() else {
            return Err(MenuError::Closed);
        };

        state.refill(items);
        self.layout = None;
        self.generation = None;
        Ok(())
    }

    /// Layer the menu is drawn in.
    ///
    /// It is the same layer whether the menu is open or not, and it is what a
    /// caller needs to say that nothing below it may be reached while it stands:
    /// which of the two that is, is the caller's to decide — the widget itself
    /// takes nothing away from the window it is drawn over.
    pub fn layer_id(&self) -> egui::LayerId {
        egui::LayerId::new(Order::Foreground, self.id)
    }

    /// Closes the menu without choosing anything.
    pub fn close(&mut self) {
        self.state = None;
    }

    /// True while the menu is shown.
    pub fn is_open(&self) -> bool {
        self.state.is_some()
    }

    /// Row the selection stands on, while the menu is open.
    pub fn selected(&self) -> Option<usize> {
        Some(self.state.as_ref()?.selected())
    }

    /// Draws the menu and returns the entry that was chosen this frame.
    ///
    /// The menu closes itself on `Esc`, on a click beside it and when an entry
    /// is chosen.
    ///
    /// What was held down is read before anything is answered and belongs to
    /// the choice: a key that the menu takes out of the input is gone by the
    /// time the caller is told about it, and the modifiers of the frame after
    /// are the modifiers of another moment.
    pub fn show(&mut self, context: &Context) -> Option<Chosen> {
        self.state.as_ref()?;

        let held = context.input(|input| input.modifiers);
        let chosen = self.keys(context).or_else(|| self.draw(context));
        if let Some(id) = chosen {
            self.close();
            return Some(Chosen { id, held });
        }

        self.click_outside(context);
        self.frames = self.frames.saturating_add(1);
        None
    }

    /// Acts on the keyboard and on the wheel, before the plates are drawn.
    ///
    /// Everything it reads is taken out of the input: while the menu is open the
    /// keys it is walked by, the text and the wheel belong to it, not to what is
    /// behind it. A view that reads the input itself — a list walked by the same
    /// arrows — would otherwise walk along with the menu standing over it.
    fn keys(&mut self, context: &Context) -> Option<String> {
        let state = self.state.as_mut()?;
        let mut chosen = None;
        let mut close = false;
        let mut scrolled = self.scrolled;

        context.input_mut(|input| {
            if input.key_pressed(egui::Key::Escape) {
                close = true;
                return;
            }
            if input.key_pressed(egui::Key::ArrowDown) {
                state.move_selection(1);
            }
            if input.key_pressed(egui::Key::ArrowUp) {
                state.move_selection(-1);
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                state.step_in();
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                state.step_out();
            }
            if input.key_pressed(egui::Key::Backspace) {
                state.pop_query();
            }
            if input.key_pressed(egui::Key::Enter) {
                chosen = state.accept();
            }

            let typed: String = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if !typed.is_empty() {
                state.push_query(&typed);
            }

            scrolled += input.smooth_scroll_delta.y;
            let steps = (scrolled / SCROLL_STEP) as i32;
            if steps != 0 {
                scrolled -= steps as f32 * SCROLL_STEP;
                for _ in 0..steps.abs() {
                    state.move_selection(if steps > 0 { -1 } else { 1 });
                }
            }

            input.smooth_scroll_delta = Vec2::ZERO;
            input.events.retain(|event| !taken(event));
        });

        self.scrolled = scrolled;

        if close {
            self.close();
        }
        chosen
    }

    /// Draws the plates and returns what a click chose.
    fn draw(&mut self, context: &Context) -> Option<String> {
        let (rows, selected, trail, query) = {
            let state = self.state.as_ref()?;
            (
                state.rows(),
                state.selected(),
                state.trail(),
                state.query().to_string(),
            )
        };
        let Layout { width, plates } = self.layout(context, &rows, &trail, &query);
        let whole = rows
            .get(selected)
            .map(|row| row.full.clone())
            .filter(|full| !full.is_empty());
        let first = self.window(selected, plates, rows.len());
        let last = (first + plates).min(rows.len());
        let mut clicked = None;

        let area = Area::new(self.id)
            .order(Order::Foreground)
            .constrain(true)
            .movable(false)
            .anchor(
                Align2::CENTER_CENTER,
                Vec2::new(-self.room(&rows, whole.is_some(), width), 0.0),
            );

        let response = area.show(context, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width);
                notice(ui, self.notice.as_deref());
                header(ui, &query);
                for (index, row) in rows.iter().enumerate().take(last).skip(first) {
                    if plate(ui, row, index == selected, width) {
                        clicked = Some(index);
                    }
                }
                footer(ui, first, last, rows.len());
            });
        });

        self.rect = response.response.rect;
        self.whole(context, whole, width);
        let state = self.state.as_mut()?;
        clicked.and_then(|index| state.choose(index))
    }

    /// How far left the menu stands to leave room for the plate beside it.
    ///
    /// The two together keep the middle of the window, so the menu moves by
    /// half of what the plate and the gap take. Whether it moves at all is
    /// [`Shift`]: with a plate under every entry the two are the same thing,
    /// and with a plate under some of them one keeps the menu still while the
    /// other gives the room back as soon as it is not needed.
    fn room(&self, rows: &[Row], shown: bool, width: f32) -> f32 {
        let wanted = match self.shift {
            Shift::Auto => shown,
            Shift::Always => rows.iter().any(|row| !row.full.is_empty()),
        };
        if !wanted {
            return 0.0;
        }
        let plate = if self.rect == Rect::NOTHING {
            width
        } else {
            self.rect.width()
        };
        (plate + WHOLE_GAP) / 2.0
    }

    /// The plate beside the menu: the whole of what the label of the selected
    /// entry was cut from.
    ///
    /// It stands right of the menu, which moved left to make room for it, in an
    /// area of its own hanging from its top right corner: a whole of three
    /// lines and a whole of one leave every entry exactly where it was.
    /// Nothing of it can be chosen — it is there to be read, which is what an
    /// entry too long for its plate asks for.
    fn whole(&self, context: &Context, whole: Option<String>, width: f32) {
        if self.rect == Rect::NOTHING {
            return;
        }
        let Some(full) = whole else {
            return;
        };

        Area::new(self.id.with("whole"))
            .order(Order::Foreground)
            .constrain(true)
            .movable(false)
            .fixed_pos(self.rect.right_top() + Vec2::new(WHOLE_GAP, 0.0))
            .pivot(Align2::LEFT_TOP)
            .show(context, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    ui.add(egui::Label::new(full).wrap());
                });
            });
    }

    /// Size of the window, measured again whenever the level, the query or the
    /// number of entries changes, and kept in between.
    fn layout(&mut self, context: &Context, rows: &[Row], trail: &[String], query: &str) -> Layout {
        let generation = format!("{}\u{1f}{query}\u{1f}{}", trail.join("\u{1f}"), rows.len());
        if self.generation.as_deref() != Some(generation.as_str()) {
            self.generation = Some(generation);
            self.first = 0;
            self.layout = None;
        }

        *self.layout.get_or_insert_with(|| Layout {
            width: plate_width(context, rows),
            plates: plate_count(context, self.max_plates, rows.len()),
        })
    }

    /// First row shown, moved only as far as the selection asks for.
    ///
    /// The plates are not scrolled: the window walks with the selection, one
    /// row at a time, so nothing moves under a pointer that is not moving.
    fn window(&mut self, selected: usize, plates: usize, rows: usize) -> usize {
        let highest = rows.saturating_sub(plates);
        if selected < self.first {
            self.first = selected;
        } else if selected + 1 >= self.first + plates {
            self.first = (selected + 1).saturating_sub(plates);
        }
        self.first = self.first.min(highest);
        self.first
    }

    /// Closes the menu when a click lands beside it.
    ///
    /// A pointer that wanders off changes nothing: the menu stands where it is
    /// and is left by a key or by a click, never by the pointer alone. A menu set
    /// to [`Beside::Ignored`] is not left by that click either.
    fn click_outside(&mut self, context: &Context) {
        if self.beside == Beside::Ignored || self.rect == Rect::NOTHING || self.frames == 0 {
            return;
        }

        let (position, clicked) =
            context.input(|input| (input.pointer.latest_pos(), input.pointer.any_click()));
        let Some(position) = position else {
            return;
        };
        if clicked && !self.rect.contains(position) {
            self.close();
        }
    }
}

/// Keys the menu is walked by, which are the keys it takes out of the input.
const WALKED: &[egui::Key] = &[
    egui::Key::Escape,
    egui::Key::ArrowUp,
    egui::Key::ArrowDown,
    egui::Key::ArrowLeft,
    egui::Key::ArrowRight,
    egui::Key::Backspace,
    egui::Key::Enter,
];

/// True for an event the menu acts on, and so does not leave to anything else.
///
/// Both the press and the release of such a key go: a view that acts on the
/// release of a key whose press the menu took would act on half a keystroke.
fn taken(event: &egui::Event) -> bool {
    match event {
        egui::Event::MouseWheel { .. } | egui::Event::Text(_) => true,
        egui::Event::Key { key, .. } => WALKED.contains(key),
        _ => false,
    }
}

/// How many plates are shown at once: what was asked for, else what fits into
/// the height of the window less a fifth.
fn plate_count(context: &Context, asked: Option<usize>, rows: usize) -> usize {
    if let Some(plates) = asked {
        return plates.min(rows.max(1));
    }
    let height = context.content_rect().height() * HEIGHT_SHARE;
    let fits = (height / plate_height_of(context)).floor().max(1.0) as usize;
    fits.min(rows.max(1))
}

/// The line above everything: what the menu is about, in as many lines as it
/// takes. It is the one text of the menu that is not an entry, so it is drawn
/// strongly and wraps instead of being cut.
fn notice(ui: &mut egui::Ui, notice: Option<&str>) {
    let Some(text) = notice else {
        return;
    };
    ui.add(egui::Label::new(egui::RichText::new(text).strong()).wrap());
    ui.separator();
}

/// The line above the plates: what was typed so far.
fn header(ui: &mut egui::Ui, query: &str) {
    if query.is_empty() {
        return;
    }
    ui.label(egui::RichText::new(query).weak());
    ui.separator();
}

/// The line below the plates, shown only while they do not all fit: which of
/// them is on screen.
fn footer(ui: &mut egui::Ui, first: usize, last: usize, rows: usize) {
    if rows <= last - first {
        return;
    }
    ui.separator();
    ui.label(egui::RichText::new(format!("{}{RANGE}{last} / {rows}", first + 1)).weak());
}

/// One plate. Returns true when it was clicked.
fn plate(ui: &mut egui::Ui, row: &Row, selected: bool, width: f32) -> bool {
    if row.separator {
        ui.separator();
        return false;
    }

    let height = plate_height(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let visuals = ui.visuals();
    let hovered = response.hovered() && row.is_reachable();

    if selected || hovered {
        let fill = if selected {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.hovered.bg_fill
        };
        ui.painter().rect_filled(rect, 4.0, fill);
    }

    let color = if !row.enabled && !row.has_children {
        visuals.weak_text_color()
    } else if selected {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let font = FontId::proportional(ui.text_style_height(&egui::TextStyle::Body) * 0.9);

    let mut label = row.label.clone();
    if !row.parents.is_empty() {
        label = format!("{}  {PARENTS}  {label}", row.parents);
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(PADDING.x, 0.0),
        Align2::LEFT_CENTER,
        label,
        font.clone(),
        color,
    );

    let mut right = rect.right_center() - Vec2::new(PADDING.x, 0.0);
    if row.has_children {
        let drawn = ui
            .painter()
            .text(right, Align2::RIGHT_CENTER, INTO, font.clone(), color);
        right.x -= drawn.width() + PADDING.x;
    }
    if !row.detail.is_empty() {
        ui.painter().text(
            right,
            Align2::RIGHT_CENTER,
            &row.detail,
            font,
            visuals.weak_text_color(),
        );
    }

    let response = if row.hint.is_empty() {
        response
    } else {
        response.on_hover_text(&row.hint)
    };

    response.clicked() && row.is_reachable()
}

fn plate_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + PADDING.y * 2.0
}

fn plate_height_of(context: &Context) -> f32 {
    body_size(context) + PADDING.y * 2.0
}

/// Size of the body text of the current theme.
fn body_size(context: &Context) -> f32 {
    context.style_of(context.theme()).text_styles[&egui::TextStyle::Body].size
}

/// Width of a plate: the longest entry, between a floor and a ceiling.
fn plate_width(context: &Context, rows: &[Row]) -> f32 {
    let font = FontId::proportional(body_size(context));
    let longest = rows
        .iter()
        .map(|row| {
            let mut text = row.label.clone();
            if !row.detail.is_empty() {
                text = format!("{text}    {}", row.detail);
            }
            if row.has_children {
                text = format!("{text}  {INTO}");
            }
            context.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(text, font.clone(), Color32::WHITE)
                    .size()
                    .x
            })
        })
        .fold(0.0_f32, f32::max);

    (longest + PADDING.x * 4.0).clamp(MIN_WIDTH, MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context with a window of this height, after one frame, so the fonts
    /// and the style are there.
    fn context(height: f32) -> Context {
        let context = Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, height),
            )),
            ..Default::default()
        };
        let mut output = context.run_ui(input, |_| {});
        output.textures_delta.clear();
        context
    }

    fn items(count: usize) -> Vec<MenuItem> {
        (0..count)
            .map(|index| MenuItem::new(format!("id{index}"), format!("entry {index}")))
            .collect()
    }

    #[test]
    fn every_glyph_the_menu_draws_is_in_the_fonts_of_the_toolkit() {
        let context = context(600.0);
        let missing: Vec<&str> = GLYPHS
            .iter()
            .copied()
            .filter(|glyph| {
                !context.fonts_mut(|fonts| fonts.has_glyphs(&FontId::proportional(14.0), glyph))
            })
            .collect();

        assert!(
            missing.is_empty(),
            "no font of the toolkit carries these: {missing:?}"
        );
    }

    /// One frame of the menu with `Enter` pressed, and the modifiers held while
    /// it was — as the platform reports them, which is a change of the
    /// modifiers and then the key.
    fn enter(context: &Context, menu: &mut PlateMenu, held: egui::Modifiers) -> Option<Chosen> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events: vec![
                egui::Event::ModifiersChanged(held),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: held,
                },
            ],
            ..Default::default()
        };

        let mut chosen = None;
        let mut output = context.run_ui(input, |ui| chosen = menu.show(ui.ctx()));
        output.textures_delta.clear();
        chosen
    }

    /// The entry that was chosen comes back with the keys that were held while
    /// it was, because both ways of choosing it choose the same entry and what
    /// the second of them means is the caller's to say.
    #[test]
    fn what_was_held_while_an_entry_was_chosen_comes_back_with_it() {
        let context = context(600.0);

        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");
        let plain = enter(&context, &mut menu, egui::Modifiers::NONE).expect("an entry is chosen");
        assert_eq!(plain.id, "id0");
        assert!(!plain.held.shift);

        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");
        let shifted =
            enter(&context, &mut menu, egui::Modifiers::SHIFT).expect("an entry is chosen");
        assert_eq!(shifted.id, "id0", "the same entry, chosen the other way");
        assert!(shifted.held.shift);
    }

    #[test]
    fn a_menu_without_a_reachable_entry_is_refused() {
        let mut menu = PlateMenu::new();
        let refused = menu.open(vec![
            MenuItem::separator(),
            MenuItem::new("x", "X").enabled(false),
        ]);

        assert!(matches!(refused, Err(MenuError::Empty)));
        assert!(!menu.is_open());
    }

    #[test]
    fn the_default_plate_count_fits_the_window_less_a_fifth() {
        let context = context(600.0);
        let menu = PlateMenu::new();
        let height = plate_height_of(&context);
        let expected = (600.0 * HEIGHT_SHARE / height).floor() as usize;

        assert!(expected > 1, "a window of 600 points holds several plates");
        assert_eq!(plate_count(&context, menu.max_plates, 100), expected);
        assert_eq!(
            plate_count(&context, menu.max_plates, 3),
            3,
            "a short menu is not padded"
        );
    }

    #[test]
    fn a_given_plate_count_is_kept() {
        let context = context(2000.0);
        let menu = PlateMenu::new().max_plates(4);

        assert_eq!(plate_count(&context, menu.max_plates, 100), 4);
        assert_eq!(plate_count(&context, menu.max_plates, 2), 2);
    }

    #[test]
    fn the_window_walks_with_the_selection_one_row_at_a_time() {
        let mut menu = PlateMenu::new();

        assert_eq!(menu.window(0, 3, 10), 0);
        assert_eq!(
            menu.window(2, 3, 10),
            0,
            "what is visible stays where it is"
        );
        assert_eq!(menu.window(3, 3, 10), 1, "one row down, one row along");
        assert_eq!(menu.window(9, 3, 10), 7, "the end shows the last plates");
        assert_eq!(menu.window(1, 3, 10), 1, "and back up the same way");
        assert_eq!(menu.window(0, 3, 10), 0);
        assert_eq!(menu.window(0, 20, 10), 0, "a menu that fits never moves");
    }

    #[test]
    fn the_wheel_walks_the_plates_and_is_taken_out_of_the_input() {
        let context = context(600.0);
        let mut menu = PlateMenu::new();
        menu.open(items(5)).expect("the menu opens");

        let scrolled = wheel(&context, &mut menu, -SCROLL_STEP * 3.0);
        assert!(
            menu.selected().is_some_and(|selected| selected > 0),
            "turning the wheel down walks towards the end"
        );
        assert_eq!(
            scrolled,
            Vec2::ZERO,
            "and what it read is gone from the input"
        );

        let down = menu.selected().expect("the menu is open");
        wheel(&context, &mut menu, SCROLL_STEP * 3.0);
        assert!(
            menu.selected().is_some_and(|selected| selected < down),
            "turning it up walks back"
        );
    }

    /// Turns the wheel for as many frames as the toolkit needs to hand the
    /// whole delta over, and reports what was left in the input.
    fn wheel(context: &Context, menu: &mut PlateMenu, delta: f32) -> Vec2 {
        let mut left = Vec2::ZERO;
        for frame in 0..8 {
            let events = if frame == 0 {
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, delta),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                Vec::new()
            };
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                menu.show(ui.ctx());
                left = ui.input(|input| input.smooth_scroll_delta);
            });
            output.textures_delta.clear();
        }
        left
    }

    #[test]
    fn opening_on_an_entry_puts_the_selection_there() {
        let mut menu = PlateMenu::new();
        menu.open_at(items(5), "id2").expect("the menu opens");

        assert_eq!(menu.selected(), Some(2));
    }

    #[test]
    fn opening_on_an_entry_that_is_not_there_starts_at_the_first() {
        let mut menu = PlateMenu::new();
        menu.open_at(items(5), "nothing_of_the_sort")
            .expect("the menu opens");

        assert_eq!(menu.selected(), Some(0));
    }

    #[test]
    fn the_keys_the_menu_walks_by_are_taken_out_of_the_input() {
        let context = Context::default();
        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");

        let mut left = true;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            menu.show(ui.ctx());
            left = ui.input(|input| input.key_pressed(egui::Key::ArrowDown));
        });
        output.textures_delta.clear();

        assert_eq!(menu.selected(), Some(1), "the menu walked");
        assert!(!left, "and nothing behind it can walk along");
    }

    #[test]
    fn the_layer_is_the_same_open_or_closed() {
        let mut menu = PlateMenu::new().with_id("a menu of its own");
        let closed = menu.layer_id();

        menu.open(items(2)).expect("the menu opens");

        assert_eq!(menu.layer_id(), closed);
        assert_eq!(menu.layer_id().order, egui::Order::Foreground);
    }

    #[test]
    fn a_click_beside_a_menu_that_ignores_it_leaves_it_open() {
        let context = Context::default();
        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");
        menu.beside(Beside::Ignored);

        click(&context, &mut menu, egui::Pos2::new(5.0, 5.0));
        assert!(menu.is_open(), "a click beside it is no answer");

        menu.beside(Beside::Closes);
        click(&context, &mut menu, egui::Pos2::new(5.0, 5.0));
        assert!(!menu.is_open(), "and it closes again when it may");
    }

    /// Draws the menu for as many frames as a click needs and clicks there.
    fn click(context: &Context, menu: &mut PlateMenu, at: egui::Pos2) {
        for frame in 0..3 {
            let events = if frame == 2 {
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                Vec::new()
            };
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = context.run_ui(input, |ui| {
                menu.show(ui.ctx());
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn a_notice_belongs_to_the_menu_it_was_set_on() {
        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");
        menu.notice("what happened");
        assert_eq!(menu.notice.as_deref(), Some("what happened"));

        menu.notice("   ");
        assert_eq!(menu.notice, None, "a notice of blanks is no notice");

        menu.notice("what happened");
        menu.open(items(3)).expect("the menu opens");
        assert_eq!(menu.notice, None, "an opening clears it");
    }

    #[test]
    fn opening_puts_the_selection_on_the_first_entry() {
        let mut menu = PlateMenu::new();
        menu.open(items(3)).expect("the menu opens");

        assert!(menu.is_open());
        menu.close();
        assert!(!menu.is_open());
    }
}
