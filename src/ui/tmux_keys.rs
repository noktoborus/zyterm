//! The keys of tmux in its mode (`docs/tmux.md`, *The keys*): what each does,
//! the plate of what can be pressed next and the numbers of the panes.

use std::time::{Duration, Instant};

use rust_i18n::t;
use zyt_keymux::KeyStroke;
use zyt_tmux::{Edge, LAYOUTS, PaneId, WindowId};

use crate::app::App;
use crate::tmux::{Choice, Menu, MenuSession, MenuWindow, Places};
use crate::ui::tmux::{Renamed, ask_rename};

/// How long the numbers of the panes stand.
const NUMBERS_TIME: Duration = Duration::from_secs(1);

/// The byte of `ctrl+b`.
const PREFIX: u8 = 2;

/// A pane narrower or lower than this has no room to split: two cells and the
/// one between them.
const MIN_TO_SPLIT: usize = 3;

/// An action of the keys of tmux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmuxKey {
    /// A new window.
    NewWindow,
    /// The window closed, with no question.
    CloseWindow,
    /// The question of a new name of the window.
    RenameWindow,
    /// The window this many on, round past either end.
    StepWindow(i32),
    /// The window shown before.
    LastWindow,
    /// The window of this index.
    Window(u32),
    /// The next layout.
    NextLayout,
    /// One of `LAYOUTS`.
    Layout(usize),
    /// The pane split: side by side, or one above the other.
    Split(bool),
    /// The pane closed, with no question.
    ClosePane,
    /// The pane to a new window.
    BreakPane,
    /// The pane zoomed, or unzoomed.
    Zoom,
    /// The pane changes places with the one before it, or after it.
    Swap(bool),
    /// The focus to the pane on that side.
    Focus(Edge),
    /// The focus to the next pane.
    FocusNext,
    /// The focus to the pane in focus before.
    FocusLast,
    /// The numbers of the panes.
    Numbers,
    /// An edge of the pane moved by cells.
    Nudge(Edge, u16),
    /// The question of a new name of the session.
    RenameSession,
    /// The session this many on, round past either end.
    StepSession(i32),
    /// Leave the mode.
    Detach,
    /// `^B` to the pane.
    Prefix,
}

