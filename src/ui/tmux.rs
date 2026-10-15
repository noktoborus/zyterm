//! The menu of tmux: the sessions and windows of tmux in control mode, and the
//! three actions, as the one menu of the window shows them.
//!
//! The session this client is on is the top level itself: its windows, each
//! with *Rename* one step in. Its name stands in the notice, under the version,
//! while there are other sessions. These follow, each with its windows one
//! level in. Then *New window*, *Close window* — one step in, since a window
//! closed ends its programs — and *Detach from tmux*.

use plate_menu::MenuItem;
use rust_i18n::t;
use zyt_tmux::{PaneId, SessionId, WindowId};

use crate::app::App;
use crate::tmux::{Calling, Choice, Info, Menu, MenuSession, MenuWindow};
use crate::ui::icons;
use crate::ui::menu::shorten;

/// Prefix of another session: `tmux.session:$2`.
pub const SESSION: &str = "tmux.session:";
/// Prefix of a window: `tmux.window:$1:@4`.
pub const WINDOW: &str = "tmux.window:";
/// Prefix of a pane of a window of several: `tmux.pane:$1:@4:%7`.
pub const PANE: &str = "tmux.pane:";
/// Prefix of the entry that renames a window of the session shown:
/// `tmux.rename_window:@4`.
pub const RENAME: &str = "tmux.rename_window:";
/// A new window in the session shown.
pub const NEW_WINDOW: &str = "tmux.new_window";
/// The entry that holds the closing of the window shown, one step in.
pub const CLOSE_WINDOW: &str = "tmux.close_window";
/// Prefix of the entry that closes a window: `tmux.close_window:@4`.
pub const CLOSE: &str = "tmux.close_window:";
/// Leaving the mode. Every menu of tmux holds it, so it names the menu.
pub const DETACH: &str = "tmux.detach";
/// The line that says how many sessions and windows were past the limit.
const MORE: &str = "tmux.more";

/// How much of a name the plate shows; the whole of it stands on the plate
/// beside the menu.
const NAME_LENGTH: usize = 48;

/// The window being renamed, while the question stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    /// The window.
    pub window: WindowId,
    /// Its name as it was when asked: what the question names.
    pub was: String,
    /// The name as typed so far; it starts as the name the window has.
    pub name: String,
    /// Whether the name standing in the field was selected, so what is typed
    /// first takes its place.
    pub selected: bool,
}

/// The entries of the menu of tmux over the model as it stands.
pub fn items(menu: &Menu) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if let Some(shown) = &menu.shown {
        items.extend(
            shown
                .windows
                .iter()
                .map(|window| shown_window_item(shown.id, window, menu)),
        );
    }
    if !menu.others.is_empty() {
        items.push(MenuItem::separator());
        items.extend(
            menu.others
                .iter()
                .map(|session| session_item(session, menu)),
        );
    }
    if menu.more > 0 {
        items.push(
            MenuItem::new(MORE, t!("tmux.more", count = menu.more))
                .enabled(false)
                .searchable(false),
        );
    }
    items.push(MenuItem::separator());
    items.push(MenuItem::new(NEW_WINDOW, t!("tmux.new_window")));
    items.push(close_item(menu));
    items.push(MenuItem::new(DETACH, t!("tmux.detach")));
    items
}

/// The entry the menu opens on: the window shown, so `Enter` at once changes
/// nothing.
pub fn opens_at(menu: &Menu) -> Option<String> {
    let session = menu.shown.as_ref()?;
    menu.window.map(|window| window_id(session.id, window))
}

