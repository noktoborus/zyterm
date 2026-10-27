//! tmux in control mode: the panes of tmux as terminals of this window.
//!
//! `Session` finds the mode in the bytes of its line with the splitter of
//! `zyt-tmux` and hands what is inside it to [`Control`], which reads the
//! protocol, keeps a terminal for every pane whose output comes or that was
//! shown, and says what to
//! write back. The protocol itself is `crates/zyt-tmux/README.md`; why the mode
//! is built as it is, `docs/tmux.md`.

mod flow;
mod history;
mod input;
pub mod key;
mod marks;
mod menu;
mod pane;
mod sink;
mod windows;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use rust_i18n::t;
use zyt_term::{Terminal, TerminalEvent};
use zyt_tmux::{Client, Command, Layout, PaneId, Part, SessionId, WindowId};

pub use menu::{Calling, Choice, Info, Menu, MenuPane, MenuSession, MenuWindow};

use marks::Marks;

use input::Held;
use pane::{Pane, PaneState};
use windows::{Model, pane_size};

/// The oldest tmux served: before 3.0 `send-keys -H` is refused, and nothing
/// could be typed.
const OLDEST: (u32, u32) = (3, 0);

/// Input sent in one `send-keys` at most: a paste goes in pieces of this, so a
/// key typed during it is not held behind all of it.
const PIECE: usize = 1024;

/// The first tmux with `refresh-client -B`: the bells of the windows come as
/// they ring, not only with the lists.
const SUBSCRIPTIONS: (u32, u32) = (3, 2);

/// How far behind the screen shown has to be for `RX` to stand: the least it
/// shows, a second.
const LAG_SHOWN: Duration = Duration::from_secs(1);

/// Snapshots asked for one pane in a row before it is given up on: a pane
/// closed under its snapshot answers `%error` every time.
const SNAPSHOT_TRIES: u32 = 3;

/// How the mode stands once it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The protocol is read and commands are written.
    Control,
    /// `detach` was sent or `%exit` read; `ESC \` is awaited, nothing more is
    /// written.
    Leaving,
}

/// How a terminal of a pane is made: what every terminal of the session has.
#[derive(Debug, Clone, Copy)]
pub struct PaneConfig {
    /// Lines of scrollback.
    pub scrollback: usize,
    /// What a program may do with the clipboard.
    pub clipboard: zyt_term::ClipboardAccess,
    /// What a picture may do.
    pub pictures: zyt_term::PictureAccess,
    /// How far a screen may reach past the area, columns and rows: the size of
    /// a pane comes from the other end of the line.
    pub overflow: (usize, usize),
}

/// What `Control` has to do once the chunk is read, when the client is free
/// to send.
#[derive(Debug, PartialEq, Eq)]
enum Todo {
    /// The opening pair ended well: ask the version.
    AskVersion,
    /// The version is known and served: the size, the lists.
    Start,
    /// Show the pane the model says, snapshotting it if it has no terminal.
    Show,
    /// Snapshot a pane again.
    Snapshot(PaneId),
    /// Leave with `detach`.
    Detach,
    /// Control mode started inside a pane: `detach` typed into it.
    DetachNested(PaneId),
    /// A `send-keys` ended: the next keys may go.
    Keys,
    /// Sessions came or went: the lists are asked again.
    Lists,
    /// A pane tmux paused is let go and snapshotted under a gap.
    Resume(PaneId),
    /// The modes of a deeper snapshot came: its captures are asked.
    Deep(PaneId),
}

