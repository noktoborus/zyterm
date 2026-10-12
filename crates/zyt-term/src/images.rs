//! Pictures a program sent, and how the cells they stand in say so.
//!
//! A picture is not a cell and the grid holds nothing but cells, so what the
//! grid carries is a mark: the cells a picture stands in say which picture that
//! is, and the picture itself is kept here, by that number.
//!
//! The mark is a zerowidth character of the cell, which is where a combining
//! character goes, so it is drawn as nothing and the character of the cell is
//! left to say whatever it says. Everything the grid does with a cell therefore
//! happens to the mark as well: it scrolls, it reflows when the window is
//! resized, it falls out of the bottom of the scrollback, and a selection takes
//! it. Nothing had to be written for any of that.
//!
//! The mark is a reference nothing holds: a number whose picture the store no
//! longer keeps reads as no picture at all, and the cells are then the text they
//! always were. That is what lets the store drop the oldest picture when the
//! budget is spent without walking the scrollback to find who pointed at it.
//!
//! The characters are code points `U+FDE0`..`U+FDED`, which Unicode promises
//! never to assign, for the reason `crate::null` uses the ones before them: a
//! cell carrying one came from here. The private use area could not promise it,
//! being where the fonts of a prompt keep their arrows.

use crate::sixel::{Pixels, SixelReport};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Zerowidth character that says the cell belongs to a picture. The digits of
/// the number of that picture follow it, most significant first.
const BELONGS: char = '\u{fde0}';

/// Zerowidth character of the digit nought; the nine after it are the others.
const DIGIT_ZERO: char = '\u{fde1}';

/// Zerowidth character before the column of the cell inside the picture.
const AT_COLUMN: char = '\u{fdec}';

/// Zerowidth character before the row of the cell inside the picture.
const AT_ROW: char = '\u{fded}';

/// How much the store keeps before it drops the oldest picture.
///
/// It is a budget and not a count, because what a picture costs is its own
/// size. A device that sends pictures faster than anybody looks at them is why
/// there is a limit at all.
pub const DEFAULT_PICTURE_BUDGET: usize = 16 * 1024 * 1024;

/// Which picture of a snapshot a cell belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(pub u32);

/// Which picture a cell belongs to, and which part of it the cell shows.
///
/// The place is the cell's own and not the picture's, so a cell says what to
/// draw in it without anything having to find the rest of the picture first.
/// That is what keeps a picture whose first rows have scrolled off the page, or
/// whose rectangle a reflow has pulled apart, drawing what each cell of it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePart {
    /// The picture.
    pub id: ImageId,
    /// Column of the cell inside the rectangle of the picture.
    pub column: usize,
    /// Row of the cell inside it.
    pub row: usize,
}

/// A picture a program sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// What the payload said about itself.
    pub report: SixelReport,
    /// The payload as it arrived, the introducer and the terminator left out.
    pub payload: Arc<[u8]>,
    /// The cells it was placed in: columns and rows.
    ///
    /// It is what the picture takes on the page, which is not what its pixels
    /// say on their own: a cell is as large as the font of the moment, and a
    /// picture wider than the room left on the line is fitted to that room.
    /// A renderer needs it to know which part of a picture a cell shows, since
    /// the page may hold the bottom of one whose first row has scrolled off.
    pub cells: (usize, usize),
    /// Whether the page shows the line of it instead of the picture.
    ///
    /// A picture is drawn by default and folded by whoever asked for it to be:
    /// it is the one thing about a picture on the page that is a reader's
    /// choice and not the program's. Folded, the cells show the line that was
    /// written through them, which says what the picture is; the picture itself
    /// is kept and the cells go on naming it, so unfolding it draws it again.
    pub folded: bool,
    /// The pixels of it, read as it arrived.
    ///
    /// They are read then and not when somebody asks, because somebody asking
    /// is a pointer waiting: a picture of a screenful comes to milliseconds of
    /// reading, and a frame spent on it is a frame the window stood still. What
    /// asks for a picture — the drawing of it, the menu of it — is handed what
    /// is already there.
    pub pixels: Arc<Pixels>,
}