impl TmuxKey {
    /// Every action, with its identifier in the key map.
    pub const ALL: &'static [(&'static str, Self)] = &[
        ("tmux.window.new", Self::NewWindow),
        ("tmux.window.close", Self::CloseWindow),
        ("tmux.window.rename", Self::RenameWindow),
        ("tmux.window.next", Self::StepWindow(1)),
        ("tmux.window.previous", Self::StepWindow(-1)),
        ("tmux.window.last", Self::LastWindow),
        ("tmux.window.0", Self::Window(0)),
        ("tmux.window.1", Self::Window(1)),
        ("tmux.window.2", Self::Window(2)),
        ("tmux.window.3", Self::Window(3)),
        ("tmux.window.4", Self::Window(4)),
        ("tmux.window.5", Self::Window(5)),
        ("tmux.window.6", Self::Window(6)),
        ("tmux.window.7", Self::Window(7)),
        ("tmux.window.8", Self::Window(8)),
        ("tmux.window.9", Self::Window(9)),
        ("tmux.window.layout_next", Self::NextLayout),
        ("tmux.window.layout_1", Self::Layout(0)),
        ("tmux.window.layout_2", Self::Layout(1)),
        ("tmux.window.layout_3", Self::Layout(2)),
        ("tmux.window.layout_4", Self::Layout(3)),
        ("tmux.window.layout_5", Self::Layout(4)),
        ("tmux.pane.split_right", Self::Split(true)),
        ("tmux.pane.split_below", Self::Split(false)),
        ("tmux.pane.close", Self::ClosePane),
        ("tmux.pane.break", Self::BreakPane),
        ("tmux.pane.zoom", Self::Zoom),
        ("tmux.pane.swap_previous", Self::Swap(true)),
        ("tmux.pane.swap_next", Self::Swap(false)),
        ("tmux.pane.focus_up", Self::Focus(Edge::Up)),
        ("tmux.pane.focus_down", Self::Focus(Edge::Down)),
        ("tmux.pane.focus_left", Self::Focus(Edge::Left)),
        ("tmux.pane.focus_right", Self::Focus(Edge::Right)),
        ("tmux.pane.focus_next", Self::FocusNext),
        ("tmux.pane.focus_last", Self::FocusLast),
        ("tmux.pane.numbers", Self::Numbers),
        ("tmux.pane.resize_up", Self::Nudge(Edge::Up, 1)),
        ("tmux.pane.resize_down", Self::Nudge(Edge::Down, 1)),
        ("tmux.pane.resize_left", Self::Nudge(Edge::Left, 1)),
        ("tmux.pane.resize_right", Self::Nudge(Edge::Right, 1)),
        ("tmux.pane.resize_up_5", Self::Nudge(Edge::Up, 5)),
        ("tmux.pane.resize_down_5", Self::Nudge(Edge::Down, 5)),
        ("tmux.pane.resize_left_5", Self::Nudge(Edge::Left, 5)),
        ("tmux.pane.resize_right_5", Self::Nudge(Edge::Right, 5)),
        ("tmux.session.rename", Self::RenameSession),
        ("tmux.session.previous", Self::StepSession(-1)),
        ("tmux.session.next", Self::StepSession(1)),
        ("tmux.session.detach", Self::Detach),
        ("tmux.prefix", Self::Prefix),
    ];

    /// Its identifier in the key map.
    pub fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, key)| *key == self)
            .map_or("", |(id, _)| id)
    }

    /// The action of an identifier in the key map.
    fn of(id: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(of, _)| *of == id)
            .map(|(_, key)| *key)
    }
}

/// What the keys of tmux keep between frames.
#[derive(Debug, Default)]
pub struct Follow {
    /// The window and the pane shown at the last frame.
    shown: Option<(WindowId, PaneId)>,
    /// The window shown before.
    last_window: Option<WindowId>,
    /// The pane in focus before, in the window shown.
    last_pane: Option<PaneId>,
    /// Until when the numbers of the panes stand.
    numbers: Option<Instant>,
}

impl Follow {
    /// Takes note of what is shown now.
    fn follow(&mut self, now: Option<(WindowId, PaneId)>) {
        let Some((window, pane)) = now else {
            return;
        };
        match self.shown {
            Some((was, _)) if was != window => {
                self.last_window = Some(was);
                self.last_pane = None;
            }
            Some((_, was)) if was != pane => self.last_pane = Some(was),
            _ => {}
        }
        self.shown = now;
    }
}

/// While the numbers stand a key is theirs: a digit moves the focus to that
/// pane, and any key hides them and goes nowhere. True when the key was taken.
pub fn numbers_take(app: &mut App, key: egui::Key, now: Instant) -> bool {
    let Some(until) = app.ui.tmux_keys.numbers.take() else {
        return false;
    };
    if now > until {
        return false;
    }
    let digit = key.name().parse::<u32>().ok();
    if let (Some(digit), Some(menu)) = (digit, app.session.tmux_menu())
        && let Some(window) = shown_window(&menu)
        && let Some(pane) = window.pane_list.iter().find(|p| p.index == Some(digit))
    {
        app.session.tmux_choose(Choice::Focus {
            window: window.id,
            pane: pane.id,
        });
    }
    true
}

fn shown_window(menu: &Menu) -> Option<&MenuWindow> {
    let session = menu.shown.as_ref()?;
    session.windows.iter().find(|w| Some(w.id) == menu.window)
}

/// The entry this many on from `at`, round past either end.
fn stepped<T>(list: &[T], at: usize, steps: i32) -> Option<&T> {
    let at = (at as i64 + i64::from(steps)).rem_euclid(list.len() as i64);
    list.get(at as usize)
}