/// The state the replies and notifications change, apart from the client that
/// reads them.
struct State {
    model: Model,
    panes: BTreeMap<PaneId, Pane>,
    shown: Option<PaneId>,
    wanted: Option<PaneId>,
    version: Option<String>,
    phase: Phase,
    todo: Vec<Todo>,
    told: Vec<String>,
    opening: Vec<String>,
    unknown: BTreeSet<Vec<u8>>,
    shown_changed: bool,
    /// The lists ended since the menu last asked.
    lists_changed: bool,
    /// What tmux said of the window shown, for the notice of the menu.
    info: Option<Info>,
    /// How large the screen of a pane may be.
    limit: (usize, usize),
    /// The area the panes are shown in: the size of a pane whose layout has
    /// not come.
    area: (usize, usize),
    /// The window shown whose pane the lists were asked for.
    asked_for: Option<WindowId>,
    /// How a terminal of a pane is made.
    config: PaneConfig,
    /// The windows calling: a bell or a notification of a pane not shown.
    marks: Marks,
    /// tmux paused the pane shown and output was thrown away: `RX` is red
    /// until the screen is back under a second behind.
    lost: bool,
    /// `select-window` commands on their way.
    selects: usize,
}

/// tmux in control mode on the line of a session.
pub struct Control {
    client: Client,
    state: State,
    /// Commands for the line, written by the session after each call.
    out: Vec<u8>,
    /// Input held while a `send-keys` awaits its reply, or while leaving.
    keys: Held,
    /// The size this client asked for last.
    size: (usize, usize),
    /// The line was lost and has not come back.
    line_gone: bool,
    /// The `pause-after` wanted, from `tmux.pause`.
    pause: Option<u32>,
    /// The `pause-after` tmux was last told; none, tmux's own default.
    pause_told: Option<u32>,
}

impl Control {
    /// The mode, from its first line on. `lossy` on a port, where a byte may
    /// be lost; `size` is the area the panes are shown in.
    pub fn new(lossy: bool, config: PaneConfig, size: (usize, usize), read_buffer: usize) -> Self {
        let mut client = Client::new(lossy);
        client.fit(read_buffer);
        Self {
            client,
            state: State {
                model: Model::default(),
                panes: BTreeMap::new(),
                shown: None,
                wanted: None,
                version: None,
                phase: Phase::Control,
                todo: Vec::new(),
                told: Vec::new(),
                opening: Vec::new(),
                unknown: BTreeSet::new(),
                shown_changed: false,
                lists_changed: false,
                info: None,
                limit: limit_of(size, config.overflow),
                area: size,
                marks: Marks::default(),
                lost: false,
                selects: 0,
                asked_for: None,
                config,
            },
            out: Vec::new(),
            keys: Held::default(),
            size,
            line_gone: false,
            pause: None,
            pause_told: None,
        }
    }

    /// Reads a chunk of the protocol, decoding it where it stands.
    pub fn feed(&mut self, bytes: &mut [u8]) {
        if let Err(error) = self.client.feed(bytes, &mut self.state) {
            log::debug!("tmux: {error}");
        }
        for pane in self.state.panes.values_mut() {
            pane.flush();
        }
        self.act();
    }

    /// Brings the line held across chunks to the read buffer, as the buffers
    /// of the line are brought to it.
    pub fn fit(&mut self, bytes: usize) {
        self.client.fit(bytes);
    }

    /// Commands for the line since the last call.
    pub fn take_out(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// What the mode has to say on the desktop since the last call.
    pub fn take_told(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.told)
    }

    /// True once since the terminal shown changed.
    pub fn take_shown_changed(&mut self) -> bool {
        std::mem::take(&mut self.state.shown_changed)
    }

    /// Where the mode stands.
    pub fn phase(&self) -> Phase {
        self.state.phase
    }

    /// Input typed into the pane shown: `send-keys`, one on the way at a time.
    pub fn input(&mut self, bytes: &[u8]) {
        self.keys.push_bytes(bytes);
        self.send_keys();
    }

    /// A key typed into the pane shown that tmux knows by `name`: tmux encodes
    /// it for the terminal of the pane. `bytes` are what this terminal encodes
    /// it as, for the line if the mode ends before it goes.
    pub fn key(&mut self, name: &str, bytes: &[u8]) {
        self.keys.push_named(name, bytes);
        self.send_keys();
    }

