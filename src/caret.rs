//! Caret notation: `^C` in a text, the byte 0x03 on the line.
//!
//! A control code is a byte no keyboard writes and no field holds, and the
//! block of several lines is where one is most often wanted: a script that ends
//! with an interrupt, a menu of a bootloader that is left with an escape, a
//! shell fed a line and then the end of its input. So the block is written in
//! the notation every terminal already prints such a byte in, and the plate
//! turns it into the byte before the block is sent.
//!
//! The notation is the one of `stty` and of a terminal's own output:
//!
//! | written | byte | |
//! | --- | --- | --- |
//! | `^@` | 0x00 | NUL |
//! | `^A` … `^Z` | 0x01 … 0x1a | the letter, minus 0x40 |
//! | `^[` | 0x1b | Esc |
//! | `^\` | 0x1c | FS |
//! | `^]` | 0x1d | GS |
//! | `^^` | 0x1e | RS |
//! | `^_` | 0x1f | US |
//! | `^?` | 0x7f | Delete |
//!
//! A lower case letter stands for the same byte as its upper case, because that
//! is how the key is pressed.
//!
//! A caret that no such character follows stands as it was written, and so does
//! the character after it. There is no way to escape a caret, because every way
//! of escaping one takes the notation further from what a terminal prints: the
//! switch of the plate is how a block with carets in it is sent whole.

/// The text with every caret sequence of the table replaced by its byte.
pub fn expand(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        if character != '^' {
            out.push(character);
            continue;
        }
        match chars.peek().copied().and_then(code) {
            Some(byte) => {
                chars.next();
                out.push(byte);
            }
            None => out.push('^'),
        }
    }

    out
}

/// The control character one character stands for in that notation.
fn code(after: char) -> Option<char> {
    let point = match after {
        '@'..='_' => after as u32 - 0x40,
        'a'..='z' => after.to_ascii_uppercase() as u32 - 0x40,
        '?' => 0x7f,
        _ => return None,
    };
    char::from_u32(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_becomes_the_control_code_of_that_key() {
        assert_eq!(expand("^C"), "\u{3}");
        assert_eq!(expand("^d"), "\u{4}");
        assert_eq!(expand("^J"), "\n");
        assert_eq!(expand("^M"), "\r");
    }

    #[test]
    fn the_punctuation_of_the_table_becomes_its_own_byte() {
        assert_eq!(expand("^@"), "\u{0}");
        assert_eq!(expand("^["), "\u{1b}");
        assert_eq!(expand("^\\"), "\u{1c}");
        assert_eq!(expand("^]"), "\u{1d}");
        assert_eq!(expand("^^"), "\u{1e}");
        assert_eq!(expand("^_"), "\u{1f}");
        assert_eq!(expand("^?"), "\u{7f}");
    }

    #[test]
    fn a_sequence_stands_where_it_was_written() {
        assert_eq!(expand("echo done\r^C"), "echo done\r\u{3}");
    }

    #[test]
    fn a_caret_nothing_of_the_table_follows_is_left_alone() {
        assert_eq!(expand("2^3"), "2^3");
        assert_eq!(expand("end^"), "end^");
    }

    #[test]
    fn a_text_with_no_caret_in_it_is_itself() {
        assert_eq!(expand("ls -la\rexit\r"), "ls -la\rexit\r");
    }
}