/// Does what an action says, on the pane, the window or the session shown.
pub fn run(app: &mut App, key: TmuxKey) {
    if key == TmuxKey::Prefix {
        app.send_input(&[PREFIX]);
        return;
    }
    let Some(menu) = app.session.tmux_menu() else {
        return;
    };
    let places = app.session.tmux_places().clone();
    if !possible(key, Some(&menu), &places, &app.ui.tmux_keys) {
        return;
    }
    match key {
        TmuxKey::Numbers => app.ui.tmux_keys.numbers = Some(Instant::now() + NUMBERS_TIME),
        TmuxKey::RenameWindow => {
            if let Some(window) = menu.window {
                ask_rename(app, Renamed::Window(window));
            }
        }
        TmuxKey::RenameSession => {
            if let Some(shown) = &menu.shown {
                ask_rename(app, Renamed::Session(shown.id));
            }
        }
        _ => {
            if let Some(choice) = choice_of(key, &menu, &places, &app.ui.tmux_keys) {
                app.session.tmux_choose(choice);
            }
        }
    }
}

/// Whether an action can do anything over the model as it stands: what the plate
/// shows and what a key runs.
fn possible(key: TmuxKey, menu: Option<&Menu>, places: &Places, follow: &Follow) -> bool {
    let Some(menu) = menu else {
        return key == TmuxKey::Prefix;
    };
    match key {
        TmuxKey::Prefix => true,
        TmuxKey::Numbers => shown_window(menu).is_some_and(|w| !w.pane_list.is_empty()),
        TmuxKey::RenameWindow => menu.window.is_some(),
        TmuxKey::RenameSession => menu.shown.is_some(),
        _ => choice_of(key, menu, places, follow).is_some(),
    }
}

/// What an action chooses over the model as it stands; none when it has
/// nothing to act on.
fn choice_of(key: TmuxKey, menu: &Menu, places: &Places, follow: &Follow) -> Option<Choice> {
    let session = menu.shown.as_ref()?;
    let window = shown_window(menu);
    let pane = window.and_then(|window| window.pane).or(menu.pane);
    let several = window.filter(|window| window.panes > 1);
    let go_to = |window: &MenuWindow| Choice::Window {
        session: session.id,
        window: window.id,
    };
    let focus = |pane: PaneId| {
        Some(Choice::Focus {
            window: window?.id,
            pane,
        })
    };
    Some(match key {
        TmuxKey::NewWindow => Choice::NewWindow,
        TmuxKey::CloseWindow => Choice::CloseWindow(window?.id),
        TmuxKey::StepWindow(steps) => {
            let shown = window?.id;
            let at = session.windows.iter().position(|w| w.id == shown)?;
            go_to(stepped(&session.windows, at, steps).filter(|w| w.id != shown)?)
        }
        TmuxKey::LastWindow => {
            let last = follow.last_window?;
            go_to(session.windows.iter().find(|w| w.id == last)?)
        }
        TmuxKey::Window(index) => go_to(
            session
                .windows
                .iter()
                .find(|w| w.index == Some(index) && Some(w.id) != menu.window)?,
        ),
        TmuxKey::NextLayout => Choice::Layout {
            window: several?.id,
            layout: None,
        },
        TmuxKey::Layout(layout) => Choice::Layout {
            window: several?.id,
            layout: Some(LAYOUTS[layout]),
        },
        TmuxKey::Split(side_by_side) => {
            let pane = pane?;
            let room = places.panes.iter().find(|place| place.pane == pane);
            let room = room.map(|place| match side_by_side {
                true => place.columns,
                false => place.rows,
            });
            if room.is_some_and(|cells| cells < MIN_TO_SPLIT) {
                return None;
            }
            Choice::Split {
                window: window?.id,
                pane,
                side_by_side,
            }
        }
        TmuxKey::ClosePane => Choice::ClosePane(pane?),
        TmuxKey::BreakPane => Choice::BreakPane {
            window: several?.id,
            pane: pane?,
        },
        TmuxKey::Zoom => Choice::Zoom {
            window: several?.id,
            pane: pane?,
        },
        TmuxKey::Swap(up) => {
            several?;
            Choice::Swap { pane: pane?, up }
        }
        TmuxKey::Focus(edge) => focus(neighbour(places, pane?, edge)?)?,
        TmuxKey::FocusNext => {
            let panes = &window?.pane_list;
            let at = panes.iter().position(|p| Some(p.id) == pane)?;
            focus(stepped(panes, at, 1)?.id)?
        }
        TmuxKey::FocusLast => {
            let last = follow.last_pane?;
            window?.pane_list.iter().find(|p| p.id == last)?;
            focus(last)?
        }
        TmuxKey::Nudge(edge, cells) => {
            let pane = pane?;
            // Zoomed, the neighbours are not seen, and tmux unzooms first.
            if several?.zoomed.is_none() && !edge_moves(places, pane, edge) {
                return None;
            }
            Choice::Nudge { pane, edge, cells }
        }
        TmuxKey::StepSession(steps) => stepped_session(session, &menu.others, steps)?,
        TmuxKey::Detach => Choice::Detach,
        TmuxKey::RenameWindow | TmuxKey::RenameSession | TmuxKey::Numbers | TmuxKey::Prefix => {
            return None;
        }
    })
}