    /// The input held, which leaves with the mode: written to the line when
    /// leaving ends, since it was typed for the shell that comes back.
    pub fn take_keys(&mut self) -> Vec<u8> {
        self.keys.take_bytes()
    }

    /// Bytes of input held, for the button of tmux.
    pub fn keys_held(&self) -> usize {
        self.keys.len()
    }

    /// The area changed: this client asks tmux for its size, and the sizes of
    /// the panes come back with `%layout-change`.
    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.size = (columns, rows);
        self.apply_limit();
        if self.state.phase == Phase::Control && !self.line_gone && self.state.version.is_some() {
            self.client.send(client_size(self.size), &mut self.out);
        }
    }

    /// Leaves with `detach`: the session stays on its server.
    pub fn detach(&mut self) {
        if self.state.phase == Phase::Control && !self.line_gone {
            self.client.send(Command::Detach, &mut self.out);
        }
        self.state.phase = Phase::Leaving;
    }

    /// The port was lost: what was written into it may not have arrived, and a
    /// reply waited for that never comes would move every later one onto the
    /// wrong command. The panes stand as they are.
    pub fn line_lost(&mut self) {
        self.client.forget();
        self.line_gone = true;
        self.state.selects = 0;
        self.state.model.give_up_aim();
    }

    /// The port came back: the state is asked again, and the pane shown
    /// snapshotted anew, since what it missed did not reach it.
    pub fn line_back(&mut self) {
        if !std::mem::take(&mut self.line_gone) || self.state.phase != Phase::Control {
            return;
        }
        self.start();
        let loading: Vec<PaneId> = self
            .state
            .panes
            .iter()
            .filter(|(id, pane)| pane.state == PaneState::Loading || Some(**id) == self.state.shown)
            .map(|(id, _)| *id)
            .collect();
        for pane in loading {
            self.snapshot(pane);
        }
    }

    /// The terminal of the pane shown, once there is one.
    pub fn shown(&self) -> Option<&Terminal> {
        self.state
            .shown
            .and_then(|pane| self.state.panes.get(&pane))
            .map(pane::Pane::view)
    }

    /// The terminal of the pane shown, to change.
    pub fn shown_mut(&mut self) -> Option<&mut Terminal> {
        let shown = self.state.shown?;
        self.state.panes.get_mut(&shown).map(pane::Pane::view_mut)
    }

    /// Every terminal of a pane, to read.
    pub fn terminals(&self) -> impl Iterator<Item = &Terminal> {
        self.state.panes.values().map(|pane| &pane.terminal)
    }

    /// The terminals of the panes take this many lines of scrollback, and
    /// those made later as well.
    pub fn set_config(&mut self, config: PaneConfig) {
        self.state.config = config;
        self.apply_limit();
        for pane in self.state.panes.values_mut() {
            pane.terminal.set_clipboard_access(config.clipboard);
            pane.terminal.set_picture_access(config.pictures);
        }
    }

    /// The events of the terminals of the panes: all of the pane shown, and
    /// the commands of a shell of the others, which the history keeps
    /// whichever pane they ran in.
    pub fn take_events(&mut self) -> Vec<TerminalEvent> {
        let shown = self.state.shown;
        let mut events = Vec::new();
        let State {
            panes,
            model,
            marks,
            ..
        } = &mut self.state;
        for (id, pane) in panes {
            let taken = pane.terminal.take_events();
            if Some(*id) == shown {
                events.extend(taken);
                continue;
            }
            let window = model.window_of(*id);
            for event in taken {
                match event {
                    TerminalEvent::Command(_) => events.push(event),
                    TerminalEvent::Bell => {
                        if let Some(window) = window {
                            marks.bell(window);
                        }
                        events.push(event);
                    }
                    TerminalEvent::Notification { kind, title, body } => {
                        let named = window.and_then(|window| model.window(window));
                        if let Some(window) = window {
                            marks.note(window, &title, &body);
                        }
                        let title = match (named, title.is_empty()) {
                            (Some(named), true) => named.label(),
                            (Some(named), false) => format!("{}: {title}", named.label()),
                            (None, _) => title,
                        };
                        events.push(TerminalEvent::Notification { kind, title, body });
                    }
                    _ => {}
                }
            }
        }
        events
    }

    /// What the button of tmux names: the window shown.
    /// How a window calls: its last notification before a bell, since a
    /// notification has a text; tmux's flag of a bell when nothing was heard
    /// here.
    fn calling_of(&self, window: WindowId) -> Calling {
        let mark = self.state.marks.get(window);
        if let Some(note) = mark.and_then(|mark| mark.note.as_ref()) {
            return Calling::Note {
                title: note.title.clone(),
                body: note.body.clone(),
                since: note.at.elapsed(),
            };
        }
        let rang = mark.is_some_and(|mark| mark.bell)
            || self
                .state
                .model
                .window(window)
                .is_some_and(|entry| entry.bell);
        match rang {
            true => Calling::Bell,
            false => Calling::None,
        }
    }

    /// How many windows of the session shown call: the count on the button.
    pub fn calling(&self) -> usize {
        let Some(session) = self.state.model.session else {
            return 0;
        };
        self.state
            .model
            .windows_of(session)
            .into_iter()
            .filter(|(window, _)| self.calling_of(*window) != Calling::None)
            .count()
    }

    /// A window of several panes is named by the pane shown: `1.1 bash`.
    pub fn label(&self) -> Option<String> {
        let window = self.state.model.shown_window()?;
        let entry = self.state.model.window(window)?;
        match self.state.shown {
            Some(pane) if entry.panes().len() > 1 && entry.panes().contains(&pane) => {
                Some(entry.pane_label(pane))
            }
            _ => Some(entry.label()),
        }
    }

    /// What the debug window says of the mode.
    pub fn report(&self) -> Report {
        let window = self
            .state
            .model
            .shown_window()
            .and_then(|window| self.state.model.window(window));
        Report {
            version: self.state.version.clone(),
            phase: self.state.phase,
            session: self.state.model.session_name().map(str::to_string),
            window_size: window
                .and_then(|window| window.layout.as_ref())
                .map(|layout| (layout.columns as usize, layout.rows as usize)),
            shown: self.state.shown,
            pane_size: self.shown().map(Terminal::size),
            panes: self.state.panes.len(),
            awaiting: self.client.sent().len(),
            keys_held: self.keys.len(),
        }
    }

    /// Does what the replies of the chunk asked for, now that the client may
    /// send.
    fn act(&mut self) {
        for todo in std::mem::take(&mut self.state.todo) {
            if self.state.phase != Phase::Control || self.line_gone {
                break;
            }
            match todo {
                Todo::AskVersion => self.client.send(Command::Version, &mut self.out),
                Todo::Start => self.start(),
                Todo::Show => self.show(),
                Todo::Snapshot(pane) => self.snapshot(pane),
                Todo::Detach => self.detach(),
                Todo::DetachNested(pane) => {
                    self.client.send_keys(pane, b"detach\n", &mut self.out);
                }
                Todo::Keys => {}
                Todo::Lists => self.ask_lists(&[]),
                Todo::Resume(pane) => self.resume(pane),
                Todo::Deep(pane) => self.deep_snapshot(pane),
            }
        }
        self.send_keys();
    }

    /// The screen of a pane is held to the area and the margin past it, which
    /// move with the area and the settings.
    fn apply_limit(&mut self) {
        let limit = limit_of(self.size, self.state.config.overflow);
        self.state.limit = limit;
        self.state.area = self.size;
        for pane in self.state.panes.values_mut() {
            pane.set_limit(limit);
        }
    }

    /// A choice of the menu of tmux, written as the commands that do it. A
    /// window of another session goes with `switch-client` first, in one line,
    /// since `select-window` alone selects it in a session this client is not
    /// on.
    pub fn choose(&mut self, choice: Choice) {
        if self.state.phase != Phase::Control || self.line_gone {
            return;
        }
        match choice {
            Choice::Window { session, window } if Some(session) == self.state.model.session => {
                self.state.model.aim(window);
                self.select_window(window);
                self.show();
            }
            Choice::Window { session, window } => {
                self.state.selects += 1;
                self.client.send_line(
                    &[
                        Command::SwitchClient(session),
                        Command::SelectWindow(window),
                    ],
                    &mut self.out,
                );
            }
            Choice::Session(session) => {
                self.client
                    .send(Command::SwitchClient(session), &mut self.out);
            }
            Choice::NewWindow => {
                if let Some(session) = self.state.model.session {
                    self.client.send(Command::NewWindow(session), &mut self.out);
                }
            }
            Choice::CloseWindow(window) => {
                self.client.send(Command::KillWindow(window), &mut self.out);
            }
            Choice::Pane {
                session,
                window,
                pane,
            } => {
                if !self.state.model.choose_pane(window, pane) {
                    return;
                }
                let here = Some(session) == self.state.model.session
                    && Some(window) == self.state.model.shown_window();
                match here {
                    true => self.show(),
                    false => self.choose(Choice::Window { session, window }),
                }
            }
            Choice::RenameWindow { window, name } => {
                self.client.rename_window(window, &name, &mut self.out);
            }
            Choice::Detach => self.detach(),
        }
    }

    /// `select-window`, counted while on its way: a window tmux says is
    /// current meanwhile is what it did before, and the window chosen ahead
    /// stands.
    fn select_window(&mut self, window: WindowId) {
        self.state.selects += 1;
        self.client
            .send(Command::SelectWindow(window), &mut self.out);
    }

    /// The menu opens on what is kept, and the lists and the notice are asked
    /// again; their replies refill it.
    pub fn refresh(&mut self) {
        if self.state.phase == Phase::Control && !self.line_gone && self.state.version.is_some() {
            self.ask_lists(&[Command::WindowInfo]);
        }
    }

    fn ask_lists(&mut self, after: &[Command]) {
        self.state.model.listing();
        let mut commands = vec![
            Command::ListSessions,
            Command::ListWindows,
            Command::ListPanes,
        ];
        commands.extend_from_slice(after);
        self.client.send_line(&commands, &mut self.out);
    }

    /// True once since the lists ended: the menu, if it stands, is refilled.
    pub fn take_lists_changed(&mut self) -> bool {
        std::mem::take(&mut self.state.lists_changed)
    }

    /// Whether the pane shown is not its screen yet: it is shown as it is,
    /// under a plate that says so, until its snapshot ends.
    pub fn loading(&self) -> bool {
        self.state
            .shown
            .and_then(|pane| self.state.panes.get(&pane))
            .is_some_and(Pane::waits_for_screen)
    }

    /// What the menu of tmux is made from.
    pub fn menu(&self) -> Menu {
        let model = &self.state.model;
        let session_of = |id: SessionId, name: &str| {
            let windows: Vec<MenuWindow> = model
                .windows_of(id)
                .into_iter()
                .map(|(window, entry)| {
                    let panes = entry.panes();
                    let pane_list = match panes.len() {
                        0 | 1 => Vec::new(),
                        _ => {
                            let mut list: Vec<MenuPane> = panes
                                .iter()
                                .map(|pane| MenuPane {
                                    id: *pane,
                                    label: entry.pane_label(*pane),
                                })
                                .collect();
                            list.sort_by_key(|pane| {
                                entry
                                    .listed
                                    .iter()
                                    .find(|listed| listed.id == pane.id)
                                    .map_or(u32::MAX, |listed| listed.index)
                            });
                            list
                        }
                    };
                    MenuWindow {
                        id: window,
                        label: entry.label(),
                        name: entry.name.clone(),
                        panes: panes.len().max(1),
                        pane_list,
                        current: model.current_of(id) == Some(window),
                        calling: self.calling_of(window),
                    }
                })
                .collect();
            MenuSession {
                id,
                name: name.to_string(),
                calling: windows.iter().any(|window| window.calling != Calling::None),
                windows,
            }
        };
        let shown = model.session.and_then(|session| {
            model
                .sessions()
                .find(|(id, _)| *id == session)
                .map(|(id, name)| session_of(id, name))
        });
        let others = model
            .sessions()
            .filter(|(id, _)| Some(*id) != model.session)
            .map(|(id, name)| session_of(id, name))
            .collect();
        Menu {
            shown,
            others,
            window: model.shown_window(),
            pane: self.state.shown,
            more: model.more(),
            info: self.state.info.clone(),
        }
    }

    /// The size of this client and the lists, after the version.
    fn start(&mut self) {
        let mut commands = vec![
            client_size(self.size),
            Command::ListSessions,
            Command::ListWindows,
            Command::ListPanes,
        ];
        if self.state.serves(SUBSCRIPTIONS) {
            commands.push(Command::Bells);
            if self.pause.is_some() {
                commands.push(Command::PauseAfter(self.pause));
                self.pause_told = self.pause;
            }
        }
        self.client.send_line(&commands, &mut self.out);
    }

    /// Shows the pane the model says is active in the window shown, at once:
    /// a pane without its screen yet is shown as it is, and snapshotted. The
    /// first pane of the mode waits for its snapshot, the plain terminal
    /// standing until then.
    fn show(&mut self) {
        let Some(wanted) = self.state.model.pane_to_show() else {
            self.ask_for_the_pane();
            return;
        };
        self.state.wanted = Some(wanted);
        let paused = self
            .state
            .panes
            .get(&wanted)
            .is_some_and(|pane| pane.paused.is_some());
        let state = self.state.panes.get(&wanted).map(|pane| pane.state);
        if paused {
            self.resume(wanted);
        } else if let Some(PaneState::Unseen) | None = state {
            self.snapshot(wanted);
        }
        if state == Some(PaneState::Fresh) || self.state.shown.is_some() {
            self.state.show(wanted);
        }
    }

    /// A window shown whose pane nobody named: `new-window` is answered by
    /// `%session-window-changed` and `%window-add` alone when the size did not
    /// change, so the lists are asked, once for that window, and their end
    /// shows its pane.
    fn ask_for_the_pane(&mut self) {
        let Some(window) = self.state.model.shown_window() else {
            return;
        };
        if self.state.asked_for == Some(window) {
            return;
        }
        self.state.asked_for = Some(window);
        self.ask_lists(&[]);
    }

    /// Asks for the screens of a pane on a terminal made anew: the modes, the
    /// screen under the alternate one, the visible one, in one line so they
    /// agree with each other.
    fn snapshot(&mut self, id: PaneId) {
        let (columns, rows) = self.state.size_of(id).unwrap_or(self.size);
        let Ok(terminal) = self.state.make_terminal(columns, rows) else {
            return;
        };
        let pane = match self.state.panes.entry(id) {
            std::collections::btree_map::Entry::Occupied(occupied) => {
                let pane = occupied.into_mut();
                pane.begin_snapshot(terminal);
                pane
            }
            std::collections::btree_map::Entry::Vacant(vacant) => {
                vacant.insert(Pane::loading(terminal, self.state.limit))
            }
        };
        pane.tries += 1;
        self.client.send_line(
            &[
                Command::Display(id),
                Command::Capture {
                    pane: id,
                    part: Part::Saved,
                },
                Command::Capture {
                    pane: id,
                    part: Part::Visible,
                },
            ],
            &mut self.out,
        );
    }

    /// One `send-keys` on the way: what is typed while one awaits its `%end`
    /// is held and goes with the next.
    fn send_keys(&mut self) {
        if self.keys.is_empty()
            || self.state.phase != Phase::Control
            || self.line_gone
            || self
                .client
                .sent()
                .any(|command| matches!(command, Command::SendKeys(_)))
        {
            return;
        }
        let Some(pane) = self
            .state
            .shown
            .filter(|pane| self.state.panes.contains_key(pane))
        else {
            return;
        };
        let taken = self.keys.take(PIECE);
        let keys: Vec<zyt_tmux::Keys<'_>> = taken.iter().map(input::Taken::keys).collect();
        self.client.send_input(pane, &keys, &mut self.out);
    }
}

