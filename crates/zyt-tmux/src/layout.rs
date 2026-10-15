//! The layout of a window, as `%layout-change` writes it:
//! `b25f,80x24,0,0{40x24,0,0,1,39x24,41,0,2}`.

use crate::error::{Error, Result};
use crate::id::{PaneId, number};

/// How deep `{…}` and `[…]` may nest. The parser is a recursive descent, and
/// a line of the protocol is not given the stack.
pub const MAX_DEPTH: usize = 64;

/// One cell of a layout: its size, where it stands, and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Width in columns.
    pub columns: u32,
    /// Height in rows.
    pub rows: u32,
    /// Column of the left edge in the window.
    pub x: u32,
    /// Row of the top edge in the window.
    pub y: u32,
    /// A pane, or cells side by side.
    pub content: Content,
}

/// What a cell holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// One pane.
    Pane(PaneId),
    /// Cells left to right, `{…}`.
    Columns(Vec<Layout>),
    /// Cells top to bottom, `[…]`.
    Rows(Vec<Layout>),
}

impl Layout {
    /// Reads a layout and checks its checksum.
    pub fn parse(text: &[u8]) -> Result<Self> {
        let (written, body) = text
            .split_first_chunk::<5>()
            .filter(|(head, _)| head[4] == b',')
            .ok_or(Error::MalformedLayout)?;
        let written = std::str::from_utf8(&written[..4])
            .ok()
            .and_then(|hex| u16::from_str_radix(hex, 16).ok())
            .ok_or(Error::MalformedLayout)?;
        let counted = checksum(body);
        if written != counted {
            return Err(Error::LayoutChecksum { written, counted });
        }
        let mut reader = Reader { text: body, at: 0 };
        let layout = reader.cell(0)?;
        if reader.at != body.len() {
            return Err(Error::MalformedLayout);
        }
        Ok(layout)
    }

    /// The panes of the layout, left to right and top to bottom.
    pub fn panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        self.each(&mut |cell| {
            if let Content::Pane(pane) = cell.content {
                panes.push(pane);
            }
        });
        panes
    }

    /// The cell of a pane.
    pub fn find(&self, pane: PaneId) -> Option<&Layout> {
        match &self.content {
            Content::Pane(found) => (*found == pane).then_some(self),
            Content::Columns(cells) | Content::Rows(cells) => {
                cells.iter().find_map(|cell| cell.find(pane))
            }
        }
    }

    fn each<'a>(&'a self, visit: &mut impl FnMut(&'a Layout)) {
        visit(self);
        if let Content::Columns(cells) | Content::Rows(cells) = &self.content {
            for cell in cells {
                cell.each(visit);
            }
        }
    }
}

/// The checksum of tmux (`layout_checksum`): a rotate right and an add per
/// byte, sixteen bits.
pub fn checksum(text: &[u8]) -> u16 {
    text.iter().fold(0u16, |sum, &byte| {
        sum.rotate_right(1).wrapping_add(u16::from(byte))
    })
}

/// Reads cells from a layout, its checksum taken off.
struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    /// `WxH,X,Y` and then `,ID`, `{cells}` or `[cells]`.
    fn cell(&mut self, depth: usize) -> Result<Layout> {
        let columns = self.number()?;
        self.expect(b'x')?;
        let rows = self.number()?;
        self.expect(b',')?;
        let x = self.number()?;
        self.expect(b',')?;
        let y = self.number()?;
        let content = match self.peek() {
            Some(b',') => {
                self.at += 1;
                Content::Pane(PaneId(self.number()?))
            }
            Some(b'{') => Content::Columns(self.cells(depth, b'}')?),
            Some(b'[') => Content::Rows(self.cells(depth, b']')?),
            _ => return Err(Error::MalformedLayout),
        };
        Ok(Layout {
            columns,
            rows,
            x,
            y,
            content,
        })
    }

    /// Cells parted by `,` up to `close`, the opening bracket at hand.
    fn cells(&mut self, depth: usize, close: u8) -> Result<Vec<Layout>> {
        if depth + 1 > MAX_DEPTH {
            return Err(Error::MalformedLayout);
        }
        self.at += 1;
        let mut cells = vec![self.cell(depth + 1)?];
        loop {
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    cells.push(self.cell(depth + 1)?);
                }
                Some(byte) if byte == close => {
                    self.at += 1;
                    return Ok(cells);
                }
                _ => return Err(Error::MalformedLayout),
            }
        }
    }

    fn number(&mut self) -> Result<u32> {
        let digits = self.text[self.at..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let value = number(&self.text[self.at..self.at + digits]).ok_or(Error::MalformedLayout)?;
        self.at += digits;
        Ok(value)
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek() != Some(byte) {
            return Err(Error::MalformedLayout);
        }
        self.at += 1;
        Ok(())
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
}

#[cfg(test)]
mod tests;
