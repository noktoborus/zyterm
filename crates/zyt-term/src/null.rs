//! Runs of NUL bytes, written into the grid so a renderer can draw them.
//!
//! A NUL byte is a character the standards tell a terminal to ignore, so it
//! reaches no cell and leaves no trace: a device that has gone quiet in the
//! middle of a word, a line read at the wrong speed and a flash image sent to a
//! console all say the same nothing. That nothing is what somebody is looking
//! for, so this crate puts it on the screen.
//!
//! One run of NUL bytes becomes one block of cells: the cell the mark stands
//! in, and the decimal count of the bytes after it. What the mark is drawn as is
//! not decided here — a code point is a glyph only where a font carries one —
//! so the renderer is handed [`NullPart::Mark`] and paints whatever it has.
//! [`NULL_SYMBOL`] is the code point the mark stands for.
//!
//! The cells carry noncharacters and not the characters they are drawn as.
//! `U+FDD0`..`U+FDEF` are code points Unicode promises never to assign, so a
//! device cannot send one and a cell holding one came from here and from nowhere
//! else — which the private use area could not promise, being where the fonts of
//! a prompt keep their arrows and their branches.
//!
//! A run is counted inside one chunk of bytes. A run split over two reads is two
//! blocks, because the first of them is on the screen before the second read
//! happens, and a block that waited for the next chunk would be a run of NUL
//! bytes nothing showed until the device said something else.

/// The code point the mark of a run stands for, `SYMBOL FOR NULL`.
///
/// It is what a renderer draws where a font carries it, and what the text of a
/// selection carries whether one does or not.
pub const NULL_SYMBOL: char = '\u{2400}';

/// Cell the mark of a run stands in.
const MARK: char = '\u{fdd0}';

/// Cell the digit nought stands in; the nine after it are the other digits.
const DIGIT_ZERO: char = '\u{fdd1}';

/// What one cell of a run of NUL bytes carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullPart {
    /// The cell the mark stands in.
    Mark,
    /// One digit of the count, most significant first.
    Digit(u8),
}

impl NullPart {
    /// The character this part is drawn as, given the glyph the caller has for
    /// the mark.
    pub fn text(self, mark: char) -> char {
        match self {
            Self::Mark => mark,
            Self::Digit(digit) => char::from(b'0' + digit.min(9)),
        }
    }
}

/// The part of a run this character is, and nothing for every other character.
pub fn null_part(ch: char) -> Option<NullPart> {
    match ch {
        MARK => Some(NullPart::Mark),
        DIGIT_ZERO..='\u{fdda}' => Some(NullPart::Digit((ch as u32 - DIGIT_ZERO as u32) as u8)),
        _ => None,
    }
}

/// The character a cell of a run reads as outside the grid, which is the mark
/// as its own code point and a digit as a digit.
///
/// It is what text taken out of the grid carries — a selection that is copied, a
/// command a shell marked — so a block that was read as a block on the screen
/// is read as one in whatever it was pasted into.
pub fn null_text(ch: char) -> Option<char> {
    Some(null_part(ch)?.text(NULL_SYMBOL))
}

/// Writes one run of `count` NUL bytes into the buffer, as the bytes the parser
/// is fed.
///
/// A run of nothing writes nothing: there is no block for a count of nought.
pub(crate) fn write_run(count: usize, out: &mut Vec<u8>) {
    if count == 0 {
        return;
    }

    let mut buffer = [0u8; 4];
    out.extend_from_slice(MARK.encode_utf8(&mut buffer).as_bytes());
    for digit in count.to_string().bytes() {
        let cell =
            char::from_u32(DIGIT_ZERO as u32 + u32::from(digit - b'0')).unwrap_or(DIGIT_ZERO);
        out.extend_from_slice(cell.encode_utf8(&mut buffer).as_bytes());
    }
}

/// Copies `bytes` into the buffer with every run of NUL bytes written as a
/// block, and answers whether there was one.
///
/// Nothing is copied while there is no NUL byte in the chunk, which is every
/// chunk of an ordinary session: the caller then feeds the bytes it already has.
pub(crate) fn mark_runs(bytes: &[u8], out: &mut Vec<u8>) -> bool {
    if !bytes.contains(&0) {
        return false;
    }

    out.clear();
    out.reserve(bytes.len());
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|byte| *byte == 0) {
        out.extend_from_slice(&rest[..start]);
        let run = rest[start..].iter().take_while(|byte| **byte == 0).count();
        write_run(run, out);
        rest = &rest[start + run..];
    }
    out.extend_from_slice(rest);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The characters a chunk is written as, read back as the parts they are.
    fn parts(bytes: &[u8]) -> Vec<Option<NullPart>> {
        std::str::from_utf8(bytes)
            .expect("the run is written as text")
            .chars()
            .map(null_part)
            .collect()
    }

    /// Every part of a run is told apart from every other character, and a
    /// character of any script is no part of one.
    #[test]
    fn a_cell_of_a_run_is_told_apart_from_a_character() {
        assert_eq!(null_part(MARK), Some(NullPart::Mark));
        for digit in 0..=9u8 {
            let cell = char::from_u32(DIGIT_ZERO as u32 + u32::from(digit)).expect("a code point");

            assert_eq!(null_part(cell), Some(NullPart::Digit(digit)));
            assert_eq!(null_text(cell), Some(char::from(b'0' + digit)));
        }

        for ch in [
            'a', '0', ' ', '\u{2400}', '\u{e0b0}', '\u{fddb}', '\u{fdef}',
        ] {
            assert_eq!(null_part(ch), None, "{ch:?}");
        }
    }

    /// A chunk with no NUL byte in it is not copied at all, and the caller is
    /// told so.
    #[test]
    fn a_chunk_without_a_nul_byte_is_left_alone() {
        let mut out = vec![0xffu8];

        assert!(!mark_runs(b"nothing missing here", &mut out));
        assert_eq!(out, vec![0xff], "the buffer was not touched");
    }

    /// A run becomes one mark and the digits of how long it was, and the bytes
    /// around it stand where they stood.
    #[test]
    fn a_run_becomes_a_mark_and_its_count() {
        let mut out = Vec::new();
        assert!(mark_runs(b"a\0\0\0b", &mut out));

        let text = std::str::from_utf8(&out).expect("the chunk is text");
        assert_eq!(text.chars().next(), Some('a'));
        assert_eq!(text.chars().last(), Some('b'));
        assert_eq!(
            parts(&out),
            vec![None, Some(NullPart::Mark), Some(NullPart::Digit(3)), None]
        );
    }

    /// Two runs with something between them are two blocks, and a count of more
    /// than one digit is written with all of them.
    #[test]
    fn two_runs_are_two_blocks() {
        let mut chunk = vec![0u8; 12];
        chunk.push(b'x');
        chunk.push(0);
        let mut out = Vec::new();
        assert!(mark_runs(&chunk, &mut out));

        assert_eq!(
            parts(&out),
            vec![
                Some(NullPart::Mark),
                Some(NullPart::Digit(1)),
                Some(NullPart::Digit(2)),
                None,
                Some(NullPart::Mark),
                Some(NullPart::Digit(1)),
            ]
        );
    }

    /// A chunk that is nothing but NUL bytes is one block and no other cell.
    #[test]
    fn a_chunk_of_nothing_but_nul_bytes_is_one_block() {
        let mut out = Vec::new();
        assert!(mark_runs(&[0, 0], &mut out));

        assert_eq!(
            parts(&out),
            vec![Some(NullPart::Mark), Some(NullPart::Digit(2))]
        );
    }
}
