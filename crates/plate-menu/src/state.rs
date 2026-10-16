//! What the menu remembers between frames.
//!
//! The state is a value of its own, without a toolkit in it, so walking the
//! tree, searching it and moving the selection are tested without a window.

use crate::item::MenuItem;
use crate::search::{item_at, level, path_of, search};

/// One line of the menu as it is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Indices leading to the entry.
    pub path: Vec<usize>,
    /// What the caller is told when the entry is chosen.
    pub id: String,
    /// Text of the entry.
    pub label: String,
    /// Labels of the entries above it, shown while searching.
    pub parents: String,
    /// Characters of `parents` the query matched, by index.
    pub matched_parents: Vec<usize>,
    /// Characters of the label the query matched, by index.
    pub matched_label: Vec<usize>,
    /// Text shown right of the label.
    pub detail: String,
    /// Text shown while the pointer rests on the row.
    pub hint: String,
    /// The whole of what the label was cut from, empty when the label is the
    /// whole of it.
    pub full: String,
    /// False draws the row weak and refuses to choose it.
    pub enabled: bool,
    /// True when the entry carries entries of its own.
    pub has_children: bool,
    /// True when the entry is chosen rather than stepped into, although it
    /// carries entries of its own.
    pub choosable: bool,
    /// True for a line between entries.
    pub separator: bool,
    /// True for the row that leads back to the level above.
    pub back: bool,
}

impl Row {
    /// True while the row can be chosen or walked into.
    pub fn is_reachable(&self) -> bool {
        self.back || (!self.separator && (self.enabled || self.has_children))
    }

    /// True for an entry that is chosen from its label and stepped into from a
    /// zone of its own.
    pub fn is_split(&self) -> bool {
        self.has_children && self.choosable && !self.back
    }
}

/// Position in the menu: the level, the selected row and the query.
#[derive(Debug, Default, Clone)]
pub struct MenuState {
    items: Vec<MenuItem>,
    path: Vec<usize>,
    selected: usize,
    query: String,
    caret: usize,
}

impl MenuState {
    /// State showing the first level of these entries.
    pub fn new(items: Vec<MenuItem>) -> Self {
        let mut state = Self {
            items,
            path: Vec::new(),
            selected: 0,
            query: String::new(),
            caret: 0,
        };
        state.select_first();
        state
    }

    /// Entries the menu was opened with.
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// Puts other entries in place of these, with the position kept.
    ///
    /// It is what a list that is read again while it stands needs: the ports of
    /// a machine are looked up once a second, and a menu that started over on
    /// every look would take the query, the level and the selection with it.
    /// The selection lands on the entry it stood on when that entry is still
    /// there, and on the first one when it is gone; a level that no longer
    /// exists is left for the one above it.
    pub fn refill(&mut self, items: Vec<MenuItem>) {
        let standing = self
            .rows()
            .get(self.selected)
            .map(|row| row.id.clone())
            .unwrap_or_default();

        self.items = items;
        while !self.path.is_empty() && item_at(&self.items, &self.path).is_none() {
            self.path.pop();
        }

        self.select_first();
        if !standing.is_empty() {
            self.select_id(&standing);
        }
    }

    /// Text typed so far.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Where typed text goes into the query, in characters from its start.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// True while a query is typed and nothing answers it.
    pub fn nothing_found(&self) -> bool {
        !self.query.trim().is_empty() && search(&self.items, &self.query).is_empty()
    }

    /// Labels of the levels walked into, for the line above the entries.
    pub fn trail(&self) -> Vec<String> {
        let mut trail = Vec::new();
        let mut path = Vec::new();
        for index in &self.path {
            path.push(*index);
            if let Some(item) = item_at(&self.items, &path) {
                trail.push(item.label.clone());
            }
        }
        trail
    }

