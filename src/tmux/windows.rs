//! Sessions, windows and layouts of tmux, as the lists and the notifications
//! say them: the one model of them this side keeps.

use std::collections::{BTreeMap, BTreeSet};

use zyt_tmux::{Content, Layout, PaneId, SessionId, WindowId};

/// No more sessions and windows than this are kept: the numbers come from the
/// other end of a line.
pub const MAX_ENTRIES: usize = 1000;

/// A pane as `list-panes` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPane {
    /// The pane.
    pub id: PaneId,
    /// Its index in the window.
    pub index: u32,
    /// What runs in it.
    pub command: String,
}

/// One window of tmux.
#[derive(Debug, Default, Clone)]
pub struct Window {
    /// The session it is in, when a list said so.
    pub session: Option<SessionId>,
    /// Its index in the session.
    pub index: Option<u32>,
    /// Its name.
    pub name: String,
    /// Its active pane.
    pub active: Option<PaneId>,
    /// Every pane of it.
    pub layout: Option<Layout>,
    /// The panes seen: one when the window is zoomed.
    pub visible: Option<Layout>,
    /// Its panes as the last `list-panes` named them, by their index.
    pub listed: Vec<ListedPane>,
    /// tmux says a bell rang in it while it was not current: `!` of its flags,
    /// from the lists and from the subscription of 3.2.
    pub bell: bool,
    /// The pane chosen here, shown in place of the active one while the window
    /// holds it, until another is chosen. Nothing is sent for it: the active
    /// pane is tmux's, and every client sees it.
    pub chosen: Option<PaneId>,
}

impl Window {
    /// How the window is named where it is read: `1: vim`.
    pub fn label(&self) -> String {
        match self.index {
            Some(index) => format!("{index}: {}", self.name),
            None => self.name.clone(),
        }
    }

    /// How a pane of it is named where it is read: `1.1 bash`, the index of
    /// the window, of the pane, and what runs in it; the id of the pane before
    /// a list named it.
    pub fn pane_label(&self, pane: PaneId) -> String {
        let listed = self.listed.iter().find(|listed| listed.id == pane);
        match (self.index, listed) {
            (Some(window), Some(listed)) => {
                format!("{window}.{} {}", listed.index, listed.command)
            }
            (None, Some(listed)) => format!("{} {}", listed.index, listed.command),
            (_, None) => pane.to_string(),
        }
    }

    /// The pane this client shows of the window: the one chosen here, or the
    /// active one.
    pub fn shown_pane(&self) -> Option<PaneId> {
        self.chosen
            .filter(|chosen| self.panes().contains(chosen))
            .or(self.active)
    }

    /// The pane tmux zoomed the window on: the one pane seen of several.
    pub fn zoomed(&self) -> Option<PaneId> {
        let seen = self.visible.as_ref()?.panes();
        match (seen.as_slice(), self.panes().len()) {
            ([pane], 2..) => Some(*pane),
            _ => None,
        }
    }

    /// The cell of a pane in the layout it is seen in.
    pub fn cell(&self, pane: PaneId) -> Option<&Layout> {
        self.visible
            .as_ref()
            .and_then(|visible| visible.find(pane))
            .or_else(|| self.layout.as_ref().and_then(|layout| layout.find(pane)))
    }

    /// Every pane of the window: those of its layout; before a layout came,
    /// those a list named, or the active pane.
    pub fn panes(&self) -> Vec<PaneId> {
        if let Some(layout) = &self.layout {
            return layout.panes();
        }
        if !self.listed.is_empty() {
            return self.listed.iter().map(|pane| pane.id).collect();
        }
        self.active.into_iter().collect()
    }
}

/// What a listing saw, so what it no longer names goes when it ends.
#[derive(Debug, Default)]
struct Seen {
    sessions: BTreeSet<SessionId>,
    windows: BTreeSet<WindowId>,
    panes: BTreeMap<WindowId, Vec<ListedPane>>,
}

