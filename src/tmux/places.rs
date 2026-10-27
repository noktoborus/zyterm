//! Where the panes of the window shown stand, side by side as the layout of
//! tmux puts them, and the dividers between them: the cells tmux keeps there.

use zyt_tmux::{Content, Layout, PaneId};

/// The panes of the window shown, drawn at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Places {
    /// The window.
    pub window: Option<zyt_tmux::WindowId>,
    /// The pane the keys go to, drawn by the main view.
    pub shown: Option<PaneId>,
    /// Columns of the window.
    pub columns: usize,
    /// Rows of the window.
    pub rows: usize,
    /// Each pane seen, where it stands.
    pub panes: Vec<Place>,
    /// The cells between two of them.
    pub dividers: Vec<Divider>,
}

/// A pane, in cells of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    /// The pane.
    pub pane: PaneId,
    /// Column of its left edge.
    pub x: usize,
    /// Row of its top edge.
    pub y: usize,
    /// Its width.
    pub columns: usize,
    /// Its height.
    pub rows: usize,
    /// What is shown of it is not its screen yet: a plate stands over it.
    pub waits: bool,
}

/// The cell tmux keeps between two cells of a layout: a column between
/// cells side by side, a row between cells one above the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divider {
    /// The column, or the row, of the divider.
    pub at: usize,
    /// Where it begins along its length, and how long it is.
    pub from: usize,
    /// How long it is.
    pub length: usize,
    /// A column between cells side by side; a row otherwise.
    pub side_by_side: bool,
    /// A pane of the cell before it: `resize-pane` of it moves the divider.
    pub pane: PaneId,
    /// Where the cell before it begins: the size it is given is counted from
    /// here.
    pub start: usize,
}

impl Places {
    /// The panes of a layout that are seen, with what `waits` says of each;
    /// none when fewer than two are: one pane is the screen of its own.
    pub fn of(layout: &Layout, waits: impl Fn(PaneId) -> bool) -> Self {
        let mut places = Self {
            columns: layout.columns as usize,
            rows: layout.rows as usize,
            ..Self::default()
        };
        walk(layout, &mut places, &waits);
        if places.panes.len() < 2 {
            return Self::default();
        }
        places
    }
}

fn walk(cell: &Layout, places: &mut Places, waits: &impl Fn(PaneId) -> bool) {
    let (cells, side_by_side) = match &cell.content {
        Content::Pane(pane) => {
            places.panes.push(Place {
                pane: *pane,
                x: cell.x as usize,
                y: cell.y as usize,
                columns: cell.columns as usize,
                rows: cell.rows as usize,
                waits: waits(*pane),
            });
            return;
        }
        Content::Columns(cells) => (cells, true),
        Content::Rows(cells) => (cells, false),
    };
    for pair in cells.windows(2) {
        let before = &pair[0];
        let Some(pane) = before.panes().first().copied() else {
            continue;
        };
        let divider = match side_by_side {
            true => Divider {
                at: (before.x + before.columns) as usize,
                from: cell.y as usize,
                length: cell.rows as usize,
                side_by_side,
                pane,
                start: before.x as usize,
            },
            false => Divider {
                at: (before.y + before.rows) as usize,
                from: cell.x as usize,
                length: cell.columns as usize,
                side_by_side,
                pane,
                start: before.y as usize,
            },
        };
        places.dividers.push(divider);
    }
    for inner in cells {
        walk(inner, places, waits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_panes_side_by_side_and_one_under_them() {
        let layout =
            Layout::parse(b"6b21,80x24,0,0[80x12,0,0{40x12,0,0,0,39x12,41,0,2},80x11,0,13,1]")
                .expect("a layout");
        let places = Places::of(&layout, |pane| pane == PaneId(1));
        assert_eq!((places.columns, places.rows), (80, 24));
        let at: Vec<_> = places
            .panes
            .iter()
            .map(|place| {
                (
                    place.pane.0,
                    place.x,
                    place.y,
                    place.columns,
                    place.rows,
                    place.waits,
                )
            })
            .collect();
        assert_eq!(
            at,
            [
                (0, 0, 0, 40, 12, false),
                (2, 41, 0, 39, 12, false),
                (1, 0, 13, 80, 11, true)
            ]
        );
        assert_eq!(
            places.dividers,
            [
                Divider {
                    at: 12,
                    from: 0,
                    length: 80,
                    side_by_side: false,
                    pane: PaneId(0),
                    start: 0
                },
                Divider {
                    at: 40,
                    from: 0,
                    length: 12,
                    side_by_side: true,
                    pane: PaneId(0),
                    start: 0
                },
            ]
        );
    }

    #[test]
    fn one_pane_seen_is_no_places() {
        let layout = Layout::parse(b"b25d,80x24,0,0,0").expect("a layout");
        assert_eq!(Places::of(&layout, |_| false), Places::default());
    }
}
