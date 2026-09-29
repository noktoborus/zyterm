//! Modem control lines.

use serde::{Deserialize, Serialize};

/// What this side does with a line it drives.
///
/// Three answers and not two: the line is left to the driver, held down, or
/// held up. A driver raises both lines when the port opens and hardware flow
/// control drives `RTS` by itself, so "not held up" and "down" are not the same
/// thing, and a switch of two states cannot say which of them was meant.
///
/// [`LineHold::Auto`] writes nothing at all. A level a forced hold left on the
/// line stays there until the port is opened again: a driver takes its lines
/// over on open and no call hands one back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineHold {
    /// The driver drives the line; nothing is written from here.
    #[default]
    Auto,
    /// The line is held down from here.
    Down,
    /// The line is held up from here.
    Up,
}

impl LineHold {
    /// The level this hold writes, and nothing where it writes none.
    pub fn level(self) -> Option<bool> {
        match self {
            Self::Auto => None,
            Self::Down => Some(false),
            Self::Up => Some(true),
        }
    }

    /// Whether the line is driven from here rather than by the driver.
    pub fn is_forced(self) -> bool {
        self.level().is_some()
    }
}

/// What this side does with each of the two lines it drives.
///
/// The two are kept together because they are set together: a port is opened
/// with both of them, and a device that remembers one remembers the other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineHolds {
    /// What to do with Request To Send.
    #[serde(default)]
    pub rts: LineHold,
    /// What to do with Data Terminal Ready.
    #[serde(default)]
    pub dtr: LineHold,
}

/// Snapshot of the modem control lines of an open port.
///
/// The two lines this side drives are two answers and not one. What was asked
/// for is what this application did — `rts_asked`, `dtr_asked` — and what the
/// line is doing is what the driver says, which is not the same thing: opening
/// a port raises both of them before anything has asked for anything, and a
/// window that showed only what it asked for would call them down while they
/// stand up.
///
/// What the driver says is `None` where the platform has no way of saying it.
/// Windows sets those two lines and remembers them; it cannot read them back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLines {
    /// Request To Send, as this side asked for it.
    pub rts_asked: bool,
    /// Data Terminal Ready, as this side asked for it.
    pub dtr_asked: bool,
    /// Request To Send, as the driver reports it.
    pub rts: Option<bool>,
    /// Data Terminal Ready, as the driver reports it.
    pub dtr: Option<bool>,
    /// Clear To Send, driven by the peer.
    pub cts: bool,
    /// Data Set Ready, driven by the peer.
    pub dsr: bool,
    /// Data Carrier Detect, driven by the peer.
    pub cd: bool,
    /// Ring Indicator, driven by the peer.
    pub ri: bool,
}

impl ControlLines {
    /// Whether the Request To Send line is up, as far as anything knows: what
    /// the driver says, and what was asked for where it says nothing.
    pub fn rts_up(&self) -> bool {
        self.rts.unwrap_or(self.rts_asked)
    }

    /// Whether the Data Terminal Ready line is up, the same way.
    pub fn dtr_up(&self) -> bool {
        self.dtr.unwrap_or(self.dtr_asked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line this side drives is up when the driver says it is, and what was
    /// asked for is the answer only where the driver has none: opening a port
    /// raises both of them, so what was asked for is not what is.
    #[test]
    fn the_driver_answers_first_and_what_was_asked_for_answers_last() {
        let raised_by_the_driver = ControlLines {
            rts_asked: false,
            rts: Some(true),
            ..ControlLines::default()
        };
        assert!(raised_by_the_driver.rts_up());

        let asked_for_and_down = ControlLines {
            rts_asked: true,
            rts: Some(false),
            ..ControlLines::default()
        };
        assert!(!asked_for_and_down.rts_up(), "the line is what it is");

        let unread = ControlLines {
            dtr_asked: true,
            dtr: None,
            ..ControlLines::default()
        };
        assert!(
            unread.dtr_up(),
            "a platform that cannot read it back leaves what was asked for"
        );

        assert!(!ControlLines::default().rts_up());
        assert!(!ControlLines::default().dtr_up());
    }

    /// A hold says what is written to the line, and the automatic one writes
    /// nothing: a mode answering a level would drive the line it is there to
    /// leave alone.
    #[test]
    fn only_a_forced_hold_names_a_level() {
        assert_eq!(LineHold::default(), LineHold::Auto);
        assert_eq!(LineHold::Auto.level(), None);
        assert_eq!(LineHold::Down.level(), Some(false));
        assert_eq!(LineHold::Up.level(), Some(true));

        assert!(!LineHold::Auto.is_forced());
        assert!(LineHold::Down.is_forced());
        assert!(LineHold::Up.is_forced());
    }
}