/// How large a screen may be: the area and the margin past it, each side apart.
fn limit_of(area: (usize, usize), overflow: (usize, usize)) -> (usize, usize) {
    (
        area.0.saturating_add(overflow.0),
        area.1.saturating_add(overflow.1),
    )
}

fn client_size((columns, rows): (usize, usize)) -> Command {
    Command::ClientSize {
        columns: u16::try_from(columns).unwrap_or(u16::MAX),
        rows: u16::try_from(rows).unwrap_or(u16::MAX),
    }
}

/// What the debug window says of the mode.
#[derive(Debug, Clone)]
pub struct Report {
    /// The version of tmux.
    pub version: Option<String>,
    /// Where the mode stands.
    pub phase: Phase,
    /// The session this client is on.
    pub session: Option<String>,
    /// The size tmux gave the window shown.
    pub window_size: Option<(usize, usize)>,
    /// The pane shown.
    pub shown: Option<PaneId>,
    /// Its size.
    pub pane_size: Option<(usize, usize)>,
    /// Panes with a terminal.
    pub panes: usize,
    /// Commands awaiting their reply.
    pub awaiting: usize,
    /// Bytes of input held.
    pub keys_held: usize,
}

impl State {
    /// Whether the tmux on the other end is `version` or later.
    fn serves(&self, version: (u32, u32)) -> bool {
        self.version
            .as_deref()
            .and_then(version_number)
            .is_some_and(|number| number >= version)
    }