impl Picture {
    /// What this picture costs the budget.
    ///
    /// Both halves of it: the payload is kept because it is what the program
    /// sent, and the pixels because reading them again is the cost this keeps
    /// paying.
    fn cost(&self) -> usize {
        self.payload.len() + self.pixels.rgba.len()
    }
}

/// The pictures of one terminal, by number.
#[derive(Debug)]
pub(crate) struct ImageStore {
    kept: BTreeMap<ImageId, Arc<Picture>>,
    next: u32,
    held: usize,
    budget: usize,
}

impl Default for ImageStore {
    fn default() -> Self {
        Self {
            kept: BTreeMap::new(),
            next: 1,
            held: 0,
            budget: DEFAULT_PICTURE_BUDGET,
        }
    }
}

impl ImageStore {
    /// Keeps a picture and answers the number the cells will carry.
    ///
    /// The oldest pictures go until the newest one fits, and a picture larger
    /// than the whole budget is not kept at all: its cells then read as the
    /// text they are, which is the same answer a reader gets for a picture that
    /// was dropped an hour ago.
    pub fn keep(&mut self, picture: Picture) -> Option<ImageId> {
        if picture.cost() > self.budget {
            return None;
        }

        let id = ImageId(self.next);
        self.next = self.next.wrapping_add(1).max(1);
        self.held += picture.cost();
        self.kept.insert(id, Arc::new(picture));

        while self.held > self.budget {
            let Some(oldest) = self.kept.keys().next().copied() else {
                break;
            };
            if oldest == id {
                break;
            }
            self.drop_one(oldest);
        }
        Some(id)
    }

    /// The picture of that number, while the store still has it.
    pub fn get(&self, id: ImageId) -> Option<Arc<Picture>> {
        self.kept.get(&id).cloned()
    }

    /// Folds a picture, or unfolds it.
    pub fn fold(&mut self, id: ImageId, folded: bool) {
        if let Some(picture) = self.kept.get_mut(&id) {
            Arc::make_mut(picture).folded = folded;
        }
    }

    /// Says which cells a picture stands in, which is settled when it is
    /// placed.
    pub fn set_cells(&mut self, id: ImageId, cells: (usize, usize)) {
        if let Some(picture) = self.kept.get_mut(&id) {
            Arc::make_mut(picture).cells = cells;
        }
    }

    /// How many pictures are kept, how much they take, and what the budget is.
    pub fn held(&self) -> (usize, usize, usize) {
        (self.kept.len(), self.held, self.budget)
    }

    /// Changes the budget, dropping the oldest pictures down to it at once.
    ///
    /// A lowered budget is memory the window was asked to give back, so it is
    /// given back now and not when the next picture arrives.
    pub fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
        while self.held > self.budget {
            let Some(oldest) = self.kept.keys().next().copied() else {
                break;
            };
            self.drop_one(oldest);
        }
    }

    /// Drops every picture.
    ///
    /// The numbers go on from where they stood, so a cell still carrying the
    /// mark of a dropped picture, or a renderer still holding its texture,
    /// never meets a new picture under the old number.
    pub fn clear(&mut self) {
        self.kept.clear();
        self.held = 0;
    }

    fn drop_one(&mut self, id: ImageId) {
        if let Some(picture) = self.kept.remove(&id) {
            self.held = self.held.saturating_sub(picture.cost());
        }
    }
}

/// The characters one cell of a picture carries: the picture, then the place of
/// the cell inside it.
pub(crate) fn mark(part: ImagePart) -> Vec<char> {
    let mut marked = Vec::with_capacity(12);
    for (lead, number) in [
        (BELONGS, part.id.0 as usize),
        (AT_COLUMN, part.column),
        (AT_ROW, part.row),
    ] {
        marked.push(lead);
        marked.extend(digits(number));
    }
    marked
}

fn digits(number: usize) -> impl Iterator<Item = char> {
    number
        .to_string()
        .into_bytes()
        .into_iter()
        .map(|digit| digit_char(digit - b'0'))
}

