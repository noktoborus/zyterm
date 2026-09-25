//! Modem control lines.

use serde::{Deserialize, Serialize};

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
}