/// Whether tmux can move an edge that way: a neighbour on either side along that
/// line, since a pane at the top moves its bottom edge up as well.
fn edge_moves(places: &Places, pane: PaneId, edge: Edge) -> bool {
    let sides = match edge {
        Edge::Up | Edge::Down => [Edge::Up, Edge::Down],
        Edge::Left | Edge::Right => [Edge::Left, Edge::Right],
    };
    sides
        .into_iter()
        .any(|side| neighbour(places, pane, side).is_some())
}

/// The session this many on in the order of their identifiers.
fn stepped_session(shown: &MenuSession, others: &[MenuSession], steps: i32) -> Option<Choice> {
    let mut ids: Vec<_> = others.iter().map(|session| session.id).collect();
    ids.push(shown.id);
    ids.sort();
    let at = ids.iter().position(|id| *id == shown.id)?;
    let id = *stepped(&ids, at, steps)?;
    (id != shown.id).then_some(Choice::Session(id))
}

/// The pane on that side of `pane`: the nearest that shares some of the edge,
/// and of those the one that shares most.
fn neighbour(places: &Places, pane: PaneId, edge: Edge) -> Option<PaneId> {
    let from = places.panes.iter().find(|place| place.pane == pane)?;
    let shared = |a: usize, a_len: usize, b: usize, b_len: usize| {
        (a + a_len).min(b + b_len).saturating_sub(a.max(b))
    };
    places
        .panes
        .iter()
        .filter(|place| place.pane != pane)
        .filter_map(|place| {
            let across = shared(from.x, from.columns, place.x, place.columns);
            let along = shared(from.y, from.rows, place.y, place.rows);
            let (far, shared) = match edge {
                Edge::Up => (from.y.checked_sub(place.y + place.rows)?, across),
                Edge::Down => (place.y.checked_sub(from.y + from.rows)?, across),
                Edge::Left => (from.x.checked_sub(place.x + place.columns)?, along),
                Edge::Right => (place.x.checked_sub(from.x + from.columns)?, along),
            };
            (shared > 0).then_some((far, std::cmp::Reverse(shared), place.pane))
        })
        .min()
        .map(|(_, _, pane)| pane)
}

/// How a key stands on the plate: `C-←`, `M-1`, `%`.
fn key_label(stroke: &KeyStroke) -> String {
    let text = stroke.to_text();
    let mut label = String::new();
    let mut rest = text.as_str();
    for (name, short) in [("ctrl+", "C-"), ("alt+", "M-"), ("shift+", "S-")] {
        if let Some(after) = rest.strip_prefix(name) {
            label.push_str(short);
            rest = after;
        }
    }
    label.push_str(match rest {
        "up" => "\u{2191}",
        "down" => "\u{2193}",
        "left" => "\u{2190}",
        "right" => "\u{2192}",
        other => other,
    });
    label
}