    fn make_terminal(&self, columns: usize, rows: usize) -> crate::error::Result<Terminal> {
        let mut terminal = crate::session::new_terminal(zyt_term::TerminalConfig {
            columns: columns.max(2),
            rows: rows.max(1),
            scrollback: self.config.scrollback,
            clipboard: self.config.clipboard,
        })?;
        terminal.set_picture_access(self.config.pictures);
        Ok(terminal)
    }

    /// The size the layout of its window gives a pane, once it is known.
    fn size_of(&self, pane: PaneId) -> Option<(usize, usize)> {
        self.model
            .window_of(pane)
            .and_then(|window| self.model.window(window))
            .and_then(|window| window.cell(pane))
            .and_then(pane_size)
    }

    /// The pane `%output` names, a terminal made for it when it has none:
    /// what a hidden pane says is read as it comes. Only a pane of a window
    /// the model knows is made, so how many there are is held by the lists,
    /// not by the line; the output of one the lists have not named yet is
    /// dropped. Its size is the one its layout gives, the area before one came.
    fn pane_for_output(&mut self, pane: PaneId) -> Option<&mut Pane> {
        if !self.panes.contains_key(&pane) {
            self.model.window_of(pane)?;
            let (columns, rows) = self.size_of(pane).unwrap_or(self.area);
            let terminal = self
                .make_terminal(columns, rows)
                .inspect_err(|error| log::debug!("tmux: a terminal for {pane}: {error}"))
                .ok()?;
            self.panes.insert(pane, Pane::unseen(terminal, self.limit));
        }
        self.panes.get_mut(&pane)
    }

