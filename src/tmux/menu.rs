//! What the menu of tmux is made from: the model as it stands, and what an
//! entry chooses.

use std::time::Duration;

use zyt_tmux::{PaneId, SessionId, WindowId};

/// How a window calls for the person, while it is not shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Calling {
    /// It says nothing.
    None,
    /// A bell rang in it.
    Bell,
    /// Its last notification.
    Note {
        /// Its heading, empty when the program gave none.
        title: String,
        /// Its text.
        body: String,
        /// How long ago it came.
        since: Duration,
    },
}

/// A choice of the menu of tmux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// A window, of the session shown or of another: the client goes there.
    Window {
        /// The session it is in.
        session: SessionId,
        /// The window.
        window: WindowId,
    },
    /// Another session: its current window.
    Session(SessionId),
    /// A new window in the session shown.
    NewWindow,
    /// A window closed, its programs ended.
    CloseWindow(WindowId),
    /// A window of the session shown given another name.
    RenameWindow {
        /// The window.
        window: WindowId,
        /// Its new name.
        name: String,
    },
    /// A pane of a window: the client goes to the window, the pane is shown
    /// and tmux zooms it.
    Pane {
        /// The session it is in.
        session: SessionId,
        /// The window.
        window: WindowId,
        /// The pane.
        pane: PaneId,
    },
    /// Leave the mode; the session stays.
    Detach,
    /// A pane zoomed, or unzoomed when its window is zoomed on it.
    Zoom {
        /// The window.
        window: WindowId,
        /// The pane.
        pane: PaneId,
    },
    /// A pane closed, its program ended.
    ClosePane(PaneId),
    /// A pane split: a new pane beside it, or below it, made active.
    Split {
        /// The window.
        window: WindowId,
        /// The pane split.
        pane: PaneId,
        /// The panes side by side, or one above the other.
        side_by_side: bool,
    },
    /// The wheel over the button of tmux: the window this many on in the
    /// session shown, round past either end, is shown ahead of tmux.
    Step(i32),
    /// The wheel rested: tmux is told the window it rests on, and its pane
    /// is snapshotted when it has no screen.
    Settle,
}

/// What tmux says of the window shown, for the notice of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// The version of tmux.
    pub version: String,
    /// The size tmux gave the window: `120x40`.
    pub size: String,
    /// How many clients the session has.
    pub clients: String,
}

/// A window, as the menu names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuWindow {
    /// The window.
    pub id: WindowId,
    /// `index: name`.
    pub label: String,
    /// Its name alone.
    pub name: String,
    /// How many panes it has.
    pub panes: usize,
    /// Its panes, by their index, when it has several.
    pub pane_list: Vec<MenuPane>,
    /// The pane this client shows of it.
    pub pane: Option<PaneId>,
    /// The pane tmux zoomed it on.
    pub zoomed: Option<PaneId>,
    /// It is the window shown in its session.
    pub current: bool,
    /// How it calls.
    pub calling: Calling,
}

/// A pane, as the menu names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuPane {
    /// The pane.
    pub id: PaneId,
    /// `1.1 bash`.
    pub label: String,
}

/// A session, as the menu names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSession {
    /// The session.
    pub id: SessionId,
    /// Its name.
    pub name: String,
    /// Its windows, by their index.
    pub windows: Vec<MenuWindow>,
    /// A window of it calls.
    pub calling: bool,
}

/// What the menu of tmux is made from: the model as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    /// The session this client is on, first.
    pub shown: Option<MenuSession>,
    /// Every other session.
    pub others: Vec<MenuSession>,
    /// The window shown.
    pub window: Option<WindowId>,
    /// The pane shown.
    pub pane: Option<PaneId>,
    /// Sessions and windows past the limit, not kept.
    pub more: usize,
    /// What tmux said of the window shown when last asked.
    pub info: Option<Info>,
}
