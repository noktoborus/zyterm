//! The input held for the pane shown while a `send-keys` is on its way: bytes
//! as typed, and keys by their name in tmux, in the order they were typed.
//!
//! A key tmux knows by name — `Home`, `C-Up`, `F5` — goes by that name, so tmux
//! encodes it for the terminal of the pane as it does for a client that is not
//! in control mode. Its bytes are kept beside the name: input that leaves with
//! the mode is written to the line, and the shell that comes back reads bytes.

use std::collections::VecDeque;

use zyt_tmux::Keys;

/// One piece of input held.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    /// Bytes as typed.
    Bytes(Vec<u8>),
    /// A key by its name, and the bytes this terminal encodes it as.
    Named {
        /// Its name in tmux.
        name: String,
        /// Its bytes, for the line once the mode is gone.
        bytes: Vec<u8>,
    },
}

impl Piece {
    fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) | Self::Named { bytes, .. } => bytes.len(),
        }
    }
}

/// Input held, in the order typed.
#[derive(Debug, Default)]
pub struct Held {
    pieces: VecDeque<Piece>,
    bytes: usize,
}

/// A piece taken to be sent: what [`Keys`] borrows from.
#[derive(Debug, PartialEq, Eq)]
pub enum Taken {
    /// Bytes as typed.
    Bytes(Vec<u8>),
    /// A key by its name.
    Named(String),
}

impl Taken {
    /// The piece as `zyt-tmux` sends it.
    pub fn keys(&self) -> Keys<'_> {
        match self {
            Self::Bytes(bytes) => Keys::Bytes(bytes),
            Self::Named(name) => Keys::Named(name),
        }
    }
}

impl Held {
    /// Bytes typed; they join the bytes held last.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.bytes += bytes.len();
        if let Some(Piece::Bytes(last)) = self.pieces.back_mut() {
            last.extend_from_slice(bytes);
            return;
        }
        self.pieces.push_back(Piece::Bytes(bytes.to_vec()));
    }

    /// A key by its name, with the bytes it is in this terminal.
    pub fn push_named(&mut self, name: &str, bytes: &[u8]) {
        self.bytes += bytes.len();
        self.pieces.push_back(Piece::Named {
            name: name.to_string(),
            bytes: bytes.to_vec(),
        });
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// Bytes held, a key counted by its bytes.
    pub fn len(&self) -> usize {
        self.bytes
    }

    /// The pieces from the front, up to `limit` bytes; a run of bytes past it
    /// is cut, and a key is taken whole, at least one.
    pub fn take(&mut self, limit: usize) -> Vec<Taken> {
        let mut taken = Vec::new();
        let mut size = 0;
        while let Some(piece) = self.pieces.pop_front() {
            let room = limit.saturating_sub(size);
            match piece {
                Piece::Bytes(mut bytes) if bytes.len() > room => {
                    if room == 0 {
                        self.pieces.push_front(Piece::Bytes(bytes));
                        break;
                    }
                    let rest = bytes.split_off(room);
                    self.pieces.push_front(Piece::Bytes(rest));
                    size += bytes.len();
                    taken.push(Taken::Bytes(bytes));
                    break;
                }
                Piece::Named { .. } if !taken.is_empty() && piece.len() > room => {
                    self.pieces.push_front(piece);
                    break;
                }
                Piece::Bytes(bytes) => {
                    size += bytes.len();
                    taken.push(Taken::Bytes(bytes));
                }
                Piece::Named { name, bytes } => {
                    size += bytes.len();
                    taken.push(Taken::Named(name));
                }
            }
        }
        self.bytes -= size;
        taken
    }

    /// Everything held, as bytes: what the line gets once the mode is gone.
    pub fn take_bytes(&mut self) -> Vec<u8> {
        self.bytes = 0;
        self.pieces
            .drain(..)
            .flat_map(|piece| match piece {
                Piece::Bytes(bytes) | Piece::Named { bytes, .. } => bytes,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_join_and_a_key_stands_alone_in_its_place() {
        let mut held = Held::default();
        held.push_bytes(b"a");
        held.push_bytes(b"b");
        held.push_named("Home", b"\x1bOH");
        held.push_bytes(b"c");
        assert_eq!(held.len(), 6);
        assert_eq!(
            held.take(1024),
            [
                Taken::Bytes(b"ab".to_vec()),
                Taken::Named("Home".to_string()),
                Taken::Bytes(b"c".to_vec()),
            ]
        );
        assert!(held.is_empty());
        assert_eq!(held.len(), 0, "and no bytes counted");
    }

    #[test]
    fn a_piece_is_cut_at_the_limit_and_a_key_is_never_cut() {
        let mut held = Held::default();
        held.push_bytes(b"abcd");
        held.push_named("End", b"\x1bOF");
        assert_eq!(held.take(2), [Taken::Bytes(b"ab".to_vec())]);
        assert_eq!(held.take(3), [Taken::Bytes(b"cd".to_vec())]);
        assert_eq!(
            held.take(1),
            [Taken::Named("End".to_string())],
            "a key larger than the limit still goes, alone"
        );
        assert!(held.is_empty());
    }

    #[test]
    fn what_leaves_with_the_mode_is_bytes() {
        let mut held = Held::default();
        held.push_bytes(b"ls");
        held.push_named("Home", b"\x1b[H");
        assert_eq!(held.take_bytes(), b"ls\x1b[H");
        assert!(held.is_empty());
        assert_eq!(held.len(), 0, "and no bytes counted");
    }
}
