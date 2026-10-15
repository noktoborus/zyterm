//! The escapes of `%output`: a byte below `0x20` and `\` come as `\ooo`.

use memchr::memchr;

/// Decodes `\ooo` in place and returns the length of what is decoded.
///
/// Four bytes become one, so the write never passes the read. A `\` not
/// followed by three octal digits is kept as it is: tmux writes none, and a
/// byte lost on a port is no reason to lose the rest of the line.
pub fn decode(bytes: &mut [u8]) -> usize {
    let Some(first) = memchr(b'\\', bytes) else {
        return bytes.len();
    };
    let mut read = first;
    let mut written = first;
    while read < bytes.len() {
        let byte = match octal(&bytes[read..]) {
            Some(byte) => {
                read += 4;
                byte
            }
            None => {
                read += 1;
                bytes[read - 1]
            }
        };
        bytes[written] = byte;
        written += 1;
    }
    written
}

/// The byte of `\ooo` at the start of `bytes`.
fn octal(bytes: &[u8]) -> Option<u8> {
    let [b'\\', digits @ ..] = bytes.get(..4)? else {
        return None;
    };
    digits
        .iter()
        .try_fold(0u16, |value, &digit| match digit {
            b'0'..=b'7' => Some(value * 8 + u16::from(digit - b'0')),
            _ => None,
        })
        .and_then(|value| u8::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(text: &[u8]) -> Vec<u8> {
        let mut bytes = text.to_vec();
        let length = decode(&mut bytes);
        bytes.truncate(length);
        bytes
    }

    #[test]
    fn octal_escapes_become_bytes() {
        assert_eq!(decoded(b"ls\\015\\012"), b"ls\r\n");
        assert_eq!(decoded(b"a\\134b"), b"a\\b");
        assert_eq!(decoded(b"\\033[1m"), b"\x1b[1m");
        assert_eq!(decoded(b"end\\007"), b"end\x07");
    }

    #[test]
    fn utf8_passes_as_it_is() {
        assert_eq!(decoded("ќ жук\\015".as_bytes()), "ќ жук\r".as_bytes());
        assert_eq!(decoded(b"\xd1\x9c"), b"\xd1\x9c");
    }

    #[test]
    fn a_broken_escape_is_kept() {
        assert_eq!(decoded(b"a\\01"), b"a\\01");
        assert_eq!(decoded(b"a\\9xyz"), b"a\\9xyz");
        assert_eq!(decoded(b"\\777"), b"\\777");
        assert_eq!(decoded(b"\\"), b"\\");
    }
}