    /// Shows a pane. The terminals of panes of another session go: they get
    /// no `%output` while this client is away, so shown again they are rebuilt
    /// from a snapshot and a terminal kept for them holds only memory.
    fn show(&mut self, pane: PaneId) {
        if self.shown != Some(pane) {
            self.shown = Some(pane);
            self.shown_changed = true;
        }
        if let Some(window) = self.model.window_of(pane) {
            self.marks.clear(window);
        }
        let State {
            panes,
            model,
            shown,
            ..
        } = self;
        let session = model.session;
        panes.retain(|id, _| {
            Some(*id) == *shown || model.session_of(*id).is_none_or(|of| Some(of) == session)
        });
    }

    /// A pane closed in tmux: its terminal and everything of it go.
    fn close(&mut self, pane: PaneId) {
        self.panes.remove(&pane);
        if self.shown == Some(pane) {
            self.shown = None;
            self.shown_changed = true;
            self.todo.push(Todo::Show);
        }
    }

    /// The version of tmux on the other end, from `display -p '#{version}'`.
    fn take_version(&mut self, line: &[u8]) {
        let text = String::from_utf8_lossy(line).trim().to_string();
        let served = version_number(&text).is_some_and(|number| number >= OLDEST);
        if served {
            self.todo.push(Todo::Start);
        } else {
            self.told
                .push(t!("tmux.too_old", version = text.as_str()).to_string());
            self.todo.push(Todo::Detach);
        }
        self.version = Some(text);
    }

