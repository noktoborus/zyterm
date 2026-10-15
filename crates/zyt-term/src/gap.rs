//! The line of a gap: output skipped on the other end, marked where it was.
//!
//! A terminal fed from somewhere that may throw output away — a pane of tmux
//! paused because it fell behind — has a hole in its scrollback that somebody
//! reading a log must see, or the lines either side of it are read as one after
//! the other. [`Terminal::mark_gap`](crate::Terminal::mark_gap) writes a line
//! that says so.
//!
//! The line is made as a run of NUL bytes is (`crate::null`): cells with
//! noncharacters, which the renderer draws itself — the sign of a pause and a
//! dashed rule — and between them the text the caller gave, in plain cells of
//! the font of the line. This crate holds no text of its own, so what the line
//! says is the caller's.

/// The character the mark of a gap reads as outside the grid, `DOUBLE
/// VERTICAL BAR`: the sign of a pause.
pub const PAUSE_SIGN: char = '\u{23f8}';

/// Cell the sign of the pause stands in.
const MARK: char = '\u{fddc}';

/// Cell a piece of the rule stands in.
const RULE: char = '\u{fddd}';

/// Cells of the rule before the sign.
const LEAD: usize = 2;

/// What one cell of the line of a gap carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapPart {
    /// The sign of the pause.
    Mark,
    /// A piece of the dashed rule.
    Rule,
}

/// The part of a gap this character is, and nothing for every other
/// character.
pub fn gap_part(ch: char) -> Option<GapPart> {
    match ch {
        MARK => Some(GapPart::Mark),
        RULE => Some(GapPart::Rule),
        _ => None,
    }
}

/// What a cell of a gap reads as outside the grid: the sign as `⏸`, a piece of
/// the rule as nothing, so a selection over the line pastes `⏸ 14 s skipped`.
pub(crate) fn gap_text(ch: char) -> Option<Option<char>> {
    match gap_part(ch)? {
        GapPart::Mark => Some(Some(PAUSE_SIGN)),
        GapPart::Rule => Some(None),
    }
}

/// The bytes of the line of a gap `columns` wide: the rule, the sign, `text`
/// cut to what fits, and the rule to the end.
pub(crate) fn line(text: &str, columns: usize) -> String {
    let room = columns.saturating_sub(LEAD + 4);
    let text: String = text.chars().take(room).collect();
    let mut line = String::new();
    line.extend(std::iter::repeat_n(RULE, LEAD.min(columns)));
    if columns > LEAD + 2 {
        line.push(' ');
        line.push(MARK);
        line.push(' ');
        line.push_str(&text);
        line.push(' ');
    }
    let used = line.chars().count();
    line.extend(std::iter::repeat_n(RULE, columns.saturating_sub(used)));
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_fills_the_width_and_cuts_its_text() {
        let made = line("14 s skipped", 30);
        assert_eq!(made.chars().count(), 30);
        assert!(made.contains("14 s skipped"));
        assert_eq!(gap_part(made.chars().next().unwrap()), Some(GapPart::Rule));
        assert_eq!(line("a very long text indeed", 12).chars().count(), 12);
        assert_eq!(line("x", 2).chars().count(), 2, "narrower than the sign");
    }
}