/// The group of keys an identifier stands in, one row of the plate, with the
/// key of the text of that row.
fn group_of(id: &str) -> Option<&'static str> {
    let rest = id.strip_prefix("tmux.")?;
    if rest
        .strip_prefix("window.")
        .is_some_and(|name| name.parse::<u32>().is_ok())
    {
        return Some("tmux.keys.window");
    }
    if rest.starts_with("window.layout_") && rest != "window.layout_next" {
        return Some("tmux.keys.layout");
    }
    if ["up", "down", "left", "right"]
        .iter()
        .any(|side| rest == format!("pane.focus_{side}"))
    {
        return Some("tmux.keys.focus");
    }
    if rest.starts_with("pane.resize_") {
        return Some(match rest.ends_with("_5") {
            true => "tmux.keys.resize_5",
            false => "tmux.keys.resize",
        });
    }
    None
}

/// The rows of a column: the keys and what they do.
fn rows(next: &[(String, String)], column: &str) -> Vec<(String, String)> {
    let mut rows: Vec<(Option<&'static str>, String, String)> = Vec::new();
    for (keys, id) in next.iter().filter(|(_, id)| column_of(id) == column) {
        let group = group_of(id);
        match rows
            .iter_mut()
            .find(|row| group.is_some() && row.0 == group)
        {
            Some(row) => join_keys(&mut row.1, keys),
            None => {
                let title = match group {
                    Some(group) => t!(group).to_string(),
                    None => t!(format!("command.{id}")).to_string(),
                };
                rows.push((group, keys.clone(), title));
            }
        }
    }
    rows.into_iter()
        .map(|(_, keys, title)| (fold_digits(&keys), title))
        .collect()
}

/// Adds a key to a row of a group; the modifiers they share are written once.
fn join_keys(row: &mut String, keys: &str) {
    let shared = row.len() - row.trim_start_matches(['C', 'M', 'S', '-']).len();
    row.push_str(keys.strip_prefix(&row[..shared]).unwrap_or(keys));
}

/// `0123456789` is `0…9`, `M-12345` is `M-1…5`; digits with a gap stay as they are.
fn fold_digits(keys: &str) -> String {
    let digits = keys.trim_start_matches(|c: char| !c.is_ascii_digit());
    let lead = &keys[..keys.len() - digits.len()];
    let in_a_row = digits
        .as_bytes()
        .windows(2)
        .all(|pair| pair[1] == pair[0] + 1);
    match (
        digits.len() > 2 && in_a_row && digits.chars().all(|c| c.is_ascii_digit()),
        digits.chars().next(),
        digits.chars().last(),
    ) {
        (true, Some(first), Some(last)) => format!("{lead}{first}\u{2026}{last}"),
        _ => keys.to_string(),
    }
}

/// The column an identifier stands in.
fn column_of(id: &str) -> &'static str {
    match id {
        id if id.starts_with("tmux.window.") => "tmux.keys.windows",
        id if id.starts_with("tmux.session.") => "tmux.keys.session",
        id if id.starts_with("tmux.pane.") || id == "tmux.prefix" => "tmux.keys.panes",
        _ => "",
    }
}