/// What stands above the menu: what tmux says of the window shown — its
/// version, the size it gave the window and how many clients the session has,
/// more than one being why a window can be of another size than the area —
/// and, while there are other sessions, the session shown. Empty while there
/// is neither.
pub fn notice(menu: &Menu) -> String {
    let info = menu.info.as_ref().map(info_line);
    let session = menu
        .shown
        .as_ref()
        .filter(|_| !menu.others.is_empty())
        .map(|shown| session_label(&shown.name));
    [info, session]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

fn info_line(info: &Info) -> String {
    t!(
        "tmux.notice",
        version = info.version.as_str(),
        size = info.size.replace('x', "\u{d7}"),
        clients = info.clients.as_str()
    )
    .to_string()
}

fn session_label(name: &str) -> String {
    t!("tmux.session", name = name).to_string()
}

/// Does what an entry of the menu of tmux says. False while the identifier
/// names no entry of this menu.
pub fn chosen(app: &mut App, id: &str) -> bool {
    if id == MORE {
        return true;
    }
    if let Some(window) = id.strip_prefix(RENAME) {
        if let Some(window) = WindowId::parse(window.as_bytes()) {
            ask_rename(app, window);
        }
        return true;
    }
    let Some(choice) = choice_of(id) else {
        return false;
    };
    app.session.tmux_choose(choice);
    true
}

/// What an entry chooses.
pub fn choice_of(id: &str) -> Option<Choice> {
    if id == NEW_WINDOW {
        return Some(Choice::NewWindow);
    }
    if id == DETACH {
        return Some(Choice::Detach);
    }
    if let Some(window) = id.strip_prefix(CLOSE) {
        return WindowId::parse(window.as_bytes()).map(Choice::CloseWindow);
    }
    if let Some(session) = id.strip_prefix(SESSION) {
        return SessionId::parse(session.as_bytes()).map(Choice::Session);
    }
    if let Some(pane) = id.strip_prefix(PANE) {
        let mut parts = pane.split(':');
        let (Some(session), Some(window), Some(pane), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        return Some(Choice::Pane {
            session: SessionId::parse(session.as_bytes())?,
            window: WindowId::parse(window.as_bytes())?,
            pane: PaneId::parse(pane.as_bytes())?,
        });
    }
    let (session, window) = id.strip_prefix(WINDOW)?.split_once(':')?;
    Some(Choice::Window {
        session: SessionId::parse(session.as_bytes())?,
        window: WindowId::parse(window.as_bytes())?,
    })
}

fn window_id(session: SessionId, window: WindowId) -> String {
    format!("{WINDOW}{session}:{window}")
}

/// A window of the session shown: `Enter` goes there, `Right` to its panes
/// when it has several, and to *Rename*.
fn shown_window_item(session: SessionId, window: &MenuWindow, menu: &Menu) -> MenuItem {
    let mut children = pane_items(session, window, menu);
    if !children.is_empty() {
        children.push(MenuItem::separator());
    }
    children.push(
        MenuItem::new(format!("{RENAME}{}", window.id), t!("tmux.rename_window")).searchable(false),
    );
    window_item(session, window, menu)
        .choosable(true)
        .children(children)
}

/// A window: `index: name`, the mark of the window shown when it is one pane,
/// the count of its panes when it holds several — and then its panes one level
/// in, `Enter` still going to the window.
fn window_item(session: SessionId, window: &MenuWindow, menu: &Menu) -> MenuItem {
    let item = named(
        MenuItem::new(window_id(session, window.id), ""),
        &window.label,
    );
    let (detail, item) = match window.panes {
        1 if menu.window == Some(window.id) => (icons::CURRENT.to_string(), item),
        1 => (String::new(), item),
        panes => (
            panes.to_string(),
            item.choosable(true)
                .children(pane_items(session, window, menu)),
        ),
    };
    calling(item, window, &detail)
}

/// The mark of a window calling before its detail — `🔔` a bell, `✉` a
/// notification — the text of the notification on the plate beside the menu,
/// and a word of its own for the search.
fn calling(item: MenuItem, window: &MenuWindow, detail: &str) -> MenuItem {
    let mark = match &window.calling {
        Calling::None => return item.detail(detail),
        Calling::Bell => icons::BELL,
        Calling::Note { .. } => icons::NOTE,
    };
    let item = item
        .detail(format!("{mark}  {detail}").trim_end().to_string())
        .search(t!("tmux.calling_search"));
    let Calling::Note { title, body, since } = &window.calling else {
        return item;
    };
    let heading = match title.is_empty() {
        true => icons::NOTE.to_string(),
        false => format!("{} {title}", icons::NOTE),
    };
    let minutes = since.as_secs() / 60;
    let ago = match minutes {
        0 => t!("tmux.note_now").to_string(),
        minutes => t!("tmux.note_minutes", minutes = minutes).to_string(),
    };
    item.full(format!("{}\n{heading}\n{body}\n{ago}", window.label))
}

/// The panes of a window of several: `1.1 bash`, the mark of the pane shown.
fn pane_items(session: SessionId, window: &MenuWindow, menu: &Menu) -> Vec<MenuItem> {
    window
        .pane_list
        .iter()
        .map(|pane| {
            let item = named(
                MenuItem::new(format!("{PANE}{session}:{}:{}", window.id, pane.id), ""),
                &pane.label,
            );
            match menu.pane == Some(pane.id) {
                true => item.detail(icons::CURRENT),
                false => item,
            }
        })
        .collect()
}

/// Another session: `Enter` goes to its current window, `Right` to its windows.
fn session_item(session: &MenuSession, menu: &Menu) -> MenuItem {
    let windows: Vec<MenuItem> = session
        .windows
        .iter()
        .map(|window| window_item(session.id, window, menu))
        .collect();
    let current = session
        .windows
        .iter()
        .find(|window| window.current)
        .map(|window| window_id(session.id, window.id));
    let item = named(
        MenuItem::new(format!("{SESSION}{}", session.id), ""),
        &session_label(&session.name),
    )
    .detail(match session.calling {
        true => format!("{}  {}", icons::BELL, session.windows.len()),
        false => session.windows.len().to_string(),
    })
    .choosable(true)
    .children(windows);
    match current {
        Some(current) => item.opens_at(current),
        None => item,
    }
}

/// *Close window*, holding the one entry that closes the window shown.
fn close_item(menu: &Menu) -> MenuItem {
    let item = MenuItem::new(CLOSE_WINDOW, t!("tmux.close_window"));
    let shown = menu.shown.as_ref().and_then(|session| {
        session
            .windows
            .iter()
            .find(|window| Some(window.id) == menu.window)
    });
    match shown {
        Some(window) => item.children(vec![
            MenuItem::new(
                format!("{CLOSE}{}", window.id),
                t!(
                    "tmux.close_named",
                    window = shorten(&window.label, NAME_LENGTH)
                ),
            )
            .searchable(false),
        ]),
        None => item.enabled(false),
    }
}

/// Puts the question of the new name of a window of the session shown, its
/// name as it is standing in the field.
fn ask_rename(app: &mut App, window: WindowId) {
    let name = app
        .session
        .tmux_menu()
        .and_then(|menu| menu.shown)
        .and_then(|shown| shown.windows.into_iter().find(|entry| entry.id == window))
        .map(|entry| entry.name);
    if let Some(name) = name {
        app.ui.tmux_rename = Some(Rename {
            window,
            was: name.clone(),
            name,
            selected: false,
        });
    }
}

/// Asks the new name of a window, in front of the rest of the interface.
///
/// `Enter` and the first button rename it; `Esc`, a click beside the window
/// and the second button leave it as it was. An empty name is not sent: a
/// window named nothing is read in the menu as its index alone. A name left as
/// it was sends nothing either.
pub fn rename(app: &mut App, context: &egui::Context) {
    let Some(mut rename) = app.ui.tmux_rename.take() else {
        return;
    };
    let id = egui::Id::new("tmux_rename");
    let field = id.with("name");
    let mut answer = None;

    let modal = egui::Modal::new(id).show(context, |ui| {
        ui.set_width(320.0);
        ui.heading(t!(
            "tmux.rename_title",
            window = shorten(&rename.was, NAME_LENGTH)
        ));
        ui.add_space(8.0);
        let output = egui::TextEdit::singleline(&mut rename.name)
            .id(field)
            .desired_width(f32::INFINITY)
            .show(ui);
        if !rename.selected {
            let mut state = output.state;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::select_all(&output.galley)));
            state.store(ui.ctx(), field);
            rename.selected = true;
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let named = !rename.name.is_empty();
            if ui
                .add_enabled(named, egui::Button::new(t!("tmux.rename")))
                .clicked()
            {
                answer = Some(true);
            }
            if ui.button(t!("confirm.cancel")).clicked() {
                answer = Some(false);
            }
        });
    });
    context.memory_mut(|memory| memory.request_focus(field));

    let escaped = context.input(|input| input.key_pressed(egui::Key::Escape));
    if modal.backdrop_response.clicked() || escaped {
        answer = Some(false);
    }
    if context.input(|input| input.key_pressed(egui::Key::Enter)) && !rename.name.is_empty() {
        answer = Some(true);
    }

    match answer {
        Some(true) if rename.name != rename.was => app.session.tmux_choose(Choice::RenameWindow {
            window: rename.window,
            name: rename.name,
        }),
        Some(_) => {}
        None => app.ui.tmux_rename = Some(rename),
    }
}

