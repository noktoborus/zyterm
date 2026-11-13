//! Bytes as one line of text, the way the recorded sessions of `zyt-tmux`
//! write them, so a journal of the line plays through the same reader.

/// `\r`, `\n`, `\t`, `\e`, `\\`, and `\xHH` for every other byte under a
/// space, for `0x7f` and for a byte that is no part of UTF-8; every other
/// character as it is, so the text a device printed reads as text.
pub fn escape(bytes: &[u8], into: &mut Vec<u8>) {
    for chunk in bytes.utf8_chunks() {
        for character in chunk.valid().chars() {
            match character {
                '\r' => into.extend_from_slice(b"\\r"),
                '\n' => into.extend_from_slice(b"\\n"),
                '\t' => into.extend_from_slice(b"\\t"),
                '\x1b' => into.extend_from_slice(b"\\e"),
                '\\' => into.extend_from_slice(b"\\\\"),
                ' '..='~' => into.push(character as u8),
                '\x00'..='\x1f' | '\x7f' => hex(character as u8, into),
                _ => {
                    let mut buffer = [0u8; 4];
                    into.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
            }
        }
        for &byte in chunk.invalid() {
            hex(byte, into);
        }
    }
}

/// One byte as `\xHH`.
fn hex(byte: u8, into: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    into.extend_from_slice(&[
        b'\\',
        b'x',
        HEX[usize::from(byte >> 4)],
        HEX[usize::from(byte & 0xf)],
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escaped(bytes: &[u8]) -> String {
        let mut into = Vec::new();
        escape(bytes, &mut into);
        String::from_utf8(into).expect("escaped bytes are text")
    }

    #[test]
    fn the_bytes_of_a_line_of_tmux_read_as_text() {
        assert_eq!(
            escaped(b"\x1bP1000p%begin 1 2 0\r\n"),
            "\\eP1000p%begin 1 2 0\\r\\n"
        );
        assert_eq!(escaped(b"a\\b\t"), "a\\\\b\\t");
        assert_eq!(escaped(&[0x00, 0x7f]), "\\x00\\x7f");
        assert_eq!(escaped("ёж".as_bytes()), "ёж", "text stays text");
        assert_eq!(
            escaped(&[b'a', 0xd0, b'b']),
            "a\\xd0b",
            "a broken byte does not"
        );
    }
}