    /// Row the selection is on.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The rows as they are shown: the current level, or the hits of the query.
    ///
    /// Inside a level, the first row names the level it was reached from and
    /// leads back to it.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(label) = self.trail().last() {
            rows.push(back_row(label));
        }

        if self.query.trim().is_empty() {
            rows.extend(
                level(&self.items, &self.path)
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let mut path = self.path.clone();
                        path.push(index);
                        row(item, path, String::new())
                    }),
            );
            return rows;
        }

        rows.extend(
            search(&self.items, &self.query)
                .into_iter()
                .filter_map(|hit| {
                    let item = item_at(&self.items, &hit.path)?;
                    let mut found = row(item, hit.path, hit.parents);
                    found.matched_parents = hit.matched_parents;
                    found.matched_label = hit.matched_label;
                    Some(found)
                }),
        );
        rows
    }

    /// Moves the selection, skipping what cannot be chosen and wrapping at both
    /// ends.
    pub fn move_selection(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            self.selected = 0;
            return;
        }

        let count = rows.len() as isize;
        let mut index = self.selected as isize;
        for _ in 0..count {
            index = (index + delta).rem_euclid(count);
            if rows[index as usize].is_reachable() {
                self.selected = index as usize;
                return;
            }
        }
    }

    /// Moves the selection by `page` rows down, or up for a negative one,
    /// without wrapping: it stops at the first or the last entry.
    ///
    /// It lands on the reachable entry nearest to where the step ends, on the
    /// near side of it first, so a line or a disabled entry there does not
    /// take the page back to where it started.
    pub fn move_page(&mut self, page: isize) {
        let rows = self.rows();
        let entries: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_reachable() && !row.back)
            .map(|(index, _)| index)
            .collect();
        let (Some(&first), Some(&last)) = (entries.first(), entries.last()) else {
            return;
        };

        let target = (self.selected as isize + page).clamp(first as isize, last as isize) as usize;
        let landed = match page < 0 {
            true => entries.iter().copied().find(|&index| index >= target),
            false => entries.iter().copied().rev().find(|&index| index <= target),
        };
        if let Some(index) = landed {
            self.selected = index;
        }
    }

    /// Puts the selection on the last entry.
    pub fn select_last(&mut self) {
        let rows = self.rows();
        if let Some(index) = rows.iter().rposition(|row| row.is_reachable() && !row.back) {
            self.selected = index;
        }
    }

    /// Keeps the selection inside the rows from `first` up to `last`, which are
    /// the ones shown: a selection the window left lands on the reachable row
    /// shown nearest to it. One standing among them is left where it is.
    pub fn select_shown(&mut self, first: usize, last: usize) {
        if (first..last).contains(&self.selected) {
            return;
        }
        let rows = self.rows();
        let shown = || {
            rows.iter()
                .enumerate()
                .take(last)
                .skip(first)
                .filter(|(_, row)| row.is_reachable())
                .map(|(index, _)| index)
        };
        let nearest = match self.selected < first {
            true => shown().next(),
            false => shown().next_back(),
        };
        if let Some(index) = nearest {
            self.selected = index;
        }
    }

    /// Steps into the entries of the selected row. True when it had any.
    ///
    /// The entry that was stepped into says where the selection lands, so a
    /// level that is a list of values opens on the value in use.
    pub fn step_in(&mut self) -> bool {
        let Some(row) = self.rows().get(self.selected).cloned() else {
            return false;
        };
        if !row.has_children {
            return false;
        }

        let opens_at = item_at(&self.items, &row.path)
            .map(|item| item.opens_at.clone())
            .unwrap_or_default();

        self.query.clear();
        self.caret = 0;
        self.path = row.path;
        self.select_first();
        if !opens_at.is_empty() {
            self.select_id(&opens_at);
        }
        true
    }

    /// Steps back out of the current level, or clears the query at the top.
    /// True when something changed.
    pub fn step_out(&mut self) -> bool {
        if self.clear_query() {
            return true;
        }
        if self.path.pop().is_none() {
            return false;
        }
        self.select_first();
        true
    }

    /// What `Enter` does: a step into the entries of the selected entry, the
    /// identifier of an entry that has none, or a step back when the row is the
    /// one naming the level above.
    ///
    /// An entry that carries entries is a way further in and not a value, even
    /// when it has an identifier of its own: the identifier names the level for
    /// whoever builds the menu, and choosing the entry would leave the menu
    /// closed and the entries under it unseen.
    pub fn accept(&mut self) -> Option<String> {
        let row = self.rows().get(self.selected).cloned()?;
        if row.back {
            self.step_out();
            return None;
        }
        if row.has_children && !row.choosable {
            self.step_in();
            return None;
        }
        if row.enabled && !row.id.is_empty() {
            return Some(row.id);
        }
        None
    }

    /// Chooses the row at this index, the way a click does.
    pub fn choose(&mut self, index: usize) -> Option<String> {
        let row = self.rows().get(index).cloned()?;
        if !row.is_reachable() {
            return None;
        }
        self.selected = index;
        self.accept()
    }

    /// Steps into the entries of the row at this index, the way a click on
    /// the zone of a split entry does. True when it had any.
    pub fn step_into(&mut self, index: usize) -> bool {
        let reachable = self.rows().get(index).is_some_and(Row::is_reachable);
        if !reachable {
            return false;
        }
        self.selected = index;
        self.step_in()
    }

    /// Puts typed text into the query at the caret, and the caret after it.
    pub fn push_query(&mut self, text: &str) {
        let at = self.caret_byte();
        self.query.insert_str(at, text);
        self.caret += text.chars().count();
        self.select_first();
    }

    /// What `Backspace` does to the query. True while there was one.
    ///
    /// A query something answers loses the character before the caret. One
    /// nothing answers is cut back to its longest beginning that something
    /// does, or to nothing: what was typed past the last answer — a word in the
    /// wrong keyboard layout, a slip of the finger — goes in one press, and the
    /// part that still finds something stays.
    pub fn pop_query(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }

        if self.nothing_found() {
            let characters: Vec<char> = self.query.chars().collect();
            let kept = (0..characters.len())
                .rev()
                .find(|&count| {
                    let prefix: String = characters[..count].iter().collect();
                    prefix.trim().is_empty() || !search(&self.items, &prefix).is_empty()
                })
                .unwrap_or(0);
            self.query = characters[..kept].iter().collect();
            self.caret = kept;
        } else if self.caret > 0 {
            self.caret -= 1;
            let at = self.caret_byte();
            self.query.remove(at);
        }

        if self.query.trim().is_empty() {
            self.query.clear();
            self.caret = 0;
        }
        self.select_first();
        true
    }

    /// Clears the query and shows the level it was typed over. True when
    /// there was one.
    pub fn clear_query(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }
        self.query.clear();
        self.caret = 0;
        self.select_first();
        true
    }

    /// Puts the caret at the start of the query.
    pub fn caret_home(&mut self) {
        self.caret = 0;
    }

    /// Puts the caret at the end of the query.
    pub fn caret_end(&mut self) {
        self.caret = self.query.chars().count();
    }

    /// Byte offset of the caret in the query.
    fn caret_byte(&self) -> usize {
        self.query
            .char_indices()
            .nth(self.caret)
            .map_or(self.query.len(), |(at, _)| at)
    }

    /// Puts the selection on the entry with this identifier.
    ///
    /// Only the level that is shown is looked through, and only entries that
    /// can be chosen: a menu of values opens on the value in use, and an
    /// identifier none of them carries leaves the selection where it was.
    /// True when it was found.
    pub fn select_id(&mut self, id: &str) -> bool {
        let Some(index) = self
            .rows()
            .iter()
            .position(|row| row.id == id && row.is_reachable() && !row.back)
        else {
            return false;
        };
        self.selected = index;
        true
    }

    /// Puts the selection on the entry with this identifier wherever in the tree
    /// it stands, stepping into the levels that carry it.
    ///
    /// It is what a menu opened on a sub-entry needs: a caller that names one
    /// value out of a tree of lines gets the level that holds it, with that
    /// value under the cursor and the level above it one step back. The level
    /// that is shown is looked through first, so an identifier standing in it is
    /// never searched for anywhere else, and a level nothing can be selected in
    /// is left for the one that was shown. True when it was found.
    pub fn select_deep(&mut self, id: &str) -> bool {
        if self.select_id(id) {
            return true;
        }
        let Some(path) = path_of(&self.items, id) else {
            return false;
        };

        let standing = std::mem::replace(&mut self.path, path[..path.len() - 1].to_vec());
        if self.select_id(id) {
            return true;
        }
        self.path = standing;
        false
    }

    /// Puts the selection on the first entry, never on the row that leads back.
    pub fn select_first(&mut self) {
        let rows = self.rows();
        self.selected = rows
            .iter()
            .position(|row| row.is_reachable() && !row.back)
            .or_else(|| rows.iter().position(Row::is_reachable))
            .unwrap_or_default();
    }
}

