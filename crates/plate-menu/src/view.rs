//! Drawing the menu and reading the keyboard and the pointer.

mod whole;

use crate::error::{MenuError, Result};
use crate::item::MenuItem;
use crate::state::{MenuState, Row};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Area, Color32, Context, FontId, Frame, Id, Order, Pos2, Rect, Sense, Stroke, Vec2,
};
use whole::{Chrome, beside_fits, plate_step};

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
/// Mark of an entry that leads into entries of its own, the right guillemet.
pub(crate) const INTO: &str = "\u{203a}";
/// Mark of the entry that leads back, the left guillemet.
pub(crate) const BACK: &str = "\u{2039}";
/// What stands between the level of an entry and the entry, the middle dot.
const PARENTS: &str = "\u{b7}";
/// Every glyph the menu draws by itself, for the test that the fonts carry them.
#[cfg(test)]
const GLYPHS: &[&str] = &[INTO, BACK, PARENTS];
/// How far the characters of a selected hit the query did not match step back
/// from the brightest text, so the ones it matched stand out.
const MATCH_REST: f32 = 0.6;
/// Height of the wave above or below the plates, from crest to trough.
const WAVE_HEIGHT: f32 = 2.0;
/// Length of one wave, crest to crest.
const WAVE_LENGTH: f32 = 8.0;
/// Space above and below the count of a strip, and between it and the wave.
const STRIP_GAP: f32 = 3.0;

/// What a click beside the menu does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Beside {
    /// Closes it: a menu somebody opened.
    #[default]
    Closes,
    /// Nothing; only an entry or `Esc` leaves it: a menu that asks a question
    /// a stray click must not take away.
    Ignored,
}

/// The entry chosen on a frame and the modifiers held while it was.
///
/// The modifiers are reported and never read: what `Shift+Enter` means is the
/// caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Identifier of the entry, as the caller gave it.
    pub id: String,
    /// The modifiers that were down on the frame it was chosen on.
    pub held: egui::Modifiers,
}

/// A menu of wide plates in the middle of the window.
///
/// It keeps its own state: the caller opens it, calls [`Self::show`] every
/// frame and is told what was chosen. See the [crate](crate) for the plates,
/// the levels, the search and the keys.
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
    nothing_found: String,
    step_in_label: String,
    beside: Beside,
    max_plates: Option<usize>,
    rect: Rect,
    frames: u32,
    first: usize,
    shown: usize,
    scrolled: f32,
    dragged: f32,
    layout: Option<Layout>,
    generation: Option<String>,
    chrome: Option<Chrome>,
    search_from: Option<SearchFrom>,
}

/// Where the menu stood when a query was begun: it keeps that top edge and that
/// width for as long as the query stands, so the line being typed in does not
/// move while the hits under it come and go.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SearchFrom {
    /// Top edge of the frame.
    top: f32,
    /// Width of a plate.
    width: f32,
}

