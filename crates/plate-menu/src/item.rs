//! Entries of a menu.

/// One entry of a menu.
///
/// An entry is either something that can be chosen, a parent carrying more
/// entries, or a separator drawn as a line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuItem {
    /// What the caller is told when this entry is chosen.
    pub id: String,
    /// Text of the entry.
    pub label: String,
    /// Text shown right of the label — a key sequence, a value, a count —
    /// empty when there is none.
    pub detail: String,
    /// Text shown while the pointer rests on the entry, empty when there is
    /// none.
    pub hint: String,
    /// More text the query is matched against, drawn nowhere.
    pub search: String,
    /// False keeps the entry out of the hits of a query.
    pub searchable: bool,
    /// The whole of what the label was cut from, or what more the entry says;
    /// empty when the label says all of it.
    pub full: String,
    /// False draws the entry weak and refuses to choose it.
    pub enabled: bool,
    /// True while an entry carrying entries of its own is chosen rather than
    /// stepped into.
    pub choosable: bool,
    /// Entries one step deeper.
    pub children: Vec<MenuItem>,
    /// Identifier of the entry the selection lands on when this one is stepped
    /// into, empty for the first entry of the level.
    pub opens_at: String,
    /// True for a line between entries.
    pub separator: bool,
}

impl MenuItem {
    /// Entry with an identifier and a label.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: String::new(),
            hint: String::new(),
            search: String::new(),
            searchable: true,
            full: String::new(),
            enabled: true,
            choosable: false,
            children: Vec::new(),
            opens_at: String::new(),
            separator: false,
        }
    }

    /// A line between entries.
    pub fn separator() -> Self {
        Self {
            enabled: false,
            separator: true,
            ..Self::default()
        }
    }

    /// Text shown right of the label, which is where a key sequence, a value or
    /// a count belongs.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Text shown while the pointer rests on the entry.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    /// More text the query is matched against, drawn nowhere.
    ///
    /// A port is found by its path and by the name of the device in it.
    /// See [Searching](crate#searching).
    pub fn search(mut self, search: impl Into<String>) -> Self {
        self.search = search.into();
        self
    }

    /// Whether a query can find the entry.
    ///
    /// `false` for an entry about its parent: in the flat list of hits it would
    /// not say which parent it acts on. See [Searching](crate#searching).
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    /// The whole of what the label was cut from, or what more the entry says.
    ///
    /// A label is one line; the caller cuts it and gives the whole here, or
    /// more than one line holds. It is shown beside the menu or above it while
    /// the entry is selected, wrapped, a newline starting a line. See
    /// [The plate of a whole](crate#the-plate-of-a-whole).
    pub fn full(mut self, full: impl Into<String>) -> Self {
        self.full = full.into();
        self
    }

    /// Whether the entry can be chosen.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Whether an entry carrying entries of its own is chosen rather than
    /// stepped into.
    ///
    /// `Enter` and a click on its label then choose it; `Right` and a click on
    /// the zone at the right end of its plate step in. See
    /// [Levels](crate#levels).
    pub fn choosable(mut self, choosable: bool) -> Self {
        self.choosable = choosable;
        self
    }

    /// Entries one step deeper.
    pub fn children(mut self, children: Vec<MenuItem>) -> Self {
        self.children = children;
        self
    }

    /// The entry of the level below the selection lands on when this one is
    /// stepped into: the value in use. An unknown or unreachable one leaves it
    /// on the first.
    pub fn opens_at(mut self, id: impl Into<String>) -> Self {
        self.opens_at = id.into();
        self
    }

    /// True while this entry can be chosen or walked into.
    pub fn is_reachable(&self) -> bool {
        !self.separator && (self.enabled || !self.children.is_empty())
    }

    /// True while this entry carries entries of its own.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}