/// What this client knows of the server.
#[derive(Debug, Default)]
pub struct Model {
    /// The session this client is on.
    pub session: Option<SessionId>,
    sessions: BTreeMap<SessionId, String>,
    windows: BTreeMap<WindowId, Window>,
    current: BTreeMap<SessionId, WindowId>,
    /// A window of the session of this client chosen here and shown ahead
    /// of tmux, until tmux says it is current or another one is.
    aimed: Option<WindowId>,
    seen: Seen,
    /// Sessions and windows the last listings named past `MAX_ENTRIES`.
    more: usize,
}

impl Model {
    /// The name of the session this client is on.
    pub fn session_name(&self) -> Option<&str> {
        self.session
            .and_then(|session| self.sessions.get(&session))
            .map(String::as_str)
    }

    /// The sessions, in the order of their ids, with their names.
    pub fn sessions(&self) -> impl Iterator<Item = (SessionId, &str)> {
        self.sessions
            .iter()
            .map(|(session, name)| (*session, name.as_str()))
    }

    /// The windows of a session, in the order of their index.
    pub fn windows_of(&self, session: SessionId) -> Vec<(WindowId, &Window)> {
        let mut windows: Vec<(WindowId, &Window)> = self
            .windows
            .iter()
            .filter(|(_, window)| window.session == Some(session))
            .map(|(id, window)| (*id, window))
            .collect();
        windows.sort_by_key(|(id, window)| (window.index.unwrap_or(u32::MAX), *id));
        windows
    }

    /// Sessions and windows past `MAX_ENTRIES` the last listings named.
    pub fn more(&self) -> usize {
        self.more
    }

    /// The window shown in the session of this client.
    pub fn current_window(&self) -> Option<WindowId> {
        self.session.and_then(|session| self.current_of(session))
    }

    /// The window shown here: the one chosen ahead of tmux, or the one tmux
    /// says is current.
    pub fn shown_window(&self) -> Option<WindowId> {
        self.aimed
            .filter(|window| self.windows.contains_key(window))
            .or_else(|| self.current_window())
    }

    /// A window of the session of this client chosen here: shown before
    /// tmux says it is current.
    pub fn aim(&mut self, window: WindowId) {
        self.aimed = Some(window);
    }

    /// The window chosen ahead of tmux, while tmux has not said it is current.
    pub fn aimed(&self) -> Option<WindowId> {
        self.aimed
    }

    /// The window chosen ahead of tmux is given up: tmux refused it, or
    /// somebody else chose another; what tmux said is shown.
    pub fn give_up_aim(&mut self) {
        self.aimed = None;
    }

    /// The window shown in a session.
    pub fn current_of(&self, session: SessionId) -> Option<WindowId> {
        self.current.get(&session).copied()
    }

    /// A window.
    pub fn window(&self, window: WindowId) -> Option<&Window> {
        self.windows.get(&window)
    }

