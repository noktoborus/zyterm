//! The frame of one character on the line: `8N1`, `7E1`, `8N2`.

use std::fmt;
use std::str::FromStr;

/// The parity bit of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    /// No parity bit.
    None,
    /// An even parity bit.
    Even,
    /// An odd parity bit.
    Odd,
}

/// One character on the line: a start bit, the data bits, a parity bit or
/// none, the stop bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Data bits, 5 to 8.
    pub data: u8,
    /// The parity bit.
    pub parity: Parity,
    /// Stop bits, 1 or 2.
    pub stop: u8,
}

impl Frame {
    /// The bits one byte takes on the line, start bit included.
    pub fn bits(self) -> u64 {
        let parity = u64::from(self.parity != Parity::None);
        1 + u64::from(self.data) + parity + u64::from(self.stop)
    }

    /// The bits of a byte a frame of fewer than eight data bits carries.
    pub fn mask(self) -> u8 {
        (0xffu16 >> (8 - self.data)) as u8
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            data: 8,
            parity: Parity::None,
            stop: 1,
        }
    }
}

impl FromStr for Frame {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wrong = || format!("{text}: a frame is data bits, N, E or O, and stop bits, as 8N1");
        let &[data, parity, stop] = text.as_bytes() else {
            return Err(wrong());
        };
        let data = match data {
            b'5'..=b'8' => data - b'0',
            _ => return Err(wrong()),
        };
        let parity = match parity.to_ascii_uppercase() {
            b'N' => Parity::None,
            b'E' => Parity::Even,
            b'O' => Parity::Odd,
            _ => return Err(wrong()),
        };
        let stop = match stop {
            b'1' | b'2' => stop - b'0',
            _ => return Err(wrong()),
        };
        Ok(Self { data, parity, stop })
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parity = match self.parity {
            Parity::None => 'N',
            Parity::Even => 'E',
            Parity::Odd => 'O',
        };
        write!(out, "{}{parity}{}", self.data, self.stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_read_and_count() {
        let frame: Frame = "8N1".parse().unwrap();
        assert_eq!((frame.bits(), frame.mask()), (10, 0xff));
        let frame: Frame = "7e2".parse().unwrap();
        assert_eq!((frame.bits(), frame.mask()), (11, 0x7f));
        assert_eq!(frame.to_string(), "7E2");
        assert_eq!("5N1".parse::<Frame>().unwrap().mask(), 0x1f);
    }

    #[test]
    fn wrong_frames_are_refused() {
        for text in ["", "8N", "9N1", "4N1", "8X1", "8N3", "8N1x"] {
            assert!(text.parse::<Frame>().is_err(), "{text}");
        }
    }
}
