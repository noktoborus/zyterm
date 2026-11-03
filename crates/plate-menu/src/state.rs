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
    /// The mark drawn before the label, empty when there is none.
    pub icon: String,
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
        if let Some(item) = item_at(&self.items, &self.path).filter(|_| !self.path.is_empty()) {
            rows.push(back_row(item));
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
    ///
    /// The selection lands on the entry the level was stepped into from, so
    /// a walk in and out again leaves it where it was.
    pub fn step_out(&mut self) -> bool {
        if self.clear_query() {
            return true;
        }
        let left = self.path.clone();
        if self.path.pop().is_none() {
            return false;
        }
        match self.rows().iter().position(|row| row.path == left) {
            Some(index) => self.selected = index,
            None => self.select_first(),
        }
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

    /// What `Enter` does with `Shift` held or not. Held, it steps into the
    /// entries of the selected entry, even of one that is chosen, and does what
    /// `accept` does on a row that has none.
    pub fn accept_held(&mut self, shift: bool) -> Option<String> {
        if shift && self.step_in() {
            return None;
        }
        self.accept()
    }

    /// Puts the selection on the row at this index, the way the pointer does
    /// when it moves onto it. False when that row cannot be chosen.
    pub fn select(&mut self, index: usize) -> bool {
        let reachable = self.rows().get(index).is_some_and(Row::is_reachable);
        if reachable {
            self.selected = index;
        }
        reachable
    }

    /// Chooses the row at this index, the way a click does; with `Shift` held,
    /// the way `Shift+Enter` does.
    pub fn choose(&mut self, index: usize, shift: bool) -> Option<String> {
        self.select(index)
            .then(|| self.accept_held(shift))
            .flatten()
    }

    /// Steps into the entries of the row at this index, the way a click on
    /// the zone of a split entry does. True when it had any.
    pub fn step_into(&mut self, index: usize) -> bool {
        self.select(index) && self.step_in()
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
fn back_row(item: &MenuItem) -> Row {
    Row {
        path: Vec::new(),
        id: String::new(),
        label: format!("{} {}", crate::view::BACK, item.label),
        icon: item.icon.clone(),
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
        icon: item.icon.clone(),
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
mod tests;