    /// The window a pane is in.
    pub fn window_of(&self, pane: PaneId) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, window)| window.panes().contains(&pane))
            .map(|(id, _)| *id)
    }

    /// The session a pane is in, when it is known.
    pub fn session_of(&self, pane: PaneId) -> Option<SessionId> {
        self.window_of(pane)
            .and_then(|window| self.windows.get(&window))
            .and_then(|window| window.session)
    }

    /// The pane to show: of the window shown, the pane chosen here, or the
    /// active one.
    pub fn pane_to_show(&self) -> Option<PaneId> {
        self.shown_window()
            .and_then(|window| self.windows.get(&window))
            .and_then(Window::shown_pane)
    }

    /// The pane chosen in a window is let go: the active one of tmux is
    /// shown, as after a split, which makes the new pane active.
    pub fn forget_pane(&mut self, window: WindowId) {
        if let Some(entry) = self.windows.get_mut(&window) {
            entry.chosen = None;
        }
    }

    /// A pane chosen in the menu: shown in place of the active pane of its
    /// window. False when the window does not hold it.
    pub fn choose_pane(&mut self, window: WindowId, pane: PaneId) -> bool {
        let Some(entry) = self.windows.get_mut(&window) else {
            return false;
        };
        if !entry.panes().contains(&pane) {
            return false;
        }
        entry.chosen = Some(pane);
        true
    }

    /// `%session-changed`.
    pub fn session_changed(&mut self, session: SessionId, name: &str) {
        self.session = Some(session);
        self.aimed = None;
        self.name_session(session, name);
    }

    /// `%session-renamed`.
    pub fn name_session(&mut self, session: SessionId, name: &str) {
        if self.sessions.len() < MAX_ENTRIES || self.sessions.contains_key(&session) {
            self.sessions.insert(session, name.to_string());
        }
    }

    /// `%session-window-changed`.
    pub fn window_changed(&mut self, session: SessionId, window: WindowId) {
        self.current.insert(session, window);
        if self.session == Some(session) && self.aimed == Some(window) {
            self.aimed = None;
        }
        self.entry(window).session.get_or_insert(session);
    }

    /// `%window-pane-changed`. A pane chosen here stands: the notification
    /// does not say whether tmux changed the active pane before the choice or
    /// after it, and on a slow line one made before comes after.
    pub fn pane_changed(&mut self, window: WindowId, pane: PaneId) {
        self.entry(window).active = Some(pane);
    }

    /// `%window-renamed` and its unlinked twin.
    pub fn rename(&mut self, window: WindowId, name: &str) {
        self.entry(window).name = name.to_string();
    }

    /// `%window-add`: a window of the session of this client.
    pub fn add(&mut self, window: WindowId) {
        let session = self.session;
        let entry = self.entry(window);
        if entry.session.is_none() {
            entry.session = session;
        }
    }

    /// `%window-close` and `%unlinked-window-close`; the panes it held.
    pub fn close(&mut self, window: WindowId) -> Vec<PaneId> {
        self.current.retain(|_, current| *current != window);
        if self.aimed == Some(window) {
            self.aimed = None;
        }
        self.windows
            .remove(&window)
            .map(|closed| closed.panes())
            .unwrap_or_default()
    }

    /// `%layout-change`; the panes the window no longer holds.
    pub fn layout(
        &mut self,
        window: WindowId,
        layout: Layout,
        visible: Option<Layout>,
    ) -> Vec<PaneId> {
        let entry = self.entry(window);
        let before = entry.panes();
        let now = layout.panes();
        if entry.active.is_none_or(|active| !now.contains(&active)) {
            entry.active = now.first().copied();
        }
        entry.listed.retain(|pane| now.contains(&pane.id));
        entry.chosen = entry.chosen.filter(|chosen| now.contains(chosen));
        entry.layout = Some(layout);
        entry.visible = visible;
        before
            .into_iter()
            .filter(|pane| !now.contains(pane))
            .collect()
    }

    /// A line of `list-sessions`: `$S name`.
    pub fn list_session(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let Some((id, name)) = text.split_once(' ') else {
            return;
        };
        let Some(session) = SessionId::parse(id.as_bytes()) else {
            return;
        };
        self.seen.sessions.insert(session);
        if self.sessions.len() >= MAX_ENTRIES && !self.sessions.contains_key(&session) {
            self.more += 1;
            return;
        }
        self.sessions.insert(session, name.to_string());
    }

    /// The end of `list-sessions`: a session it did not name is gone.
    pub fn sessions_listed(&mut self) {
        let seen = std::mem::take(&mut self.seen.sessions);
        self.sessions.retain(|session, _| seen.contains(session));
        self.current.retain(|session, _| seen.contains(session));
    }

    /// A line of `list-windows -a`: `$S @W index active bell %P name`.
    pub fn list_window(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let mut fields = text.splitn(7, ' ');
        let (Some(session), Some(window), Some(index), Some(active), Some(bell), Some(pane)) = (
            fields
                .next()
                .and_then(|word| SessionId::parse(word.as_bytes())),
            fields
                .next()
                .and_then(|word| WindowId::parse(word.as_bytes())),
            fields.next().and_then(|word| word.parse::<u32>().ok()),
            fields.next(),
            fields.next(),
            fields
                .next()
                .and_then(|word| PaneId::parse(word.as_bytes())),
        ) else {
            return;
        };
        let name = fields.next().unwrap_or_default().to_string();
        self.seen.windows.insert(window);
        if self.windows.len() >= MAX_ENTRIES && !self.windows.contains_key(&window) {
            self.more += 1;
            return;
        }
        let entry = self.entry(window);
        entry.session = Some(session);
        entry.index = Some(index);
        entry.active = Some(pane);
        entry.name = name;
        entry.bell = bell == "1";
        if active == "1" {
            self.current.insert(session, window);
        }
    }

    /// The bell flag of a window, from `%subscription-changed`.
    pub fn ring(&mut self, window: WindowId, bell: bool) {
        if let Some(entry) = self.windows.get_mut(&window) {
            entry.bell = bell;
        }
    }

    /// The end of `list-windows`: a window it did not name is gone; the panes
    /// it held are returned.
    pub fn windows_listed(&mut self) -> Vec<PaneId> {
        let seen = std::mem::take(&mut self.seen.windows);
        let gone: Vec<WindowId> = self
            .windows
            .keys()
            .filter(|window| !seen.contains(window))
            .copied()
            .collect();
        gone.into_iter()
            .flat_map(|window| self.close(window))
            .collect()
    }

    /// A line of `list-panes -a`: `$S @W %P window-index pane-index command`.
    pub fn list_pane(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line);
        let mut fields = text.splitn(6, ' ');
        let (Some(_session), Some(window), Some(pane), Some(_window_index), Some(index)) = (
            fields
                .next()
                .and_then(|word| SessionId::parse(word.as_bytes())),
            fields
                .next()
                .and_then(|word| WindowId::parse(word.as_bytes())),
            fields
                .next()
                .and_then(|word| PaneId::parse(word.as_bytes())),
            fields.next(),
            fields.next().and_then(|word| word.parse::<u32>().ok()),
        ) else {
            return;
        };
        let command = fields.next().unwrap_or_default().to_string();
        self.seen.panes.entry(window).or_default().push(ListedPane {
            id: pane,
            index,
            command,
        });
    }

    /// The end of `list-panes`: every window takes the panes it named.
    pub fn panes_listed(&mut self) {
        let mut seen = std::mem::take(&mut self.seen.panes);
        for (id, window) in &mut self.windows {
            let mut listed = seen.remove(id).unwrap_or_default();
            listed.sort_by_key(|pane| pane.index);
            window.listed = listed;
        }
    }

    /// The listings are asked again: what is past the limit is counted anew.
    pub fn listing(&mut self) {
        self.more = 0;
    }

    fn entry(&mut self, window: WindowId) -> &mut Window {
        self.windows.entry(window).or_default()
    }
}

