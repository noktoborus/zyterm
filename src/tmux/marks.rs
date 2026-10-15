//! The windows of tmux calling for the person: a bell or a notification of a
//! pane of a window not shown, kept until that window is shown.
//!
//! A mark is set from what the panes say as their `%output` is fed — tmux reads
//! no OSC 9 or 777 itself — and stands beside the bell flag tmux keeps of a
//! window (`windows::Window::bell`), which also covers a window of another
//! session and one rung before this client came.

use std::collections::BTreeMap;
use std::time::Instant;

use zyt_tmux::WindowId;

/// The last notification of a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Its heading, empty when the program gave none.
    pub title: String,
    /// Its text.
    pub body: String,
    /// When it came.
    pub at: Instant,
}

/// What a window said while it was not shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mark {
    /// A bell rang in it.
    pub bell: bool,
    /// Its last notification.
    pub note: Option<Note>,
}

/// The marks of the windows, by window.
#[derive(Debug, Default)]
pub struct Marks {
    windows: BTreeMap<WindowId, Mark>,
}

impl Marks {
    /// A bell of a pane of `window`.
    pub fn bell(&mut self, window: WindowId) {
        self.windows.entry(window).or_default().bell = true;
    }

    /// A notification of a pane of `window`: the last one stands.
    pub fn note(&mut self, window: WindowId, title: &str, body: &str) {
        self.windows.entry(window).or_default().note = Some(Note {
            title: title.to_string(),
            body: body.to_string(),
            at: Instant::now(),
        });
    }

    /// The window is shown: what it said has been seen.
    pub fn clear(&mut self, window: WindowId) {
        self.windows.remove(&window);
    }

    /// The mark of a window.
    pub fn get(&self, window: WindowId) -> Option<&Mark> {
        self.windows.get(&window)
    }

    /// Only the windows `known` holds keep their marks: a window closed takes
    /// its mark with it.
    pub fn retain(&mut self, known: impl Fn(WindowId) -> bool) {
        self.windows.retain(|window, _| known(*window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_stands_until_its_window_is_shown() {
        let mut marks = Marks::default();
        marks.bell(WindowId(1));
        marks.note(WindowId(1), "make", "done");
        marks.note(WindowId(1), "make", "failed");
        let mark = marks.get(WindowId(1)).expect("a mark");
        assert!(mark.bell);
        assert_eq!(mark.note.as_ref().unwrap().body, "failed", "the last one");
        marks.clear(WindowId(1));
        assert!(marks.get(WindowId(1)).is_none());
    }

    #[test]
    fn a_window_gone_takes_its_mark() {
        let mut marks = Marks::default();
        marks.bell(WindowId(1));
        marks.bell(WindowId(2));
        marks.retain(|window| window == WindowId(2));
        assert!(marks.get(WindowId(1)).is_none());
        assert!(marks.get(WindowId(2)).is_some());
    }
}
