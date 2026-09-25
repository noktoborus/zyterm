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
    /// The whole of what the label was cut from, shown on a plate beside the
    /// menu while this entry is the selected one, empty when the label is the
    /// whole of it.
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

    /// More text the query is matched against.
    ///
    /// A label is what an entry is called, and it is not always all an entry is
    /// known by: a device answers to the path it is at and to the name of the
    /// thing plugged in there, and somebody looking for it types whichever of
    /// the two they have in mind. What is given here is matched along with the
    /// label and drawn nowhere, so what an entry can be found by and what it
    /// shows are two questions and the caller answers both.
    pub fn search(mut self, search: impl Into<String>) -> Self {
        self.search = search.into();
        self
    }

    /// The whole of what the label was cut from.
    ///
    /// A label is one line of a plate, so an entry made of more than fits into
    /// one — a line too long for it, a text of several lines — is cut down by
    /// the caller and the whole of it given here. It is shown on a plate of its
    /// own right of the menu, which moves left to make room for it: a level
    /// where nothing carries one has neither the plate nor the room.
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
    /// An entry that carries entries is a way further in, so `Enter` and a
    /// click walk into it and the identifier it has names the level. This says
    /// the entry is a thing of its own as well: it is chosen the way an entry
    /// without children is, and the entries under it are reached by the arrow
    /// alone. It is what a list of things that can each be done with wants —
    /// the entry is the thing, and what else can be done with it is one step
    /// deeper.
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
    /// stepped into.
    ///
    /// A level that is a list of values opens on the value in use, the way a
    /// menu opened with `open_at` does. An identifier none of the entries below
    /// carries, or one that cannot be chosen, leaves the selection on the first
    /// of them.
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