/// What the zerowidth characters of a cell say about a picture, when they say
/// anything.
pub(crate) fn part_of(zerowidth: &[char]) -> Option<ImagePart> {
    let number = |lead: char| {
        let mut characters = zerowidth.iter().skip_while(|ch| **ch != lead);
        characters.next()?;

        let mut number: usize = 0;
        let mut digits = 0;
        for digit in characters.map_while(|ch| digit_of(*ch)) {
            number = number.checked_mul(10)?.checked_add(digit as usize)?;
            digits += 1;
        }
        (digits > 0).then_some(number)
    };

    Some(ImagePart {
        id: ImageId(u32::try_from(number(BELONGS)?).ok()?),
        column: number(AT_COLUMN).unwrap_or(0),
        row: number(AT_ROW).unwrap_or(0),
    })
}

/// Whether a character is part of such a mark, which is what text taken out of
/// the grid leaves behind.
pub(crate) fn is_mark(ch: char) -> bool {
    matches!(ch, BELONGS | AT_COLUMN | AT_ROW) || digit_of(ch).is_some()
}

fn digit_char(digit: u8) -> char {
    char::from_u32(DIGIT_ZERO as u32 + digit.min(9) as u32).unwrap_or(DIGIT_ZERO)
}

fn digit_of(ch: char) -> Option<u8> {
    (DIGIT_ZERO..=digit_char(9))
        .contains(&ch)
        .then(|| (ch as u32 - DIGIT_ZERO as u32) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(bytes: usize) -> Picture {
        Picture {
            report: SixelReport::default(),
            payload: vec![b'q'; bytes].into(),
            cells: (1, 1),
            folded: false,
            pixels: Arc::new(Pixels {
                width: 1,
                height: 1,
                rgba: Vec::new(),
            }),
        }
    }

    /// What is written into a cell is what is read back out of it, whatever
    /// stands beside it.
    #[test]
    fn a_mark_is_read_back_as_what_it_was_written_from() {
        for (number, column, row) in [(1_u32, 0, 0), (7, 3, 19), (42, 11, 0), (u32::MAX, 999, 7)] {
            let part = ImagePart {
                id: ImageId(number),
                column,
                row,
            };
            assert_eq!(part_of(&mark(part)), Some(part), "{part:?}");

            let mut beside = vec!['\u{301}'];
            beside.extend(mark(part));
            assert_eq!(
                part_of(&beside),
                Some(part),
                "{part:?} after a combining mark"
            );
        }

        assert_eq!(part_of(&[]), None);
        assert_eq!(
            part_of(&['\u{301}']),
            None,
            "a combining mark is no picture"
        );
        assert_eq!(
            part_of(&[BELONGS]),
            None,
            "a mark with no digits is no number"
        );
    }

    /// The oldest pictures go when the budget is spent, and a number whose
    /// picture is gone answers nothing.
    #[test]
    fn the_oldest_pictures_go_when_the_budget_is_spent() {
        let mut store = ImageStore::default();
        store.set_budget(100);

        let first = store.keep(picture(40)).expect("the first is kept");
        let second = store.keep(picture(40)).expect("the second is kept");
        assert_eq!(store.held().1, 80);

        let third = store.keep(picture(40)).expect("the third is kept");
        assert_eq!(store.held().1, 80, "and the first one went");
        assert_eq!(store.get(first), None, "a picture that is gone is gone");
        assert!(store.get(second).is_some());
        assert!(store.get(third).is_some());

        assert_eq!(
            store.keep(picture(101)),
            None,
            "a picture larger than the whole budget is not kept"
        );
        assert!(
            store.get(third).is_some(),
            "and nothing is dropped for the one that was refused"
        );
    }

    /// A store cleared holds nothing, and the next picture gets a number none
    /// before it had.
    #[test]
    fn a_cleared_store_holds_nothing_and_numbers_on() {
        let mut store = ImageStore::default();
        let first = store.keep(picture(64)).expect("it is kept");

        store.clear();
        assert_eq!(store.get(first), None);
        assert_eq!(store.held().0, 0);
        assert_eq!(store.held().1, 0);

        let next = store.keep(picture(64)).expect("the next is kept");
        assert_ne!(next, first, "a dropped number is not handed out again");
    }

    /// A lowered budget is memory given back at once.
    #[test]
    fn a_lowered_budget_drops_what_no_longer_fits() {
        let mut store = ImageStore::default();
        let kept = store.keep(picture(1024)).expect("it is kept");

        store.set_budget(512);
        assert_eq!(store.get(kept), None);
        assert_eq!(store.held().1, 0);
    }
}