/// The size of a pane in a layout: a cell that is not a pane has none.
pub fn pane_size(cell: &Layout) -> Option<(usize, usize)> {
    matches!(cell.content, Content::Pane(_)).then_some((cell.columns as usize, cell.rows as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_say_which_pane_to_show() {
        let mut model = Model::default();
        model.session_changed(SessionId(0), "zyt");
        model.list_session(b"$0 zyt");
        model.list_window(b"$0 @0 0 0 0 %0 sh");
        model.list_window(b"$0 @1 1 1 0 %3 vim with spaces");
        assert_eq!(model.current_window(), Some(WindowId(1)));
        assert_eq!(model.pane_to_show(), Some(PaneId(3)));
        assert_eq!(
            model.window(WindowId(1)).unwrap().label(),
            "1: vim with spaces"
        );
        assert_eq!(model.session_name(), Some("zyt"));
    }

    #[test]
    fn notifications_move_the_window_and_the_pane_shown() {
        let mut model = Model::default();
        model.session_changed(SessionId(0), "zyt");
        model.list_window(b"$0 @0 0 1 0 %0 sh");
        model.window_changed(SessionId(0), WindowId(2));
        model.pane_changed(WindowId(2), PaneId(5));
        assert_eq!(model.pane_to_show(), Some(PaneId(5)));
        model.window_changed(SessionId(1), WindowId(0));
        assert_eq!(
            model.pane_to_show(),
            Some(PaneId(5)),
            "another session's window"
        );
    }

    #[test]
    fn a_layout_without_a_pane_closes_it() {
        let mut model = Model::default();
        let two = Layout::parse(b"8205,80x24,0,0{40x24,0,0,0,39x24,41,0,1}").unwrap();
        let one = Layout::parse(b"b25e,80x24,0,0,1").unwrap();
        assert!(model.layout(WindowId(0), two, None).is_empty());
        model.pane_changed(WindowId(0), PaneId(0));
        assert_eq!(model.layout(WindowId(0), one, None), [PaneId(0)]);
        assert_eq!(model.window(WindowId(0)).unwrap().active, Some(PaneId(1)));
        assert_eq!(model.window_of(PaneId(1)), Some(WindowId(0)));
        assert_eq!(model.close(WindowId(0)), [PaneId(1)]);
    }

    #[test]
    fn a_line_of_a_list_that_cannot_be_read_is_passed_over() {
        let mut model = Model::default();
        model.list_window(b"rubbish");
        model.list_window(b"$0 @x 0 1 %0 sh");
        model.list_session(b"");
        assert_eq!(model.current_window(), None);
    }

    #[test]
    fn windows_come_in_the_order_of_their_index_with_their_panes() {
        let mut model = Model::default();
        model.list_window(b"$0 @7 2 0 0 %9 top");
        model.list_window(b"$0 @3 0 1 0 %0 sh");
        model.list_window(b"$1 @4 0 1 0 %5 make");
        model.windows_listed();
        model.list_pane(b"$0 @7 %9 2 0 top");
        model.list_pane(b"$0 @7 %8 2 1 bash with args");
        model.panes_listed();
        let windows: Vec<WindowId> = model
            .windows_of(SessionId(0))
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(windows, [WindowId(3), WindowId(7)]);
        let listed = &model.window(WindowId(7)).unwrap().listed;
        assert_eq!(listed[1].command, "bash with args");
        assert_eq!(model.session_of(PaneId(5)), Some(SessionId(1)));
    }

    #[test]
    fn what_a_listing_no_longer_names_is_gone() {
        let mut model = Model::default();
        model.list_session(b"$0 a");
        model.list_session(b"$1 b");
        model.sessions_listed();
        model.list_window(b"$0 @0 0 1 0 %0 sh");
        model.list_window(b"$1 @1 0 1 0 %1 sh");
        model.windows_listed();

        model.list_session(b"$0 a");
        model.sessions_listed();
        model.list_window(b"$0 @0 0 1 0 %0 sh");
        assert_eq!(model.windows_listed(), [PaneId(1)]);
        assert_eq!(model.sessions().count(), 1);
    }

    #[test]
    fn past_the_limit_is_counted_and_not_kept() {
        let mut model = Model::default();
        model.listing();
        for window in 0..MAX_ENTRIES + 5 {
            model.list_window(format!("$0 @{window} {window} 0 0 %{window} sh").as_bytes());
        }
        assert_eq!(model.windows_of(SessionId(0)).len(), MAX_ENTRIES);
        assert_eq!(model.more(), 5);
    }
}