/// Follows the window and the pane shown, draws the plate of what can be
/// pressed next and the numbers of the panes, and asks the frame that takes
/// a repeat or the numbers down.
pub fn draw(
    app: &mut App,
    context: &egui::Context,
    area: egui::Rect,
    cell: egui::Vec2,
    places: &Places,
) {
    let shown = app.session.tmux_shown();
    app.ui.tmux_keys.follow(shown);
    let now = Instant::now();

    if let Some(until) = app.ui.tmux_keys.numbers {
        match now > until {
            true => app.ui.tmux_keys.numbers = None,
            false => {
                numbers(app, context, area, cell, places);
                context.request_repaint_after(until - now);
            }
        }
    }
    if let Some(until) = app.dispatcher.repeating(now) {
        context.request_repaint_after(until - now);
    }

    let next = app.dispatcher.next(now);
    if next.is_empty() {
        return;
    }
    let menu = app.session.tmux_menu();
    let next: Vec<(String, String)> = next
        .into_iter()
        .filter(|(_, binding)| {
            TmuxKey::of(binding.command.as_str())
                .is_some_and(|key| possible(key, menu.as_ref(), places, &app.ui.tmux_keys))
        })
        .map(|(keys, binding)| {
            let keys = keys.iter().map(key_label).collect();
            (keys, binding.command.as_str().to_string())
        })
        .collect();
    if !next.is_empty() {
        let pane = crate::ui::panes::shown_rect(places, area, cell);
        hints(context, &next, area, pane.unwrap_or(area).intersect(area));
    }
}