/// The label of an entry, cut to the plate; the whole of it beside the menu
/// when it was cut.
fn named(item: MenuItem, name: &str) -> MenuItem {
    let shown = shorten(name, NAME_LENGTH);
    let cut = shown != name;
    let item = MenuItem {
        label: shown,
        ..item
    };
    match cut {
        true => item.full(name),
        false => item,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u32, label: &str, panes: usize, current: bool) -> MenuWindow {
        MenuWindow {
            id: WindowId(id),
            label: label.to_string(),
            name: label
                .split_once(": ")
                .map_or(label, |(_, name)| name)
                .to_string(),
            panes,
            pane_list: match panes {
                1 => Vec::new(),
                _ => (0..panes as u32)
                    .map(|index| crate::tmux::MenuPane {
                        id: PaneId(id * 10 + index),
                        label: format!("{}.{index} sh", label.split(':').next().unwrap()),
                    })
                    .collect(),
            },
            current,
            calling: Calling::None,
        }
    }

    fn menu() -> Menu {
        Menu {
            shown: Some(MenuSession {
                id: SessionId(1),
                name: "main".to_string(),
                calling: false,
                windows: vec![
                    window(0, "0: bash", 1, false),
                    window(4, "1: vim", 2, false),
                    window(5, "2: htop", 1, true),
                ],
            }),
            others: vec![MenuSession {
                id: SessionId(2),
                name: "logs".to_string(),
                calling: false,
                windows: vec![
                    window(9, "0: make", 1, false),
                    window(10, "1: tail", 1, true),
                ],
            }],
            window: Some(WindowId(5)),
            pane: Some(PaneId(50)),
            more: 0,
            info: None,
        }
    }

    const SEPARATOR: &str = "—";

    fn ids(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item.separator {
                true => SEPARATOR.to_string(),
                false => item.id.clone(),
            })
            .collect()
    }

    #[test]
    fn the_session_shown_is_the_top_level_and_the_others_follow() {
        let items = items(&menu());
        assert_eq!(
            ids(&items),
            [
                "tmux.window:$1:@0",
                "tmux.window:$1:@4",
                "tmux.window:$1:@5",
                SEPARATOR,
                "tmux.session:$2",
                SEPARATOR,
                NEW_WINDOW,
                CLOSE_WINDOW,
                DETACH,
            ]
        );
        assert_eq!(items[1].detail, "2", "a window of two panes");
        assert_eq!(items[2].detail, icons::CURRENT, "the window shown");
        assert_eq!(items[4].label, t!("tmux.session", name = "logs"));
        assert_eq!(items[4].detail, "2", "a session of two windows");
        assert_eq!(
            ids(&items[4].children),
            ["tmux.window:$2:@9", "tmux.window:$2:@10"]
        );
        assert_eq!(items[4].opens_at, "tmux.window:$2:@10");
        assert_eq!(ids(&items[7].children), ["tmux.close_window:@5"]);
        assert_eq!(opens_at(&menu()).as_deref(), Some("tmux.window:$1:@5"));
    }

    #[test]
    fn a_window_of_several_panes_holds_them_one_level_in() {
        let mut menu = menu();
        menu.window = Some(WindowId(4));
        menu.pane = Some(PaneId(41));
        let items = items(&menu);
        assert!(items[1].choosable, "Enter still goes to the window");
        assert_eq!(
            ids(&items[1].children),
            [
                "tmux.pane:$1:@4:%40",
                "tmux.pane:$1:@4:%41",
                SEPARATOR,
                "tmux.rename_window:@4",
            ]
        );
        assert_eq!(items[1].children[0].label, "1.0 sh");
        assert_eq!(items[1].children[0].detail, "");
        assert_eq!(
            items[1].children[1].detail,
            icons::CURRENT,
            "the pane shown"
        );
        assert_eq!(
            choice_of("tmux.pane:$1:@4:%41"),
            Some(Choice::Pane {
                session: SessionId(1),
                window: WindowId(4),
                pane: PaneId(41),
            })
        );
        assert_eq!(choice_of("tmux.pane:$1:@4"), None);
        assert_eq!(choice_of("tmux.pane:$1:@4:%1:x"), None);
    }

    #[test]
    fn a_window_calling_says_how_before_its_detail() {
        let mut menu = menu();
        let windows = &mut menu.shown.as_mut().unwrap().windows;
        windows[0].calling = Calling::Bell;
        windows[1].calling = Calling::Note {
            title: "make".to_string(),
            body: "done".to_string(),
            since: std::time::Duration::from_secs(150),
        };
        menu.others[0].calling = true;
        let items = items(&menu);
        assert_eq!(items[0].detail, icons::BELL);
        assert_eq!(items[1].detail, format!("{}  2", icons::NOTE));
        assert!(items[1].full.contains("make") && items[1].full.contains("done"));
        assert!(
            items[1].full.contains('2'),
            "how long ago: {}",
            items[1].full
        );
        assert_eq!(items[0].search, t!("tmux.calling_search"));
        assert_eq!(
            items[4].detail,
            format!("{}  2", icons::BELL),
            "a session calling"
        );
    }

    #[test]
    fn a_window_of_the_session_shown_is_renamed_one_step_in() {
        let items = items(&menu());
        assert!(items[0].choosable, "Enter still goes to the window");
        assert_eq!(ids(&items[0].children), ["tmux.rename_window:@0"]);
        assert!(
            items[4]
                .children
                .iter()
                .all(|window| window.children.is_empty()),
            "a window of another session is not renamed from here"
        );
    }

    #[test]
    fn no_other_session_is_no_block_and_no_separator() {
        let mut alone = menu();
        alone.others.clear();
        let separators = items(&alone).iter().filter(|item| item.separator).count();
        assert_eq!(separators, 1, "only the one before the actions");
    }

    #[test]
    fn the_session_shown_is_named_in_the_notice_while_there_are_others() {
        let mut menu = menu();
        menu.info = Some(Info {
            version: "3.3a".to_string(),
            size: "120x40".to_string(),
            clients: "2".to_string(),
        });
        let text = notice(&menu);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("3.3a") && lines[0].contains("120\u{d7}40"));
        assert_eq!(lines[1], t!("tmux.session", name = "main"));

        menu.others.clear();
        assert_eq!(notice(&menu).lines().count(), 1, "one session: no name");
        menu.info = None;
        assert_eq!(notice(&menu), "", "nothing to say");
    }

    #[test]
    fn a_long_name_is_cut_and_whole_beside_the_menu() {
        let mut long = menu();
        let name = "x".repeat(NAME_LENGTH * 2);
        long.shown.as_mut().unwrap().windows[0].label = name.clone();
        let items = items(&long);
        assert_eq!(
            items[0].label.chars().count(),
            NAME_LENGTH + 1,
            "cut, and an ellipsis"
        );
        assert_eq!(items[0].full, name);
    }

    #[test]
    fn what_is_past_the_limit_is_said_in_a_line() {
        let mut many = menu();
        many.more = 12;
        let items = items(&many);
        let more = items
            .iter()
            .find(|item| item.id == MORE)
            .expect("the line of the rest");
        assert!(more.label.contains("12") && !more.enabled);
    }

    #[test]
    fn every_entry_names_its_choice() {
        assert_eq!(
            choice_of("tmux.window:$1:@4"),
            Some(Choice::Window {
                session: SessionId(1),
                window: WindowId(4)
            })
        );
        assert_eq!(
            choice_of("tmux.session:$2"),
            Some(Choice::Session(SessionId(2)))
        );
        assert_eq!(choice_of(NEW_WINDOW), Some(Choice::NewWindow));
        assert_eq!(
            choice_of("tmux.close_window:@4"),
            Some(Choice::CloseWindow(WindowId(4)))
        );
        assert_eq!(
            choice_of(CLOSE_WINDOW),
            None,
            "it holds the entry that closes"
        );
        assert_eq!(choice_of(DETACH), Some(Choice::Detach));
        assert_eq!(choice_of("tmux.window:$1"), None);
        assert_eq!(choice_of("history:ls"), None);
    }
}
