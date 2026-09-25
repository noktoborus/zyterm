//! State of the search bar.
//!
//! The bar keeps what the user typed and which modes are switched on; the
//! terminal keeps the matches. Every change of either goes to the terminal at
//! once, so what is on the screen always answers what the bar shows.

use zyt_term::{SearchDirection, SearchKind, SearchOptions, Terminal};

/// What the search bar holds between frames.
#[derive(Debug, Default)]
pub struct SearchState {
    /// The bar stands in place of the status bar.
    pub open: bool,
    /// What the user typed.
    pub query: String,
    /// Which modes are switched on.
    pub options: SearchOptions,
    /// The query is not in the terminal, or is not a pattern.
    pub missed: bool,
    /// The field asks for the keyboard on the next frame. Only opening the bar
    /// asks for it: a step or a switch must not take it from a control the user
    /// walked to with Tab.
    pub focus_wanted: bool,
}

impl SearchState {
    /// Hands the query and the modes to the terminal and steps to the first
    /// match above the viewport.
    pub fn apply(&mut self, terminal: &mut Terminal) {
        if terminal.search_set(&self.query, self.options).is_err() {
            self.missed = true;
            return;
        }
        if self.query.is_empty() {
            self.missed = false;
            return;
        }
        self.missed = !terminal.search_advance(SearchDirection::Up);
    }

    /// Steps to the next match in the given direction.
    pub fn advance(&mut self, terminal: &mut Terminal, direction: SearchDirection) {
        self.missed = !terminal.search_advance(direction);
    }

    /// Opens the bar. A selection standing in the terminal is what the user
    /// wants to find, so it becomes the query; without one the bar keeps what
    /// it was left with.
    pub fn open(&mut self, terminal: &mut Terminal) {
        if let Some(selected) = selected_query(terminal) {
            self.query = selected;
        }
        self.open = true;
        self.focus_wanted = true;
        self.apply(terminal);
    }

    /// Closes the bar and takes every mark down.
    pub fn close(&mut self, terminal: &mut Terminal) {
        self.open = false;
        self.focus_wanted = false;
        self.missed = false;
        terminal.search_clear();
    }
}

/// Translation key of the name of a way of reading the query.
pub fn kind_key(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Literal => "search.kind_literal",
        SearchKind::Fuzzy => "search.kind_fuzzy",
        SearchKind::Word => "search.kind_word",
        SearchKind::Regex => "search.kind_regex",
    }
}

/// How a way of reading the query is named in a menu entry.
pub fn kind_slug(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Literal => "literal",
        SearchKind::Fuzzy => "fuzzy",
        SearchKind::Word => "word",
        SearchKind::Regex => "regex",
    }
}

/// The way of reading the query a menu entry named.
pub fn kind_of_slug(slug: &str) -> Option<SearchKind> {
    SearchKind::ALL
        .into_iter()
        .find(|kind| kind_slug(*kind) == slug)
}

/// The selection of the terminal as a query: its first line, and nothing when
/// it holds no more than blanks. A query cannot span two lines, because a line
/// break is nowhere in the grid.
fn selected_query(terminal: &Terminal) -> Option<String> {
    let selected = terminal.selected_text()?;
    let line = selected.lines().next().unwrap_or_default();
    if line.trim().is_empty() {
        return None;
    }
    Some(line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zyt_term::TerminalConfig;

    fn terminal() -> Terminal {
        let mut terminal = Terminal::new(TerminalConfig {
            columns: 20,
            rows: 4,
            scrollback: 100,
            clipboard: zyt_term::ClipboardAccess::CopyOnly,
        })
        .expect("terminal is created");
        terminal.feed(b"alpha Cat\r\nbeta concat\r\ngamma cat\r\ndelta\r\n");
        terminal
    }

    #[test]
    fn opening_the_bar_finds_the_first_match_above_the_viewport() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "cat".to_string(),
            ..SearchState::default()
        };

        search.open(&mut terminal);

        assert!(search.open);
        assert!(search.focus_wanted);
        assert!(!search.missed);
        assert_eq!(terminal.selected_text().as_deref(), Some("cat"));
    }

    #[test]
    fn a_query_that_is_nowhere_is_reported_as_missed() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "omega".to_string(),
            ..SearchState::default()
        };

        search.open(&mut terminal);

        assert!(search.missed);
        assert!(terminal.selected_text().is_none());
    }

    #[test]
    fn a_pattern_that_cannot_be_built_is_reported_as_missed() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "a(".to_string(),
            options: SearchOptions {
                kind: zyt_term::SearchKind::Regex,
                ..SearchOptions::default()
            },
            ..SearchState::default()
        };

        search.open(&mut terminal);

        assert!(search.missed);
    }

    #[test]
    fn an_empty_query_marks_nothing_and_misses_nothing() {
        let mut terminal = terminal();
        let mut search = SearchState::default();

        search.open(&mut terminal);

        assert!(!search.missed);
        assert!(terminal.selected_text().is_none());
    }

    #[test]
    fn stepping_both_ways_leaves_the_keyboard_where_it_is() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "cat".to_string(),
            ..SearchState::default()
        };
        search.open(&mut terminal);
        search.focus_wanted = false;

        search.advance(&mut terminal, SearchDirection::Up);
        assert!(!search.focus_wanted);
        assert!(!search.missed);
        search.advance(&mut terminal, SearchDirection::Down);
        assert!(!search.missed);
    }

    #[test]
    fn a_selection_of_the_terminal_becomes_the_query() {
        let mut terminal = terminal();
        terminal
            .selection_start(zyt_term::SelectionKind::Semantic, 6, 0)
            .expect("the position is in the grid");
        assert_eq!(terminal.selected_text().as_deref(), Some("concat"));

        let mut search = SearchState {
            query: "delta".to_string(),
            ..SearchState::default()
        };
        search.open(&mut terminal);

        assert_eq!(search.query, "concat");
        assert!(!search.missed);
    }

    #[test]
    fn a_bar_without_a_selection_keeps_the_query_it_was_left_with() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "cat".to_string(),
            ..SearchState::default()
        };

        search.open(&mut terminal);

        assert_eq!(search.query, "cat");
    }

    #[test]
    fn a_selection_of_blanks_is_no_query() {
        let mut terminal = terminal();
        terminal
            .selection_start(zyt_term::SelectionKind::Lines, 0, 3)
            .expect("the position is in the grid");

        let mut search = SearchState {
            query: "cat".to_string(),
            ..SearchState::default()
        };
        search.open(&mut terminal);

        assert_eq!(search.query, "cat");
    }

    #[test]
    fn closing_the_bar_takes_every_mark_down() {
        let mut terminal = terminal();
        let mut search = SearchState {
            query: "cat".to_string(),
            ..SearchState::default()
        };
        search.open(&mut terminal);

        search.close(&mut terminal);

        assert!(!search.open);
        assert!(!search.focus_wanted);
        assert!(terminal.selected_text().is_none());
        assert_eq!(search.query, "cat");
    }
}