/// The plate of hints: in the middle of the pane in focus, or of the terminal
/// when it does not fit the pane.
fn hints(context: &egui::Context, next: &[(String, String)], area: egui::Rect, pane: egui::Rect) {
    let id = egui::Id::unique("tmux keys");
    let fits = context
        .memory(|memory| memory.area_rect(id))
        .is_none_or(|plate| plate.width() <= pane.width() && plate.height() <= pane.height());
    let center = if fits { pane.center() } else { area.center() };
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_CENTER)
        .fixed_pos(center)
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for column in ["tmux.keys.windows", "tmux.keys.panes", "tmux.keys.session"] {
                        let rows = rows(next, column);
                        if rows.is_empty() {
                            continue;
                        }
                        ui.vertical(|ui| {
                            ui.strong(t!(column));
                            egui::Grid::new(column).num_columns(2).show(ui, |ui| {
                                for (keys, title) in rows {
                                    ui.monospace(keys);
                                    ui.label(title);
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
            });
        });
}

/// The number of each pane seen, in its middle. A window zoomed shows one
/// pane, on the whole area; a window of one pane has no numbers to choose from.
fn numbers(
    app: &App,
    context: &egui::Context,
    area: egui::Rect,
    cell: egui::Vec2,
    places: &Places,
) {
    let Some(menu) = app.session.tmux_menu() else {
        return;
    };
    let Some(window) = shown_window(&menu) else {
        return;
    };
    for pane in &window.pane_list {
        let Some(index) = pane.index else {
            continue;
        };
        let rect = match places.panes.iter().find(|place| place.pane == pane.id) {
            Some(place) => crate::ui::panes::rect_of(place, area, cell),
            None if menu.pane == Some(pane.id) => area,
            None => continue,
        };
        egui::Area::new(egui::Id::unique(("tmux number", pane.id)))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_CENTER)
            .fixed_pos(rect.center())
            .interactable(false)
            .show(context, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.heading(index.to_string());
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::Place;

    fn place(pane: u32, x: usize, y: usize, columns: usize, rows: usize) -> Place {
        Place {
            pane: PaneId(pane),
            x,
            y,
            columns,
            rows,
            waits: false,
        }
    }

    /// `%0` on the left at full height, `%1` above `%2` on the right.
    fn three() -> Places {
        Places {
            panes: vec![
                place(0, 0, 0, 40, 24),
                place(1, 41, 0, 39, 12),
                place(2, 41, 13, 39, 11),
            ],
            ..Places::default()
        }
    }

    #[test]
    fn the_pane_on_a_side_is_the_next_one_that_shares_the_edge() {
        let places = three();
        let of = |pane, edge| neighbour(&places, PaneId(pane), edge);
        assert_eq!(of(0, Edge::Right), Some(PaneId(1)));
        assert_eq!(of(1, Edge::Left), Some(PaneId(0)));
        assert_eq!(of(2, Edge::Left), Some(PaneId(0)));
        assert_eq!(of(1, Edge::Down), Some(PaneId(2)));
        assert_eq!(of(2, Edge::Up), Some(PaneId(1)));
        assert_eq!(of(0, Edge::Left), None);
        assert_eq!(of(0, Edge::Up), None);
    }

    fn window(id: u32, panes: u32) -> MenuWindow {
        MenuWindow {
            id: WindowId(id),
            index: Some(id),
            label: String::new(),
            name: String::new(),
            panes: panes as usize,
            pane_list: match panes {
                1 => Vec::new(),
                _ => (0..panes)
                    .map(|pane| crate::tmux::MenuPane {
                        id: PaneId(pane),
                        index: Some(pane),
                        label: String::new(),
                    })
                    .collect(),
            },
            pane: Some(PaneId(0)),
            zoomed: None,
            current: false,
            calling: crate::tmux::Calling::None,
        }
    }

    /// A session of these windows, the first shown.
    fn menu(windows: Vec<MenuWindow>) -> Menu {
        Menu {
            window: windows.first().map(|window| window.id),
            pane: Some(PaneId(0)),
            shown: Some(MenuSession {
                id: zyt_tmux::SessionId(0),
                name: String::new(),
                windows,
                calling: false,
            }),
            others: Vec::new(),
            more: 0,
            info: None,
        }
    }

    fn can(key: TmuxKey, menu: &Menu, places: &Places) -> bool {
        possible(key, Some(menu), places, &Follow::default())
    }

    #[test]
    fn one_pane_offers_nothing_that_needs_two() {
        let menu = menu(vec![window(0, 1)]);
        let none = Places::default();
        for key in [
            TmuxKey::NextLayout,
            TmuxKey::Layout(0),
            TmuxKey::BreakPane,
            TmuxKey::Zoom,
            TmuxKey::Swap(true),
            TmuxKey::Focus(Edge::Left),
            TmuxKey::FocusNext,
            TmuxKey::FocusLast,
            TmuxKey::Numbers,
            TmuxKey::Nudge(Edge::Left, 1),
            TmuxKey::StepWindow(1),
            TmuxKey::LastWindow,
            TmuxKey::Window(0),
            TmuxKey::StepSession(1),
        ] {
            assert!(!can(key, &menu, &none), "{key:?}");
        }
        for key in [
            TmuxKey::NewWindow,
            TmuxKey::CloseWindow,
            TmuxKey::RenameWindow,
            TmuxKey::Split(true),
            TmuxKey::ClosePane,
            TmuxKey::RenameSession,
            TmuxKey::Detach,
            TmuxKey::Prefix,
        ] {
            assert!(can(key, &menu, &none), "{key:?}");
        }
    }

    #[test]
    fn with_no_menu_only_the_prefix_stands() {
        let places = Places::default();
        let follow = Follow::default();
        assert!(possible(TmuxKey::Prefix, None, &places, &follow));
        assert!(!possible(TmuxKey::Numbers, None, &places, &follow));
    }

    #[test]
    fn a_window_key_needs_another_window() {
        let menu = menu(vec![window(0, 1), window(2, 1)]);
        let places = Places::default();
        assert!(can(TmuxKey::StepWindow(1), &menu, &places));
        assert!(can(TmuxKey::Window(2), &menu, &places));
        assert!(!can(TmuxKey::Window(0), &menu, &places), "the one shown");
        assert!(!can(TmuxKey::Window(1), &menu, &places), "no such window");
    }

    #[test]
    fn an_edge_moves_along_a_line_with_a_neighbour_on_either_side() {
        let menu = menu(vec![window(0, 3)]);
        let places = three();
        let nudge = |edge| TmuxKey::Nudge(edge, 1);
        assert!(can(nudge(Edge::Left), &menu, &places));
        assert!(can(nudge(Edge::Right), &menu, &places));
        assert!(!can(nudge(Edge::Up), &menu, &places));
        assert!(!can(nudge(Edge::Down), &menu, &places));
        assert!(!can(TmuxKey::Focus(Edge::Left), &menu, &places));
        assert!(can(TmuxKey::Focus(Edge::Right), &menu, &places));
    }

    #[test]
    fn a_zoomed_window_keeps_what_tmux_unzooms_for() {
        let mut shown = window(0, 2);
        shown.zoomed = Some(PaneId(0));
        let menu = menu(vec![shown]);
        let seen = Places::default();
        for key in [
            TmuxKey::NextLayout,
            TmuxKey::Swap(false),
            TmuxKey::Nudge(Edge::Up, 1),
            TmuxKey::FocusNext,
            TmuxKey::Numbers,
        ] {
            assert!(can(key, &menu, &seen), "{key:?}");
        }
        assert!(!can(TmuxKey::Focus(Edge::Right), &menu, &seen));
    }

    #[test]
    fn a_pane_too_small_does_not_split_that_way() {
        let menu = menu(vec![window(0, 2)]);
        let places = Places {
            panes: vec![place(0, 0, 0, 2, 24), place(1, 3, 0, 77, 24)],
            ..Places::default()
        };
        assert!(!can(TmuxKey::Split(true), &menu, &places));
        assert!(can(TmuxKey::Split(false), &menu, &places));
    }

    #[test]
    fn the_last_pane_must_still_be_there() {
        let menu = menu(vec![window(0, 2)]);
        let places = Places::default();
        let mut follow = Follow {
            last_pane: Some(PaneId(1)),
            ..Follow::default()
        };
        assert!(possible(TmuxKey::FocusLast, Some(&menu), &places, &follow));
        follow.last_pane = Some(PaneId(9));
        assert!(!possible(TmuxKey::FocusLast, Some(&menu), &places, &follow));
    }

    #[test]
    fn digits_with_a_gap_are_not_folded() {
        assert_eq!(fold_digits("012"), "0\u{2026}2");
        assert_eq!(fold_digits("013"), "013");
        assert_eq!(fold_digits("M-12345"), "M-1\u{2026}5");
    }

    #[test]
    fn every_action_has_an_id_and_every_id_of_tmux_an_action() {
        for (id, key) in TmuxKey::ALL {
            assert_eq!(key.id(), *id);
        }
        for binding in zyt_keymux::DEFAULT_BINDINGS {
            if binding.context == zyt_keymux::CONTEXT_TMUX {
                assert!(
                    TmuxKey::ALL.iter().any(|(id, _)| *id == binding.command),
                    "{}",
                    binding.command
                );
            }
        }
    }

    #[test]
    fn the_keys_of_a_group_are_one_row() {
        let next: Vec<(String, String)> = [
            ("0", "tmux.window.0"),
            ("1", "tmux.window.1"),
            ("2", "tmux.window.2"),
            ("M-1", "tmux.window.layout_1"),
            ("M-2", "tmux.window.layout_2"),
            ("M-3", "tmux.window.layout_3"),
            ("C-\u{2191}", "tmux.pane.resize_up"),
            ("C-\u{2193}", "tmux.pane.resize_down"),
        ]
        .map(|(keys, id)| (keys.to_string(), id.to_string()))
        .to_vec();
        let keys = |column| -> Vec<String> {
            rows(&next, column)
                .into_iter()
                .map(|(keys, _)| keys)
                .collect()
        };
        assert_eq!(keys("tmux.keys.windows"), ["0\u{2026}2", "M-1\u{2026}3"]);
        assert_eq!(keys("tmux.keys.panes"), ["C-\u{2191}\u{2193}"]);
    }

    #[test]
    fn the_keys_read_short() {
        let label = |text| key_label(&KeyStroke::parse(text).unwrap());
        assert_eq!(label("ctrl+up"), "C-\u{2191}");
        assert_eq!(label("alt+1"), "M-1");
        assert_eq!(label("%"), "%");
    }
}