/// What a click on a plate asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    /// The entry, the way `Enter` chooses it.
    Choose,
    /// The entries under it, the way `Right` steps in.
    StepIn,
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
            nothing_found: String::new(),
            step_in_label: String::new(),
            beside: Beside::default(),
            max_plates: None,
            rect: Rect::NOTHING,
            frames: 0,
            first: 0,
            shown: 0,
            scrolled: 0.0,
            dragged: 0.0,
            layout: None,
            generation: None,
            chrome: None,
            search_from: None,
        }
    }

    /// Identifier of the area, for a second menu in the same window.
    pub fn with_id(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Id::new(id);
        self
    }

    /// How many plates are shown before the rest has to be walked to.
    ///
    /// Without it the menu shows what fits into the height of the window less a
    /// fifth of it, and less the two strips that say how many rows are hidden.
    pub fn max_plates(mut self, plates: usize) -> Self {
        self.max_plates = Some(plates.max(1));
        self
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

    /// The line shown in place of the hits when a query finds nothing.
    ///
    /// It is the caller's text, because the widget has none of its own, and it
    /// is kept from one opening to the next: it belongs to the application and
    /// not to one menu. Without it the line is an empty plate.
    pub fn nothing_found(&mut self, text: impl Into<String>) {
        self.nothing_found = text.into();
    }

    /// What a screen reader calls the zone at the right end of an entry that
    /// is chosen and has entries of its own: the zone a click steps in by.
    ///
    /// Kept from one opening to the next, as [`Self::nothing_found`] is.
    /// Without it the zone is named by the mark drawn in it.
    pub fn step_in_label(&mut self, text: impl Into<String>) {
        self.step_in_label = text.into();
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
        self.shown = 0;
        self.scrolled = 0.0;
        self.dragged = 0.0;
        self.layout = None;
        self.generation = None;
        self.chrome = None;
        self.search_from = None;
        Ok(())
    }

    /// Opens the menu with these entries and the selection on one of them.
    ///
    /// The entry is named by its identifier, which is what a list of values
    /// wants: it opens on the value in use, so the one that is set is the one
    /// under the cursor and the way to the neighbouring values is one key. An
    /// identifier none of the entries carries opens the menu on the first
    /// entry, the way [`Self::open`] does.
    ///
    /// An entry deeper than the first level opens the level that carries it,
    /// with the level above one step back: a tree of lists is opened on one
    /// value of one of them by naming that value alone.
    pub fn open_at(&mut self, items: Vec<MenuItem>, id: &str) -> Result<()> {
        self.open(items)?;
        if let Some(state) = self.state.as_mut() {
            state.select_deep(id);
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
        self.search_from = None;
    }

    /// True while the menu is shown.
    pub fn is_open(&self) -> bool {
        self.state.is_some()
    }

    /// Whether an entry of this id stands anywhere in the menu, at any level:
    /// a caller that refills a menu when its data comes asks whether the menu
    /// standing is still the one it opened.
    pub fn holds(&self, id: &str) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| crate::search::path_of(state.items(), id).is_some())
    }

    /// Row the selection stands on, while the menu is open.
    pub fn selected(&self) -> Option<usize> {
        Some(self.state.as_ref()?.selected())
    }

    /// Draws the menu and returns the entry that was chosen this frame.
    ///
    /// The menu closes itself on `Esc` with no query typed, on a click beside
    /// it and when an entry is chosen.
    ///
    /// What was held down is read before anything is answered and belongs to
    /// the choice: a key that the menu takes out of the input is gone by the
    /// time the caller is told about it, and the modifiers of the frame after
    /// are the modifiers of another moment.
    pub fn show(&mut self, context: &Context) -> Option<Chosen> {
        self.state.as_ref()?;

        let held = context.input(|input| input.modifiers);
        let chosen = self.keys(context).or_else(|| {
            self.drag(context);
            self.draw(context)
        });
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
    /// keys it is walked by, the text, a paste and the wheel belong to it, not to
    /// what is behind it. A view that reads the input itself — a list walked by
    /// the same arrows — would otherwise walk along with the menu standing over
    /// it.
    fn keys(&mut self, context: &Context) -> Option<String> {
        let state = self.state.as_mut()?;
        let page = self.shown.max(1) as isize;
        let mut chosen = None;
        let mut close = false;
        let mut scrolled = self.scrolled;

        context.input_mut(|input| {
            let pressed = |key| input.key_pressed(key);
            if pressed(egui::Key::Escape) && !state.clear_query() {
                close = true;
            }
            if pressed(egui::Key::ArrowDown) {
                state.move_selection(1);
            }
            if pressed(egui::Key::ArrowUp) {
                state.move_selection(-1);
            }
            if pressed(egui::Key::ArrowRight) {
                state.step_in();
            }
            if pressed(egui::Key::ArrowLeft) {
                state.step_out();
            }
            if pressed(egui::Key::PageDown) {
                state.move_page(page);
            }
            if pressed(egui::Key::PageUp) {
                state.move_page(-page);
            }
            match (pressed(egui::Key::Home), state.query().is_empty()) {
                (true, true) => state.select_first(),
                (true, false) => state.caret_home(),
                _ => {}
            }
            match (pressed(egui::Key::End), state.query().is_empty()) {
                (true, true) => state.select_last(),
                (true, false) => state.caret_end(),
                _ => {}
            }
            if pressed(egui::Key::Backspace) {
                let whole = input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.command
                    )
                });
                match whole {
                    true => state.clear_query(),
                    false => state.pop_query(),
                };
            }
            if pressed(egui::Key::Enter) {
                chosen = state.accept();
            }

            let typed: String = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.clone()),
                    egui::Event::Paste(text) => Some(one_line(text)),
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

    /// Moves the window of plates with a drag that began on the menu, a row
    /// for every plate the pointer travelled.
    ///
    /// The rows follow the pointer: dragging up shows the rows below. The way
    /// travelled is counted from where the press was, so what went by before the
    /// toolkit called it a drag is not lost, and what is short of a row is kept
    /// for the next frame. A selection the window leaves lands on the row shown
    /// nearest to it.
    fn drag(&mut self, context: &Context) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let (origin, dragging, down, travelled) = context.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.press_origin(),
                pointer.is_decidedly_dragging(),
                pointer.primary_down(),
                pointer.total_drag_delta().map_or(0.0, |delta| delta.y),
            )
        });
        let ours = dragging && down && origin.is_some_and(|origin| self.rect.contains(origin));
        if !ours {
            self.dragged = 0.0;
            return;
        }

        let step = plate_step(context);
        let steps = ((travelled - self.dragged) / step) as isize;
        if steps == 0 {
            return;
        }
        self.dragged += steps as f32 * step;

        let rows = state.rows().len();
        let shown = self.shown.max(1);
        let highest = rows.saturating_sub(shown);
        self.first = self.first.saturating_add_signed(-steps).min(highest);
        state.select_shown(self.first, (self.first + shown).min(rows));
    }

    /// Draws the plates and returns what a click chose.
    fn draw(&mut self, context: &Context) -> Option<String> {
        let (rows, selected, trail, query, caret, nothing_found) = {
            let state = self.state.as_ref()?;
            (
                state.rows(),
                state.selected(),
                state.trail(),
                state.query().to_string(),
                state.caret(),
                state.nothing_found(),
            )
        };
        self.follow_query(&query);
        let Layout { width, plates } = self.layout(context, &rows, &trail, &query);
        let width = self.search_from.map_or(width, |from| width.max(from.width));
        let whole = rows
            .get(selected)
            .map(|row| row.full.clone())
            .filter(|full| !full.is_empty());
        let beside = beside_fits(self.beside_width(width), context.content_rect().width());
        let top = self.search_from.map(|from| from.top);
        let stack = self.stack(context, &rows, whole.as_deref(), width, beside, plates, top);
        let first = self.window(selected, stack.plates, rows.len());
        let last = (first + stack.plates).min(rows.len());
        let windowed = last - first < rows.len();
        let (above, below) = hidden(&rows, first, last);
        let mut clicked = None;

        let room = self.room(whole.is_some(), width, beside);
        let window = context.content_rect();
        let area = Area::new(self.id)
            .order(Order::Foreground)
            .constrain(true)
            .movable(false);
        let area = match stack.top {
            Some(top) => area.anchor(Align2::CENTER_TOP, Vec2::new(-room, top - window.top())),
            None => area.anchor(Align2::CENTER_CENTER, Vec2::new(-room, stack.lower)),
        };

        let response = area.show(context, |ui| {
            Frame::popup(ui.style())
                .show(ui, |ui| {
                    ui.set_width(width);
                    notice(ui, self.notice.as_deref());
                    header(ui, &query, caret);
                    let start = ui.cursor().top();
                    if windowed {
                        strip(ui, above, width);
                    }
                    let top = ui.cursor().top();
                    for (index, row) in rows.iter().enumerate().take(last).skip(first) {
                        if let Some(click) =
                            plate(ui, row, index == selected, width, &self.step_in_label)
                        {
                            clicked = Some((index, click));
                        }
                    }
                    if nothing_found {
                        nothing(ui, &self.nothing_found, width);
                    }
                    let bottom = ui.cursor().top();
                    if windowed {
                        strip(ui, below, width);
                    }
                    (bottom - top, (top - start) + (ui.cursor().top() - bottom))
                })
                .inner
        });

        let (plates, strips) = response.inner;
        self.rect = response.response.rect;
        self.shown = last - first;
        self.chrome = Some(Chrome::of(self.rect.height(), plates, strips, self.chrome));
        self.whole(context, whole, width, beside);
        let state = self.state.as_mut()?;
        match clicked? {
            (index, Click::Choose) => state.choose(index),
            (index, Click::StepIn) => {
                state.step_into(index);
                None
            }
        }
    }

    /// Remembers where the menu stood when a query was begun, and forgets it
    /// when the query is gone.
    ///
    /// It is read off the menu drawn last frame, which is the one the query was
    /// typed over: the layout of this frame is already the layout of the hits.
    fn follow_query(&mut self, query: &str) {
        if query.is_empty() {
            self.search_from = None;
            return;
        }
        if self.search_from.is_some() || self.rect == Rect::NOTHING {
            return;
        }
        self.search_from = Some(SearchFrom {
            top: self.rect.top(),
            width: self.layout.map_or(0.0, |layout| layout.width),
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
    /// row at a time, so nothing moves under a pointer that is not moving. A
    /// window a drag moved stays where it was moved to, because the drag put the
    /// selection inside it.
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
///
/// `Tab` does nothing in the menu and is taken all the same: the toolkit would
/// move the keyboard to a widget behind it.
const WALKED: &[egui::Key] = &[
    egui::Key::Escape,
    egui::Key::ArrowUp,
    egui::Key::ArrowDown,
    egui::Key::ArrowLeft,
    egui::Key::ArrowRight,
    egui::Key::Home,
    egui::Key::End,
    egui::Key::PageUp,
    egui::Key::PageDown,
    egui::Key::Backspace,
    egui::Key::Enter,
    egui::Key::Tab,
];

/// True for an event the menu acts on, and so does not leave to anything else.
///
/// Both the press and the release of such a key go: a view that acts on the
/// release of a key whose press the menu took would act on half a keystroke.
fn taken(event: &egui::Event) -> bool {
    match event {
        egui::Event::MouseWheel { .. } | egui::Event::Text(_) | egui::Event::Paste(_) => true,
        egui::Event::Key { key, .. } => WALKED.contains(key),
        _ => false,
    }
}

/// How many plates are shown at once: what was asked for, else what fits into
/// the height of the window less a fifth.
///
/// A menu that does not fit gives the room of both strips from it, the one
/// above the plates and the one below, whether or not each is drawn: the number
/// of plates then stays the same while the window walks from the first row to
/// the last.
fn plate_count(context: &Context, asked: Option<usize>, rows: usize) -> usize {
    if let Some(plates) = asked {
        return plates.min(rows.max(1));
    }
    let height = context.content_rect().height() * HEIGHT_SHARE;
    let fits = |height: f32| (height / plate_height_of(context)).floor().max(1.0) as usize;
    if fits(height) >= rows {
        return rows.max(1);
    }
    fits(height - 2.0 * strip_step(context)).min(rows.max(1))
}

/// The entries the window leaves out above it and below it. A line between
/// entries is not one.
fn hidden(rows: &[Row], first: usize, last: usize) -> (usize, usize) {
    let count = |rows: &[Row]| rows.iter().filter(|row| !row.separator).count();
    let last = last.min(rows.len());
    (count(&rows[..first.min(last)]), count(&rows[last..]))
}

/// The query on one line: a paste of several lines is one query.
fn one_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ")
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

/// The line above the plates: what was typed so far, and the caret in it.
fn header(ui: &mut egui::Ui, query: &str, caret: usize) {
    if query.is_empty() {
        return;
    }
    let rect = ui.label(egui::RichText::new(query).weak()).rect;
    let before: String = query.chars().take(caret).collect();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().weak_text_color();
    let offset = ui.fonts_mut(|fonts| fonts.layout_no_wrap(before, font, color).size().x);
    let x = rect.left() + offset;
    ui.painter().vline(
        x,
        rect.y_range(),
        Stroke::new(1.0, ui.visuals().text_color()),
    );
    ui.separator();
}

/// A strip above or below the plates: a wave with the number of entries
/// hidden on its side in the middle, or nothing when none are.
///
/// It is drawn while the plates do not all fit, on both sides, so the plates
/// stand still while the window walks; a side with nothing hidden keeps the
/// room and draws nothing in it.
fn strip(ui: &mut egui::Ui, hidden: usize, width: f32) {
    let (_, rect) = ui.allocate_space(Vec2::new(width, strip_height(ui.ctx())));
    if hidden == 0 {
        return;
    }

    let color = ui.visuals().weak_text_color();
    let font = egui::TextStyle::Small.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(hidden.to_string(), font, color);
    let count = Rect::from_center_size(rect.center(), galley.size());
    ui.painter().galley(count.min, galley, color);

    let stroke = Stroke::new(1.0, color);
    let left = rect.left() + PADDING.x;
    wave(
        ui.painter(),
        left,
        left,
        count.left() - STRIP_GAP * 2.0,
        rect.center().y,
        stroke,
    );
    wave(
        ui.painter(),
        left,
        count.right() + STRIP_GAP * 2.0,
        rect.right() - PADDING.x,
        rect.center().y,
        stroke,
    );
}

/// A wave from `from` to `to` around the height `y`, in step with one that
/// starts at `origin`, so the two halves of a strip are one wave cut by the
/// count.
fn wave(painter: &egui::Painter, origin: f32, from: f32, to: f32, y: f32, stroke: Stroke) {
    if to <= from {
        return;
    }
    let points = ((to - from).ceil() as usize).max(1);
    let line = (0..=points)
        .map(|step| {
            let x = (from + step as f32).min(to);
            let phase = (x - origin) / WAVE_LENGTH * std::f32::consts::TAU;
            Pos2::new(x, y + phase.sin() * WAVE_HEIGHT / 2.0)
        })
        .collect();
    painter.add(egui::Shape::line(line, stroke));
}

/// Height of a strip: its count and a gap above and below.
fn strip_height(context: &Context) -> f32 {
    let style = context.style_of(context.theme());
    let font = egui::TextStyle::Small.resolve(&style);
    context.fonts_mut(|fonts| fonts.row_height(&font)) + STRIP_GAP * 2.0
}

/// How far a strip moves what is under it down.
fn strip_step(context: &Context) -> f32 {
    strip_height(context) + context.style_of(context.theme()).spacing.item_spacing.y
}

/// The line that stands in place of the hits when a query found nothing.
fn nothing(ui: &mut egui::Ui, text: &str, width: f32) {
    let (id, rect) = ui.allocate_space(Vec2::new(width, plate_height(ui)));
    let response = ui.interact(rect, id, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    ui.painter().text(
        rect.left_center() + Vec2::new(PADDING.x, 0.0),
        Align2::LEFT_CENTER,
        text,
        plate_font(ui),
        ui.visuals().weak_text_color(),
    );
}

/// One plate. Returns what a click on it asked for.
///
/// An entry that is chosen and has entries of its own is two buttons: the
/// label chooses it and the zone at its right end steps in. A pointer has no
/// `Right` to press, and without the zone the entries under it could not be
/// reached by one.
fn plate(
    ui: &mut egui::Ui,
    row: &Row,
    selected: bool,
    width: f32,
    into_label: &str,
) -> Option<Click> {
    if row.separator {
        ui.separator();
        return None;
    }

    let height = plate_height(ui);
    let (_, rect) = ui.allocate_space(Vec2::new(width, height));
    let id = ui.id().with(("plate", &row.path, row.back));
    let zone = row
        .is_split()
        .then(|| Rect::from_min_max(Pos2::new(rect.right() - height, rect.top()), rect.max));
    let label_rect = zone.map_or(rect, |zone| {
        Rect::from_min_max(rect.min, Pos2::new(zone.left(), rect.bottom()))
    });

    let response = ui.interact(label_rect, id, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            row.is_reachable(),
            selected,
            &row.label,
        )
    });
    let zone_response = zone.map(|zone| {
        let response = ui.interact(zone, id.with("into"), Sense::click());
        let name = match into_label.is_empty() {
            true => INTO,
            false => into_label,
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        response
    });

    let visuals = ui.visuals();
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if response.hovered() && row.is_reachable() {
        ui.painter()
            .rect_filled(label_rect, 4.0, visuals.widgets.hovered.bg_fill);
    }
    if let (Some(zone), Some(zone_response)) = (zone, &zone_response)
        && zone_response.hovered()
    {
        ui.painter()
            .rect_filled(zone, 4.0, visuals.widgets.hovered.bg_fill);
    }

    let color = if !row.enabled && !row.has_children {
        visuals.weak_text_color()
    } else if selected {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    // A match is the brightest text of the theme. On the selection, where the
    // whole label is already that, the rest of a hit steps back instead.
    let marked = visuals.strong_text_color();
    let searched = !row.matched_label.is_empty() || !row.matched_parents.is_empty();
    let rest = match selected && searched {
        true => marked.gamma_multiply(MATCH_REST),
        false => color,
    };
    let font = plate_font(ui);

    let galley = ui.fonts_mut(|fonts| fonts.layout_job(label(row, &font, rest, marked)));
    let at = Pos2::new(
        rect.left() + PADDING.x,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(at, galley, color);

    let mut right = rect.right_center() - Vec2::new(PADDING.x, 0.0);
    if let Some(zone) = zone {
        let line = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.6));
        ui.painter()
            .vline(zone.left(), zone.y_range().shrink(PADDING.y / 2.0), line);
        ui.painter().text(
            zone.center(),
            Align2::CENTER_CENTER,
            INTO,
            font.clone(),
            color,
        );
        right.x = zone.left() - PADDING.x;
    } else if row.has_children {
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

    if zone_response.is_some_and(|zone| zone.clicked()) {
        return Some(Click::StepIn);
    }
    (response.clicked() && row.is_reachable()).then_some(Click::Choose)
}

/// The label of a plate, the levels above it in front while searching, with
/// the characters the query matched drawn in `marked`.
fn label(row: &Row, font: &FontId, color: Color32, marked: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = TextFormat::simple(font.clone(), color);
    let found = TextFormat::simple(font.clone(), marked);
    let mut append = |text: &str, matched: &[usize]| {
        for (index, character) in text.chars().enumerate() {
            let format = match matched.contains(&index) {
                true => found.clone(),
                false => plain.clone(),
            };
            job.append(character.encode_utf8(&mut [0; 4]), 0.0, format);
        }
    };
    if !row.parents.is_empty() {
        append(&row.parents, &row.matched_parents);
        append(&format!("  {PARENTS}  "), &[]);
    }
    append(&row.label, &row.matched_label);
    job
}

/// Font of the text on a plate.
fn plate_font(ui: &egui::Ui) -> FontId {
    FontId::proportional(ui.text_style_height(&egui::TextStyle::Body) * 0.9)
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
            if row.has_children && !row.is_split() {
                text = format!("{text}  {INTO}");
            }
            let zone = match row.is_split() {
                true => plate_height_of(context) + PADDING.x,
                false => 0.0,
            };
            let parents = match row.parents.is_empty() {
                true => String::new(),
                false => format!("{}  {PARENTS}  ", row.parents),
            };
            context.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(parents + &text, font.clone(), Color32::WHITE)
                    .size()
                    .x
            }) + zone
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

    /// A menu says whether it holds an entry, at any level, and a closed one
    /// holds none.
    #[test]
    fn a_menu_says_what_it_holds() {
        let mut menu = PlateMenu::new();
        assert!(!menu.holds("id0"));
        let mut entries = items(2);
        entries.push(
            MenuItem::new("parent", "parent").children(
                items(1)
                    .into_iter()
                    .map(|item| MenuItem::new("deep", item.label))
                    .collect(),
            ),
        );
        menu.open(entries).expect("an entry can be chosen");
        assert!(menu.holds("id1") && menu.holds("deep"));
        assert!(!menu.holds("absent"));
        menu.close();
        assert!(!menu.holds("id1"));
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

    /// A menu that does not fit gives the room of both strips from what fits;
    /// one that fits keeps every plate and has no strip.
    #[test]
    fn the_default_plate_count_fits_the_window_less_a_fifth() {
        let context = context(600.0);
        let menu = PlateMenu::new();
        let height = plate_height_of(&context);
        let room = 600.0 * HEIGHT_SHARE;
        let fits = (room / height).floor() as usize;
        let expected = ((room - 2.0 * strip_step(&context)) / height).floor() as usize;

        assert!(expected > 1, "a window of 600 points holds several plates");
        assert_eq!(plate_count(&context, menu.max_plates, 100), expected);
        assert_eq!(
            plate_count(&context, menu.max_plates, fits),
            fits,
            "a menu that fits needs no strip"
        );
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

    /// One frame of a window 800 by 600 with these events.
    fn frame(context: &Context, menu: &mut PlateMenu, events: Vec<egui::Event>) -> Option<Chosen> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut chosen = None;
        let mut output = context.run_ui(input, |ui| chosen = menu.show(ui.ctx()));
        output.textures_delta.clear();
        chosen
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn press(context: &Context, menu: &mut PlateMenu, pressed: egui::Key) {
        frame(context, menu, vec![key(pressed, egui::Modifiers::NONE)]);
    }

    fn query(menu: &PlateMenu) -> String {
        menu.state
            .as_ref()
            .map(|state| state.query().to_string())
            .unwrap_or_default()
    }

    /// The first `Esc` takes the query away, the second the menu.
    #[test]
    fn escape_clears_the_query_before_it_closes_the_menu() {
        let context = context(600.0);
        let mut menu = PlateMenu::new();
        menu.open(items(5)).expect("the menu opens");
        frame(&context, &mut menu, vec![egui::Event::Text("3".into())]);
        assert_eq!(query(&menu), "3");

        press(&context, &mut menu, egui::Key::Escape);
        assert!(menu.is_open(), "the query went, the menu stays");
        assert_eq!(query(&menu), "");

        press(&context, &mut menu, egui::Key::Escape);
        assert!(!menu.is_open());
    }

    /// `Home` and `End` walk the list while nothing is typed and move the caret
    /// while something is; the pages walk the list either way.
    #[test]
    fn home_and_end_walk_the_list_or_the_query() {
        let context = context(600.0);
        let mut menu = PlateMenu::new().max_plates(4);
        menu.open(items(20)).expect("the menu opens");
        frame(&context, &mut menu, Vec::new());

        press(&context, &mut menu, egui::Key::End);
        assert_eq!(menu.selected(), Some(19));
        press(&context, &mut menu, egui::Key::Home);
        assert_eq!(menu.selected(), Some(0));
        press(&context, &mut menu, egui::Key::PageDown);
        assert_eq!(menu.selected(), Some(4), "a page is the plates shown");
        press(&context, &mut menu, egui::Key::PageUp);
        assert_eq!(menu.selected(), Some(0));

        frame(&context, &mut menu, vec![egui::Event::Text("entry".into())]);
        press(&context, &mut menu, egui::Key::Home);
        let caret = menu.state.as_ref().map(MenuState::caret);
        assert_eq!(caret, Some(0), "the caret went to the start");
        press(&context, &mut menu, egui::Key::End);
        let caret = menu.state.as_ref().map(MenuState::caret);
        assert_eq!(caret, Some(5), "and to the end");
    }

    /// A paste goes into the query on one line, `Ctrl+Backspace` clears it,
    /// and neither the paste nor `Tab` is left to what is behind the menu.
    #[test]
    fn a_paste_and_tab_belong_to_the_menu() {
        let context = context(600.0);
        let mut menu = PlateMenu::new();
        menu.open(items(5)).expect("the menu opens");

        let mut left = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 600.0),
            )),
            events: vec![
                egui::Event::Paste("entry\n1".into()),
                key(egui::Key::Tab, egui::Modifiers::NONE),
            ],
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            menu.show(ui.ctx());
            left = ui.input(|input| input.events.clone());
        });
        output.textures_delta.clear();

        assert_eq!(query(&menu), "entry 1");
        assert!(left.is_empty(), "nothing is left behind: {left:?}");

        frame(
            &context,
            &mut menu,
            vec![key(egui::Key::Backspace, egui::Modifiers::COMMAND)],
        );
        assert_eq!(query(&menu), "");
        assert!(menu.is_open());
    }

    /// The rows a window leaves out on each side, lines not counted.
    #[test]
    fn the_strips_count_the_entries_left_out() {
        let mut entries = items(6);
        entries.insert(1, MenuItem::separator());
        entries.insert(6, MenuItem::separator());
        let state = MenuState::new(entries);
        let rows = state.rows();

        assert_eq!(hidden(&rows, 0, 3), (0, 4), "the line at 6 is not counted");
        assert_eq!(hidden(&rows, 3, 5), (2, 2), "nor the one at 1");
        assert_eq!(hidden(&rows, 5, 8), (4, 0));
    }

    /// The window keeps its size while it walks from the first row to the
    /// last, whichever strips are drawn.
    #[test]
    fn the_menu_keeps_its_height_while_the_window_walks() {
        let context = context(600.0);
        let mut menu = PlateMenu::new();
        menu.open(items(60)).expect("the menu opens");
        frame(&context, &mut menu, Vec::new());
        let at_first = (menu.shown, menu.rect.height());

        press(&context, &mut menu, egui::Key::PageDown);
        frame(&context, &mut menu, Vec::new());
        let in_the_middle = (menu.shown, menu.rect.height());

        press(&context, &mut menu, egui::Key::End);
        frame(&context, &mut menu, Vec::new());
        let at_last = (menu.shown, menu.rect.height());

        assert!(at_first.0 < 60);
        assert_eq!(at_first, in_the_middle);
        assert_eq!(at_first, at_last);
    }

    /// While a query is typed the menu keeps the top edge it had, and gets
    /// shorter from the bottom.
    #[test]
    fn a_query_keeps_the_top_of_the_menu() {
        let context = context(600.0);
        let mut menu = PlateMenu::new();
        menu.open(items(12)).expect("the menu opens");
        frame(&context, &mut menu, Vec::new());
        frame(&context, &mut menu, Vec::new());
        let before = menu.rect;

        frame(&context, &mut menu, vec![egui::Event::Text("1".into())]);
        frame(&context, &mut menu, Vec::new());
        let typed = menu.rect;
        frame(&context, &mut menu, vec![egui::Event::Text("1".into())]);
        frame(&context, &mut menu, Vec::new());
        let narrowed = menu.rect;

        assert!(
            typed.height() < before.height(),
            "{typed:?} under {before:?}"
        );
        assert!(narrowed.height() < typed.height());
        assert!(
            (typed.top() - before.top()).abs() < 0.5,
            "{typed:?} under {before:?}"
        );
        assert!((narrowed.top() - before.top()).abs() < 0.5);
        assert!(narrowed.width() >= before.width() - 0.5, "no narrower");

        press(&context, &mut menu, egui::Key::Escape);
        frame(&context, &mut menu, Vec::new());
        assert_eq!(menu.rect, before, "and the menu stands where it stood");
    }

    /// Pointer events that press at `from`, move by `by` in `steps` and
    /// release, a frame each.
    fn drag(
        context: &Context,
        menu: &mut PlateMenu,
        from: Pos2,
        by: Vec2,
        steps: usize,
    ) -> Option<Chosen> {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut chosen = frame(
            context,
            menu,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        for step in 1..=steps {
            let at = from + by * (step as f32 / steps as f32);
            chosen = chosen.or(frame(context, menu, vec![egui::Event::PointerMoved(at)]));
        }
        chosen.or(frame(context, menu, vec![button(from + by, false)]))
    }

    /// A drag moves the window a row for every plate the pointer travelled,
    /// takes the selection along, stops at the end and chooses nothing.
    #[test]
    fn a_drag_moves_the_window_and_chooses_nothing() {
        let context = context(600.0);
        let mut menu = PlateMenu::new().max_plates(5);
        menu.open(items(20)).expect("the menu opens");
        frame(&context, &mut menu, Vec::new());
        frame(&context, &mut menu, Vec::new());
        let step = plate_step(&context);
        let on_the_plates = menu.rect.center();

        let chosen = drag(
            &context,
            &mut menu,
            on_the_plates,
            Vec2::new(0.0, -3.2 * step),
            4,
        );
        assert_eq!(chosen, None, "the release chooses nothing");
        assert!(menu.is_open());
        assert_eq!(menu.first, 3, "up by three plates, three rows on");
        assert_eq!(menu.selected(), Some(3), "the selection came along");

        drag(
            &context,
            &mut menu,
            on_the_plates,
            Vec2::new(0.0, -40.0 * step),
            8,
        );
        assert_eq!(menu.first, 15, "the window stops at the last rows");
        assert_eq!(menu.selected(), Some(15));

        drag(
            &context,
            &mut menu,
            on_the_plates,
            Vec2::new(0.0, 2.0 * step),
            4,
        );
        assert_eq!(menu.first, 13, "down shows the rows above");
        assert_eq!(menu.selected(), Some(15), "a selection still shown stays");
    }

    /// A drag that starts beside the menu is not the menu's: nothing moves and
    /// nothing closes.
    #[test]
    fn a_drag_beside_the_menu_moves_nothing() {
        let context = context(600.0);
        let mut menu = PlateMenu::new().max_plates(5);
        menu.open(items(20)).expect("the menu opens");
        frame(&context, &mut menu, Vec::new());

        drag(
            &context,
            &mut menu,
            Pos2::new(5.0, 300.0),
            Vec2::new(0.0, -200.0),
            4,
        );
        assert!(menu.is_open());
        assert_eq!(menu.first, 0);
        assert_eq!(menu.selected(), Some(0));
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