/// The row that names the level above and leads back to it.
fn back_row(label: &str) -> Row {
    Row {
        path: Vec::new(),
        id: String::new(),
        label: format!("{} {label}", crate::view::BACK),
        parents: String::new(),
        matched_parents: Vec::new(),
        matched_label: Vec::new(),
        detail: String::new(),
        hint: String::new(),
        full: String::new(),
        enabled: true,
        has_children: false,
        choosable: false,
        separator: false,
        back: true,
    }
}

fn row(item: &MenuItem, path: Vec<usize>, parents: String) -> Row {
    Row {
        path,
        id: item.id.clone(),
        label: item.label.clone(),
        parents,
        matched_parents: Vec::new(),
        matched_label: Vec::new(),
        detail: item.detail.clone(),
        hint: item.hint.clone(),
        full: item.full.clone(),
        enabled: item.enabled,
        has_children: item.has_children(),
        choosable: item.choosable,
        separator: item.separator,
        back: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Vec<MenuItem> {
        vec![
            MenuItem::new("copy", "Copy")
                .detail("ctrl+shift+c")
                .full("Copy the selection"),
            MenuItem::new("paste", "Paste").enabled(false),
            MenuItem::separator(),
            MenuItem::new("profile", "Cat file")
                .enabled(false)
                .children(vec![
                    MenuItem::new("profile.zmodem", "zmodem"),
                    MenuItem::new("profile.xmodem", "xmodem"),
                ]),
        ]
    }

    #[test]
    fn an_entry_that_carries_entries_is_stepped_into_and_not_chosen() {
        let mut state = MenuState::new(vec![MenuItem::new("profile", "Cat file").children(vec![
            MenuItem::new("profile.zmodem", "zmodem"),
            MenuItem::new("profile.xmodem", "xmodem"),
        ])]);

        assert_eq!(state.choose(0), None, "choosing it chooses nothing");
        assert_eq!(state.trail(), vec!["Cat file".to_string()], "it stepped in");
        assert_eq!(state.rows()[1].id, "profile.zmodem");
    }

    #[test]
    fn a_level_opens_on_the_entry_its_parent_names() {
        let mut state = MenuState::new(vec![
            MenuItem::new("parity", "Parity")
                .opens_at("parity.odd")
                .children(vec![
                    MenuItem::new("parity.none", "None"),
                    MenuItem::new("parity.even", "Even"),
                    MenuItem::new("parity.odd", "Odd"),
                ]),
        ]);

        state.step_in();

        let rows = state.rows();
        assert_eq!(rows[state.selected()].id, "parity.odd");
    }

    #[test]
    fn a_level_whose_parent_names_nothing_opens_on_its_first_entry() {
        let mut state = MenuState::new(vec![MenuItem::new("parity", "Parity").children(vec![
            MenuItem::new("parity.none", "None"),
            MenuItem::new("parity.even", "Even"),
        ])]);

        state.step_in();

        let rows = state.rows();
        assert_eq!(rows[state.selected()].id, "parity.none");
    }

    #[test]
    fn the_selection_is_put_on_the_entry_that_is_asked_for() {
        let mut state = MenuState::new(menu());

        assert!(state.select_id("profile"), "the entry is found");
        assert_eq!(state.selected(), 3);
    }

    #[test]
    fn a_sub_entry_is_selected_in_the_level_that_carries_it() {
        let mut state = MenuState::new(vec![
            MenuItem::new("rts", "RTS").children(vec![
                MenuItem::new("rts.down", "Down"),
                MenuItem::new("rts.up", "Up"),
            ]),
            MenuItem::new("dtr", "DTR").children(vec![MenuItem::new("dtr.down", "Down")]),
        ]);

        assert!(state.select_deep("rts.up"));
        assert_eq!(state.trail(), vec!["RTS".to_string()]);
        assert_eq!(state.rows()[state.selected()].id, "rts.up");
        assert!(state.rows()[0].back, "the level above is one step back");
    }

    #[test]
    fn a_sub_entry_that_cannot_be_selected_leaves_the_level_alone() {
        let mut state = MenuState::new(vec![
            MenuItem::new("copy", "Copy"),
            MenuItem::new("rts", "RTS")
                .children(vec![MenuItem::new("rts.down", "Down").enabled(false)]),
        ]);

        assert!(!state.select_deep("rts.down"));
        assert!(state.trail().is_empty());
        assert_eq!(state.rows()[state.selected()].id, "copy");
    }

    #[test]
    fn an_entry_that_is_not_there_leaves_the_selection_alone() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);
        let before = state.selected();

        assert!(!state.select_id("nothing_of_the_sort"));
        assert_eq!(state.selected(), before);
    }

    #[test]
    fn an_entry_that_cannot_be_chosen_is_not_selected() {
        let mut state = MenuState::new(menu());

        assert!(!state.select_id("paste"), "a disabled entry is no target");
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn the_selection_skips_what_cannot_be_chosen_and_wraps() {
        let mut state = MenuState::new(menu());
        assert_eq!(state.selected(), 0);

        state.move_selection(1);
        assert_eq!(
            state.selected(),
            3,
            "the disabled entry and the line are left out"
        );

        state.move_selection(1);
        assert_eq!(state.selected(), 0, "the end wraps to the start");

        state.move_selection(-1);
        assert_eq!(state.selected(), 3, "and the start to the end");
    }

    #[test]
    fn walking_in_and_out_follows_the_tree() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);

        assert!(state.step_in());
        assert_eq!(state.trail(), vec!["Cat file".to_string()]);
        let rows = state.rows();
        assert_eq!(rows.len(), 3, "the two entries and the row leading back");
        assert!(rows[0].back);
        assert_eq!(rows[0].label, "\u{2039} Cat file");
        assert_eq!(
            state.selected(),
            1,
            "the selection starts on the first entry, not on the way back"
        );
        assert_eq!(state.accept(), Some("profile.zmodem".to_string()));

        assert!(state.step_out());
        assert!(state.trail().is_empty());
        assert!(!state.step_out(), "the top level has nothing to leave");
    }

    #[test]
    fn the_row_that_names_the_level_above_leads_back_to_it() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);
        state.step_in();

        state.move_selection(-1);
        assert!(state.rows()[state.selected()].back);
        assert_eq!(state.accept(), None);
        assert!(state.trail().is_empty(), "choosing it steps back out");
    }

    #[test]
    fn a_parent_that_cannot_be_chosen_is_walked_into_by_enter() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);

        assert_eq!(state.accept(), None);
        assert_eq!(state.trail(), vec!["Cat file".to_string()]);
    }

    #[test]
    fn typing_searches_the_whole_tree_and_leaving_the_query_restores_the_level() {
        let mut state = MenuState::new(menu());
        state.push_query("xmo");

        let rows = state.rows();
        assert_eq!(
            rows.first().map(|row| row.id.clone()),
            Some("profile.xmodem".to_string())
        );
        assert_eq!(rows[0].parents, "Cat file");
        assert_eq!(state.accept(), Some("profile.xmodem".to_string()));

        assert!(state.step_out());
        assert_eq!(state.query(), "");
        assert_eq!(state.rows().len(), 4);
    }

    #[test]
    fn an_entry_that_is_choosable_is_chosen_and_still_stepped_into() {
        let mut state = MenuState::new(vec![
            MenuItem::new("source:/dev/ttyUSB0", "/dev/ttyUSB0")
                .choosable(true)
                .children(vec![MenuItem::new(
                    "source.default:/dev/ttyUSB0",
                    "At start",
                )]),
        ]);

        assert_eq!(
            state.accept(),
            Some("source:/dev/ttyUSB0".to_string()),
            "Enter chooses it instead of walking into it"
        );

        assert!(state.step_in(), "the arrow still walks into it");
        assert_eq!(state.rows()[1].id, "source.default:/dev/ttyUSB0");
    }

    #[test]
    fn entries_put_in_place_of_the_others_keep_the_selection_and_the_query() {
        let mut state = MenuState::new(menu());
        state.push_query("xmodem");
        assert!(state.select_id("profile.xmodem"), "the query finds it");

        state.refill(vec![
            MenuItem::new("added", "Added"),
            MenuItem::new("profile", "Cat file").children(vec![
                MenuItem::new("profile.zmodem", "zmodem"),
                MenuItem::new("profile.xmodem", "xmodem"),
            ]),
        ]);

        assert_eq!(state.query(), "xmodem", "the query is left alone");
        assert_eq!(state.rows()[state.selected()].id, "profile.xmodem");
    }

    #[test]
    fn an_entry_that_is_gone_leaves_the_selection_on_the_first_one() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);
        assert_eq!(state.rows()[state.selected()].id, "profile");

        state.refill(vec![MenuItem::new("copy", "Copy")]);

        assert_eq!(state.rows()[state.selected()].id, "copy");
    }

    #[test]
    fn a_level_that_is_gone_is_left_for_the_one_above_it() {
        let mut state = MenuState::new(menu());
        state.move_selection(1);
        state.step_in();

        state.refill(vec![MenuItem::new("copy", "Copy")]);

        assert!(state.trail().is_empty());
        assert_eq!(state.rows()[state.selected()].id, "copy");
    }

    /// Entries that are found by `usb` and by nothing past it.
    fn ports() -> Vec<MenuItem> {
        vec![
            MenuItem::new("usb0", "ttyUSB0"),
            MenuItem::new("usb1", "ttyUSB1"),
            MenuItem::new("acm0", "ttyACM0"),
        ]
    }

    /// `Backspace` over a query nothing answers takes back everything past the
    /// last beginning that something answers, in one press.
    #[test]
    fn a_query_that_finds_nothing_is_cut_back_to_what_still_finds_something() {
        let mut state = MenuState::new(ports());
        state.push_query("usbxyz");
        assert!(state.nothing_found());

        assert!(state.pop_query());
        assert_eq!(state.query(), "usb");
        assert_eq!(state.caret(), 3);
        assert!(!state.nothing_found());

        assert!(state.pop_query());
        assert_eq!(state.query(), "us", "a query with hits loses one character");
    }

    /// A word typed in the wrong keyboard layout finds nothing from its first
    /// letter on, and one press clears it.
    #[test]
    fn a_query_that_found_nothing_from_its_first_character_is_cleared() {
        let mut state = MenuState::new(ports());
        state.push_query("гыи");

        assert!(state.pop_query());
        assert_eq!(state.query(), "");
        assert_eq!(state.rows().len(), 3, "the level is shown again");
    }

    /// The caret is where typed text goes and what `Backspace` deletes before.
    #[test]
    fn text_goes_in_at_the_caret() {
        let mut state = MenuState::new(ports());
        state.push_query("tyb");
        state.caret_home();
        state.push_query("t");
        assert_eq!((state.query(), state.caret()), ("ttyb", 1));
        state.pop_query();
        assert_eq!((state.query(), state.caret()), ("tyb", 0));
        state.caret_end();
        assert_eq!(state.caret(), 3);

        let mut state = MenuState::new(ports());
        state.push_query("usb");
        state.caret_home();
        state.push_query("x");
        assert!(state.nothing_found());
        state.pop_query();
        assert_eq!(
            state.query(),
            "",
            "with nothing found, the text right of the caret goes as well"
        );
    }

    #[test]
    fn a_query_is_cleared_and_the_level_comes_back() {
        let mut state = MenuState::new(ports());
        assert!(!state.clear_query(), "there was none");
        state.push_query("acm");
        assert!(state.clear_query());
        assert_eq!((state.query(), state.caret()), ("", 0));
        assert_eq!(state.rows().len(), 3);
    }

    /// Ten entries with a line and a disabled entry among them.
    fn long() -> Vec<MenuItem> {
        let mut items: Vec<MenuItem> = (0..10)
            .map(|index| MenuItem::new(format!("id{index}"), format!("entry {index}")))
            .collect();
        items[4] = MenuItem::separator();
        items[9] = MenuItem::new("id9", "entry 9").enabled(false);
        items
    }

    /// A page is walked without wrapping, and lands on an entry: never on a
    /// line, never past the last entry that can be chosen.
    #[test]
    fn a_page_stops_at_the_ends_and_lands_on_an_entry() {
        let mut state = MenuState::new(long());

        state.move_page(4);
        assert_eq!(state.selected(), 3, "the line at 4 is passed back over");
        state.move_page(4);
        assert_eq!(state.selected(), 7);
        state.move_page(4);
        assert_eq!(state.selected(), 8, "the last entry that can be chosen");
        state.move_page(4);
        assert_eq!(state.selected(), 8, "and no further");

        state.move_page(-6);
        assert_eq!(state.selected(), 2);
        state.move_page(-6);
        assert_eq!(state.selected(), 0, "and no wrapping at the start");
    }

    #[test]
    fn the_last_entry_is_one_that_can_be_chosen() {
        let mut state = MenuState::new(long());
        state.select_last();
        assert_eq!(state.selected(), 8);
        state.select_first();
        assert_eq!(state.selected(), 0);
    }

    /// A window moved away from the selection takes it along, to the row shown
    /// nearest to where it was.
    #[test]
    fn a_selection_the_window_left_lands_on_the_nearest_row_shown() {
        let mut state = MenuState::new(long());

        state.select_shown(3, 6);
        assert_eq!(state.selected(), 3, "the window moved down past it");

        state.select_shown(0, 3);
        assert_eq!(state.selected(), 2, "the window moved up past it");

        state.select_shown(1, 4);
        assert_eq!(state.selected(), 2, "a selection shown stays");
    }

    /// The zone of an entry that is chosen steps into it, the way `Right` does.
    #[test]
    fn an_entry_that_is_chosen_is_stepped_into_by_its_zone() {
        let mut state = MenuState::new(vec![
            MenuItem::new("copy", "Copy"),
            MenuItem::new("source", "/dev/ttyUSB0")
                .choosable(true)
                .children(vec![MenuItem::new("default", "At start")]),
        ]);
        assert!(state.rows()[1].is_split());
        assert!(!state.rows()[0].is_split());

        assert!(state.step_into(1));
        assert_eq!(state.trail(), vec!["/dev/ttyUSB0".to_string()]);
        assert_eq!(state.rows()[state.selected()].id, "default");
        assert!(
            !state.rows()[0].is_split(),
            "the row leading back is never split"
        );
    }

    #[test]
    fn the_query_is_shortened_one_character_at_a_time() {
        let mut state = MenuState::new(menu());
        state.push_query("co");
        assert!(state.pop_query());
        assert_eq!(state.query(), "c");
        assert!(state.pop_query());
        assert!(!state.pop_query());
    }
}