    fn layout(&mut self, window: zyt_tmux::WindowId, layout: &[u8], visible: Option<&[u8]>) {
        let layout = match Layout::parse(layout) {
            Ok(layout) => layout,
            Err(error) => {
                log::debug!("tmux: the layout of {window}: {error}");
                return;
            }
        };
        let visible = visible.and_then(|visible| Layout::parse(visible).ok());
        for closed in self.model.layout(window, layout, visible) {
            self.close(closed);
        }
        let Some(entry) = self.model.window(window) else {
            return;
        };
        for (id, pane) in &mut self.panes {
            if let Some((columns, rows)) = entry.cell(*id).and_then(pane_size) {
                pane.resize(columns, rows);
            }
        }
        let shows_another = self
            .model
            .pane_to_show()
            .is_some_and(|pane| self.wanted != Some(pane));
        if self.model.shown_window() == Some(window) && self.version.is_some() && shows_another {
            self.todo.push(Todo::Show);
        }
    }

    /// The end of a snapshot: shown when it is the pane wanted, asked again
    /// when a part of it failed.
    fn snapshot_end(&mut self, id: PaneId, failed: bool) {
        let Some(pane) = self.panes.get_mut(&id) else {
            return;
        };
        if failed || pane.snapshot_failed {
            if pane.tries < SNAPSHOT_TRIES {
                self.todo.push(Todo::Snapshot(id));
            } else {
                log::debug!("tmux: the snapshot of {id} failed {SNAPSHOT_TRIES} times");
                self.panes.remove(&id);
            }
            return;
        }
        pane.visible_screen_end();
        pane.tries = 0;
        if self.wanted.is_none_or(|wanted| wanted == id) {
            self.show(id);
        }
    }
}

/// The reply of `Command::WindowInfo`: `3.3a 120x40 2`.
fn info_of(line: &[u8]) -> Option<Info> {
    let text = String::from_utf8_lossy(line);
    let mut words = text.split_whitespace();
    let info = Info {
        version: words.next()?.to_string(),
        size: words.next()?.to_string(),
        clients: words.next()?.to_string(),
    };
    Some(info)
}

/// `3.3a` as `(3, 3)`: the major and minor numbers, letters dropped.
fn version_number(text: &str) -> Option<(u32, u32)> {
    let text = text.strip_prefix("next-").unwrap_or(text);
    let (major, rest) = text.split_once('.')?;
    let minor: String = rest.chars().take_while(char::is_ascii_digit).collect();
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests;
